//! A live mech: definition, owner, pose, orders and mutable voxels.

use super::components::{ComponentId, MechComponent};
use super::definition::{build_test_mech, MechDefinition};
use super::input::MechCommand;
use super::orders::{MechOrders, MoveOrder, RotationOrder};
use super::voxel::{RenderBox, VoxelGrid};
use uuid::Uuid;

pub const FORWARD_SPEED: f64 = 1.5;
pub const REVERSE_SPEED: f64 = 0.75;
pub const ACCELERATION: f64 = 0.5;
pub const BODY_DEGREES_PER_TICK: f32 = 5.0;
pub const TORSO_DEGREES_PER_TICK: f32 = 10.0;
pub const TORSO_LIMIT_DEGREES: f32 = 45.0;
pub const ARENA_LIMIT: f64 = 128.0;
/// Peak thigh pitch (forward/back swing) while walking, in radians.
pub const THIGH_SWING_RADIANS: f32 = 32.0_f32.to_radians();
/// Peak knee bend while the foot is being carried forward, in radians.
pub const KNEE_BEND_RADIANS: f32 = 55.0_f32.to_radians();
/// One full gait cycle (both legs) is covered after this many blocks walked.
pub const STRIDE_BLOCKS: f32 = 2.0;
/// How far the body must travel before a new leg pose is emitted. Its
/// distance-gated, so at full speed the legs change ~2 times per second and
/// the sync traffic stays far below the outbound queue limits.
pub const GAIT_POSE_DISTANCE: f32 = 0.75;

#[derive(Clone, Debug)]
pub struct MechInstance {
    pub id: u64,
    pub owner: Uuid,
    /// Base centre on the ground. Local +Y is up, +Z forward.
    pub position: [f64; 3],
    pub body_yaw: f32,
    pub torso_yaw: f32,
    pub speed: f64,
    pub speed_limit: f64,
    move_accumulator: f64,
    /// Signed distance walked along the current heading; drives the gait.
    pub stride: f32,
    /// Thigh pitch (radians, about the hip axis): [left, right]. Positive
    /// pitches the leg forward. The renderer rotates leg voxels around the
    /// hip joint instead of sliding them.
    pub thigh_pitch: [f32; 2],
    /// Knee bend (radians) for the shin/foot segment, per side.
    pub knee_pitch: [f32; 2],
    /// The `stride` value at which the joint pose was last recomputed. The
    /// pose only refreshes after the body has travelled `GAIT_POSE_DISTANCE`
    /// blocks, so walking traffic scales with distance, not with wall-clock
    /// ticks. This eliminates the per-tick leg jitter that filled the
    /// outbound queue.
    last_pose_stride: f32,
    pub orders: MechOrders,
    pub fire_once: bool,
    pub definition: MechDefinition,
    pub alive: bool,
}

impl MechInstance {
    pub fn spawn(id: u64, owner: Uuid, position: [f64; 3]) -> Self {
        Self {
            id,
            owner,
            position: [
                position[0].round(),
                position[1].round(),
                position[2].round(),
            ],
            body_yaw: 0.0,
            torso_yaw: 0.0,
            speed: 0.0,
            speed_limit: FORWARD_SPEED,
            move_accumulator: 0.0,
            stride: 0.0,
            thigh_pitch: [0.0, 0.0],
            knee_pitch: [0.0, 0.0],
            last_pose_stride: 0.0,
            orders: MechOrders::default(),
            fire_once: false,
            definition: build_test_mech(),
            alive: true,
        }
    }

    pub fn grid(&self) -> &VoxelGrid {
        &self.definition.grid
    }

    pub fn grid_mut(&mut self) -> &mut VoxelGrid {
        &mut self.definition.grid
    }

    pub fn component(&self, id: ComponentId) -> Option<&MechComponent> {
        self.definition.component(id)
    }

    pub fn component_health(&self, id: ComponentId) -> u16 {
        self.definition.component(id).map(|c| c.health).unwrap_or(0)
    }

