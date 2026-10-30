//! Persistent mech orders.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MoveOrder {
    #[default]
    Stop,
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RotationOrder {
    #[default]
    Stop,
    Left,
    Right,
}

#[derive(Clone, Debug, Default)]
pub struct MechOrders {
    pub movement: MoveOrder,
    pub body_rotation: RotationOrder,
    pub torso_rotation: RotationOrder,
    pub torso_reset: bool,
    pub auto_fire: bool,
}

impl MechOrders {
    pub fn emergency_stop(&mut self) {
        self.movement = MoveOrder::Stop;
        self.body_rotation = RotationOrder::Stop;
        self.torso_rotation = RotationOrder::Stop;
        self.torso_reset = false;
        self.auto_fire = false;
    }
}
