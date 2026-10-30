//! One 1x1x1 block-display entity per visible shell voxel.
//!
//! The renderer keeps displays axis-aligned to the world block grid. Body and
//! torso yaw are quantized to 45 degrees and the voxel centres are re-rasterized onto
//! integer cells, so the result is always a block-built mech.

use arcade_protocol::packets::play::clientbound::{
    EntityPositionSync, RemoveEntities, SetEntityData, SpawnEntity,
};
use arcade_protocol::PacketWriter;
use arcade_protocol_data::GameData;
use uuid::Uuid;

use crate::mech::{shell_voxels, ComponentId, Material, MechInstance};

/// The plan's first-renderer limit was 500. A one-layer shell of the current
/// model is larger, so the experimental limit is raised; two mechs still fit
/// comfortably in the intended two-player arena.
pub const MAX_DISPLAYS_PER_MECH: usize = 700;

const META_BLOCK_STATE: u8 = 23;
const META_TYPE_BLOCK_STATE: i32 = 14;

pub struct DisplayEntity {
    pub entity_id: i32,
    pub uuid: Uuid,
    pub component: ComponentId,
    pub material: Material,
    /// Source voxel coordinate in the unrotated model.
    pub local: [usize; 3],
    last_cell: [i32; 3],
}

pub struct MechRenderer {
    pub mech_id: u64,
    pub displays: Vec<DisplayEntity>,
}

pub struct RenderSpawn {
    pub renderer: MechRenderer,
    pub packets: Vec<Vec<u8>>,
}

pub fn spawn(
    data: &GameData,
    mech: &MechInstance,
    next_entity_id: &mut dyn FnMut() -> i32,
) -> Result<RenderSpawn, String> {
    let entity_type = data
        .entity_type_id("block_display")
        .ok_or_else(|| "no block_display entity type".to_owned())?;
    let cells = shell_voxels(mech.grid());
    if cells.len() > MAX_DISPLAYS_PER_MECH {
        return Err(format!(
            "mech needs {} display entities (limit {MAX_DISPLAYS_PER_MECH})",
            cells.len()
        ));
    }
    let mut displays = Vec::with_capacity(cells.len());
    let mut packets = Vec::with_capacity(cells.len() * 2);
    for (x, y, z, voxel) in cells {
        let local = [x, y, z];
        let cell = cell_for(mech, ComponentId::from_id(voxel.component_id), local);
        let entity_id = next_entity_id();
        let uuid = Uuid::new_v4();
        packets.push(
            data.packet_ids
                .encode(&SpawnEntity {
                    entity_id,
                    uuid,
                    entity_type,
                    x: f64::from(cell[0]),
                    y: f64::from(cell[1]),
                    z: f64::from(cell[2]),
                    velocity: (0.0, 0.0, 0.0),
                    pitch: 0.0,
                    yaw: 0.0,
                    head_yaw: 0.0,
                    data: 0,
                })
                .map_err(|e| e.to_string())?,
        );
        packets.push(block_state_packet(data, entity_id, voxel.material())?);
        displays.push(DisplayEntity {
            entity_id,
            uuid,
            component: ComponentId::from_id(voxel.component_id),
            material: voxel.material(),
            local,
            last_cell: cell,
        });
    }
    Ok(RenderSpawn {
        renderer: MechRenderer {
            mech_id: mech.id,
            displays,
        },
        packets,
    })
}

/// Re-rasterizes every display after grid movement or a 45 degrees pose change.
pub fn sync(data: &GameData, mech: &MechInstance, renderer: &mut MechRenderer) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    for display in &mut renderer.displays {
        let cell = cell_for(mech, display.component, display.local);
        if cell == display.last_cell {
            continue;
        }
        if let Ok(bytes) = data.packet_ids.encode(&EntityPositionSync {
            entity_id: display.entity_id,
            x: f64::from(cell[0]),
            y: f64::from(cell[1]),
            z: f64::from(cell[2]),
            yaw: 0.0,
            pitch: 0.0,
            on_ground: true,
        }) {
            packets.push(bytes);
        }
        display.last_cell = cell;
    }
    packets
}