    pub fn apply_command(&mut self, command: MechCommand) -> bool {
        match command {
            MechCommand::MoveForward => self.orders.movement = MoveOrder::Forward,
            MechCommand::MoveBackward => self.orders.movement = MoveOrder::Backward,
            MechCommand::MoveStop => self.orders.movement = MoveOrder::Stop,
            MechCommand::TurnLeft => self.orders.body_rotation = RotationOrder::Left,
            MechCommand::TurnRight => self.orders.body_rotation = RotationOrder::Right,
            MechCommand::TurnStop => self.orders.body_rotation = RotationOrder::Stop,
            MechCommand::SpeedSlow => self.speed_limit = FORWARD_SPEED * 0.5,
            MechCommand::SpeedNormal => self.speed_limit = FORWARD_SPEED,
            MechCommand::TorsoLeft => {
                self.orders.torso_reset = false;
                self.orders.torso_rotation = RotationOrder::Left;
            }
            MechCommand::TorsoRight => {
                self.orders.torso_reset = false;
                self.orders.torso_rotation = RotationOrder::Right;
            }
            MechCommand::TorsoStop => self.orders.torso_rotation = RotationOrder::Stop,
            MechCommand::TorsoReset => {
                self.orders.torso_rotation = RotationOrder::Stop;
                self.orders.torso_reset = true;
            }
            MechCommand::FireOnce => self.fire_once = true,
            MechCommand::AutoFireOn => self.orders.auto_fire = true,
            MechCommand::AutoFireOff => self.orders.auto_fire = false,
            MechCommand::EmergencyStop => {
                self.orders.emergency_stop();
                self.fire_once = false;
            }
            MechCommand::ResetCommands => {
                self.orders = MechOrders::default();
                self.fire_once = false;
            }
            MechCommand::SystemStatus => return false,
        }
        true
    }

    /// Advances movement and rotation by `dt` seconds. Rotation constants are
    /// in degrees per server tick at 20 TPS.
    pub fn tick(&mut self, dt: f32) {
        if !self.alive {
            return;
        }
        let ticks = f64::from(dt) * 20.0;
        let body_step = f64::from(BODY_DEGREES_PER_TICK) * ticks;
        match self.orders.body_rotation {
            RotationOrder::Left => self.body_yaw += body_step as f32,
            RotationOrder::Right => self.body_yaw -= body_step as f32,
            RotationOrder::Stop => {}
        }
        self.body_yaw = normalize_degrees(self.body_yaw);

        let torso_step = f64::from(TORSO_DEGREES_PER_TICK) * ticks;
        if self.orders.torso_reset {
            let step = torso_step as f32;
            if self.torso_yaw.abs() <= step {
                self.torso_yaw = 0.0;
                self.orders.torso_reset = false;
            } else {
                self.torso_yaw -= self.torso_yaw.signum() * step;
            }
        } else {
            match self.orders.torso_rotation {
                RotationOrder::Left => self.torso_yaw += torso_step as f32,
                RotationOrder::Right => self.torso_yaw -= torso_step as f32,
                RotationOrder::Stop => {}
            }
            self.torso_yaw = self
                .torso_yaw
                .clamp(-TORSO_LIMIT_DEGREES, TORSO_LIMIT_DEGREES);
        }

        let target = match self.orders.movement {
            MoveOrder::Forward => self.speed_limit,
            MoveOrder::Backward => -self.speed_limit * (REVERSE_SPEED / FORWARD_SPEED),
            MoveOrder::Stop => 0.0,
        };
        let rate = if target == 0.0 {
            ACCELERATION * 1.5
        } else {
            ACCELERATION
        };
        let delta = target - self.speed;
        let step = rate * f64::from(dt);
        if delta.abs() <= step {
            self.speed = target;
        } else {
            self.speed += delta.signum() * step;
        }

        // Movement is quantized to whole-block steps. The accumulator keeps
        // the configured speed independent of the 45-degree body direction.
        self.move_accumulator += self.speed * f64::from(dt);
        while self.move_accumulator.abs() >= 1.0 {
            let yaw = f64::from(quantize45(self.body_yaw)).to_radians();
            let (dx, dz) = ((-yaw.sin()).round() as i32, yaw.cos().round() as i32);
            if dx == 0 && dz == 0 {
                break;
            }
            let length = (f64::from(dx * dx + dz * dz)).sqrt();
            if self.move_accumulator >= 0.0 {
                self.position[0] += f64::from(dx);
                self.position[2] += f64::from(dz);
                self.move_accumulator -= length;
            } else {
                self.position[0] -= f64::from(dx);
                self.position[2] -= f64::from(dz);
                self.move_accumulator += length;
            }
            if self.position[0].abs() > ARENA_LIMIT || self.position[2].abs() > ARENA_LIMIT {
                self.position[0] = self.position[0].clamp(-ARENA_LIMIT, ARENA_LIMIT);
                self.position[2] = self.position[2].clamp(-ARENA_LIMIT, ARENA_LIMIT);
                self.speed = 0.0;
                self.move_accumulator = 0.0;
            }
        }

        self.update_gait(dt);
    }

    /// Advances the gait from distance travelled. One full cycle takes
    /// `STRIDE_BLOCKS` blocks: the left and right thighs pitch in opposition
    /// (one forward, one back) and the knee bends to carry the foot forward,
    /// like a real two-segment leg. The pose only refreshes every
    /// `GAIT_POSE_DISTANCE` blocks, so the leg sync traffic tracks the body's
    /// steps instead of generating a continuous per-tick stream. When
    /// stopped, the joints ease to rest.
    fn update_gait(&mut self, dt: f32) {
        if self.speed.abs() > 0.001 {
            self.stride += self.speed as f32 * dt;
            if (self.stride - self.last_pose_stride).abs() < GAIT_POSE_DISTANCE {
                return;
            }
            self.last_pose_stride = self.stride;
            let cycle = self.stride / STRIDE_BLOCKS;
            let left = cycle * std::f32::consts::TAU;
            let right = left + std::f32::consts::PI;
            self.thigh_pitch[0] = THIGH_SWING_RADIANS * left.sin();
            self.thigh_pitch[1] = THIGH_SWING_RADIANS * right.sin();
            self.knee_pitch[0] = KNEE_BEND_RADIANS * left.sin().max(0.0);
            self.knee_pitch[1] = KNEE_BEND_RADIANS * right.sin().max(0.0);
        } else {
            for pitch in [&mut self.thigh_pitch, &mut self.knee_pitch] {
                for v in pitch {
                    if v.abs() < 0.02 {
                        *v = 0.0;
                    } else {
                        *v *= 0.7;
                    }
                }
            }
            if self.thigh_pitch == [0.0, 0.0] && self.knee_pitch == [0.0, 0.0] {
                self.stride = 0.0;
            }
        }
    }

    /// World-space centre of a grouped local box, optionally including torso
    /// rotation. The y coordinate is the box bottom.
    pub fn world_box_center(&self, b: &RenderBox, with_torso: bool) -> [f64; 3] {
        let [sx, _sy, sz] = self.definition.dims;
        let mut local = [
            b.min[0] as f64 + b.size[0] as f64 / 2.0 - sx as f64 / 2.0,
            b.min[1] as f64,
            b.min[2] as f64 + b.size[2] as f64 / 2.0 - sz as f64 / 2.0,
        ];
        if with_torso && self.torso_yaw != 0.0 {
            let pivot = [0.0, 0.0, 0.0];
            let angle = f64::from(self.torso_yaw).to_radians();
            let dx = local[0] - pivot[0];
            let dz = local[2] - pivot[2];
            local[0] = pivot[0] + dx * angle.cos() - dz * angle.sin();
            local[2] = pivot[2] + dx * angle.sin() + dz * angle.cos();
        }
        let body = f64::from(self.body_yaw).to_radians();
        let dx = local[0];
        let dz = local[2];
        [
            self.position[0] + dx * body.cos() - dz * body.sin(),
            self.position[1] + local[1],
            self.position[2] + dx * body.sin() + dz * body.cos(),
        ]
    }