pub fn despawn(data: &GameData, renderer: &MechRenderer) -> Result<Vec<u8>, String> {
    data.packet_ids
        .encode(&RemoveEntities {
            entity_ids: renderer.displays.iter().map(|d| d.entity_id).collect(),
        })
        .map_err(|e| e.to_string())
}

/// Absolute integer cell for a source voxel after grid-snapped body/torso yaw.
fn cell_for(mech: &MechInstance, component: ComponentId, local: [usize; 3]) -> [i32; 3] {
    let [sx, _sy, sz] = mech.definition.dims;
    let torso = matches!(
        component,
        ComponentId::Torso
            | ComponentId::Head
            | ComponentId::LeftArm
            | ComponentId::RightArm
            | ComponentId::Reactor
            | ComponentId::MainWeapon
    );
    let mut x = local[0] as f64 + 0.5 - sx as f64 / 2.0;
    let mut z = local[2] as f64 + 0.5 - sz as f64 / 2.0;
    if torso {
        let angle = f64::from(quantize45(mech.torso_yaw)).to_radians();
        let (nx, nz) = (
            x * angle.cos() - z * angle.sin(),
            x * angle.sin() + z * angle.cos(),
        );
        x = nx;
        z = nz;
    }
    let angle = f64::from(quantize45(mech.body_yaw)).to_radians();
    let (nx, nz) = (
        x * angle.cos() - z * angle.sin(),
        x * angle.sin() + z * angle.cos(),
    );
    let bx = mech.position[0].round() as i32;
    let bz = mech.position[2].round() as i32;
    let by = mech.position[1].round() as i32;
    // Snap the rotated centre to the nearest cell corner.
    [
        bx + (nx - 0.5).round() as i32,
        by + local[1] as i32,
        bz + (nz - 0.5).round() as i32,
    ]
}

fn quantize45(yaw: f32) -> f32 {
    (yaw / 45.0).round() * 45.0
}

fn block_state_packet(
    data: &GameData,
    entity_id: i32,
    material: Material,
) -> Result<Vec<u8>, String> {
    let state = data
        .blocks
        .state(material.block_name())
        .map(|s| s as i32)
        .ok_or_else(|| format!("no block state for {}", material.block_name()))?;
    let mut value = PacketWriter::new();
    value.write_varint(state);
    data.packet_ids
        .encode(&SetEntityData {
            entity_id,
            entries: vec![(META_BLOCK_STATE, META_TYPE_BLOCK_STATE, value.into_inner())],
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mech::{MechCommand, MechInstance};
    use std::path::Path;

    fn data() -> GameData {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        GameData::load(&dir).unwrap()
    }

    #[test]
    fn spawn_uses_one_display_per_shell_voxel() {
        let data = data();
        let mech = MechInstance::spawn(1, Uuid::nil(), [0.0, 65.0, 0.0]);
        let mut id = 100;
        let spawned = spawn(&data, &mech, &mut || {
            id += 1;
            id
        })
        .unwrap();
        assert_eq!(
            spawned.renderer.displays.len(),
            shell_voxels(mech.grid()).len()
        );
        assert_eq!(spawned.packets.len(), spawned.renderer.displays.len() * 2);
        assert!(spawned.renderer.displays.len() <= MAX_DISPLAYS_PER_MECH);
    }

    #[test]
    fn moving_one_grid_cell_updates_displays() {
        let data = data();
        let mut mech = MechInstance::spawn(1, Uuid::nil(), [0.0, 65.0, 0.0]);
        let mut id = 100;
        let mut spawned = spawn(&data, &mech, &mut || {
            id += 1;
            id
        })
        .unwrap();
        mech.apply_command(MechCommand::MoveForward);
        for _ in 0..80 {
            mech.tick(0.05);
        }
        let updates = sync(&data, &mech, &mut spawned.renderer);
        assert!(!updates.is_empty());
    }
}