    pub fn total_yaw(&self, with_torso: bool) -> f32 {
        if with_torso {
            self.body_yaw + self.torso_yaw
        } else {
            self.body_yaw
        }
    }

    pub fn mark_destroyed(&mut self) {
        self.alive = false;
    }
}

fn quantize45(yaw: f32) -> f32 {
    (yaw / 45.0).round() * 45.0
}

fn normalize_degrees(mut degrees: f32) -> f32 {
    while degrees >= 360.0 {
        degrees -= 360.0;
    }
    while degrees < 0.0 {
        degrees += 360.0;
    }
    degrees
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_centres_the_model() {
        let mech = MechInstance::spawn(1, Uuid::nil(), [100.0, 65.0, 200.0]);
        assert!(mech.alive);
        assert_eq!(mech.grid().dims, [16, 34, 10]);
    }

    #[test]
    fn forward_order_accelerates_and_stops() {
        let mut mech = MechInstance::spawn(1, Uuid::nil(), [0.0, 65.0, 0.0]);
        mech.apply_command(MechCommand::MoveForward);
        for _ in 0..40 {
            mech.tick(0.05);
        }
        assert!(mech.speed > 0.5, "speed {}", mech.speed);
        let z = mech.position[2];
        assert!(z > 0.2, "moved {z}");
        mech.apply_command(MechCommand::MoveStop);
        for _ in 0..100 {
            mech.tick(0.05);
        }
        assert!(mech.speed.abs() < 0.001);
    }

    #[test]
    fn torso_rotation_is_clamped_and_resets() {
        let mut mech = MechInstance::spawn(1, Uuid::nil(), [0.0, 65.0, 0.0]);
        mech.apply_command(MechCommand::TorsoLeft);
        for _ in 0..100 {
            mech.tick(0.05);
        }
        assert!((mech.torso_yaw - TORSO_LIMIT_DEGREES).abs() < 0.001);
        mech.apply_command(MechCommand::TorsoReset);
        for _ in 0..100 {
            mech.tick(0.05);
        }
        assert!(mech.torso_yaw.abs() < 0.001);
    }

    #[test]
    fn walking_gait_swings_legs_in_opposition() {
        let mut mech = MechInstance::spawn(1, Uuid::nil(), [0.0, 65.0, 0.0]);
        mech.apply_command(MechCommand::MoveForward);
        let mut opposed = false;
        let mut knee_bent = false;
        for _ in 0..400 {
            mech.tick(0.05);
            if mech.thigh_pitch[0] * mech.thigh_pitch[1] < 0.0
                && mech.thigh_pitch[0].abs() > 0.2
                && mech.thigh_pitch[1].abs() > 0.2
            {
                opposed = true;
            }
            if mech.knee_pitch.iter().any(|k| *k > 0.2) {
                knee_bent = true;
            }
        }
        assert!(mech.speed > 0.5, "speed {}", mech.speed);
        assert!(
            opposed,
            "legs never swung in opposition: {:?}",
            mech.thigh_pitch
        );
        assert!(knee_bent, "knee never bent: {:?}", mech.knee_pitch);
    }

    #[test]
    fn emergency_stop_clears_orders() {
        let mut mech = MechInstance::spawn(1, Uuid::nil(), [0.0, 65.0, 0.0]);
        mech.apply_command(MechCommand::MoveForward);
        mech.apply_command(MechCommand::AutoFireOn);
        mech.apply_command(MechCommand::EmergencyStop);
        assert_eq!(mech.orders.movement, MoveOrder::Stop);
        assert!(!mech.orders.auto_fire);
    }
}
