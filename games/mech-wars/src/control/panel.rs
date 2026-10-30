//! Physical control panel rendered with block/text displays and invisible
//! Interaction entities. Clicking an interaction maps to a `MechCommand`.

use arcade_protocol::packets::play::clientbound::{RemoveEntities, SetEntityData, SpawnEntity};
use arcade_protocol::{PacketWriter, Text};
use arcade_protocol_data::GameData;
use uuid::Uuid;

use crate::mech::{MechCommand, MVP_COMMANDS};

const SHORT_LABELS: [&str; 18] = [
    "FWD", "BACK", "STOP", "BODY<", "BODY>", "TSTOP", "SLOW", "NORM", "TORSO<", "TORSO>", "TSTOP",
    "TRST", "FIRE", "AUTO+", "AUTO-", "E-STOP", "STAT", "RST",
];

const META_TRANSLATION: u8 = 11;
const META_SCALE: u8 = 12;
const META_LEFT_ROTATION: u8 = 13;
const META_BILLBOARD: u8 = 15;
const META_BLOCK_STATE: u8 = 23;
const META_TEXT: u8 = 23;
const META_TEXT_FLAGS: u8 = 27;
const META_INTERACTION_WIDTH: u8 = 8;
const META_INTERACTION_HEIGHT: u8 = 9;
const META_INTERACTION_RESPONSIVE: u8 = 10;

const TYPE_BYTE: i32 = 0;
const TYPE_FLOAT: i32 = 3;
const TYPE_TEXT: i32 = 5;
const TYPE_BOOLEAN: i32 = 8;
const TYPE_BLOCK_STATE: i32 = 14;
const TYPE_VECTOR3: i32 = 39;
const TYPE_QUATERNION: i32 = 40;

pub struct PanelButton {
    pub entity_id: i32,
    pub command: MechCommand,
}

pub struct ControlPanel {
    pub owner: Uuid,
    pub buttons: Vec<PanelButton>,
    pub display_ids: Vec<i32>,
    pub packets: Vec<Vec<u8>>,
    pub anchor: [f64; 3],
    pub yaw: f32,
}

impl ControlPanel {
    pub fn command_for(&self, entity_id: i32) -> Option<MechCommand> {
        self.buttons
            .iter()
            .find(|b| b.entity_id == entity_id)
            .map(|b| b.command)
    }

    pub fn remove_packet(&self, data: &GameData) -> Option<Vec<u8>> {
        data.packet_ids
            .encode(&RemoveEntities {
                entity_ids: self.display_ids.clone(),
            })
            .ok()
    }
}

pub fn spawn(
    data: &GameData,
    owner: Uuid,
    anchor: [f64; 3],
    yaw: f32,
    next_entity_id: &mut dyn FnMut() -> i32,
) -> Result<ControlPanel, String> {
    let block_type = data
        .entity_type_id("block_display")
        .ok_or("no block_display type")?;
    let text_type = data
        .entity_type_id("text_display")
        .ok_or("no text_display type")?;
    let interaction_type = data
        .entity_type_id("interaction")
        .ok_or("no interaction type")?;

    let mut packets = Vec::new();
    let mut display_ids = Vec::new();
    let mut buttons = Vec::new();

    // Backboard.
    let board_id = next_entity_id();
    display_ids.push(board_id);
    push_block_display(
        data,
        &mut packets,
        board_id,
        anchor,
        [4.4, 2.2, 0.2],
        block_state(data, "blackstone")?,
        yaw,
    )?;

    for (i, command) in MVP_COMMANDS.iter().enumerate() {
        let col = i % 6;
        let row = i / 6;
        let local_u = -1.65 + col as f64 * 0.66;
        let local_v = 0.65 - row as f64 * 0.62;
        let [bx, by, bz] = rotate_point(anchor, yaw, [local_u, local_v, -0.25]);

        let display_id = next_entity_id();
        display_ids.push(display_id);
        push_block_display(
            data,
            &mut packets,
            display_id,
            [bx, by, bz],
            [0.4, 0.4, 0.4],
            block_state(data, "copper_block")?,
            yaw,
        )?;

        let interaction_id = next_entity_id();
        display_ids.push(interaction_id);
        let spawn = data.packet_ids.encode(&SpawnEntity {
            entity_id: interaction_id,
            uuid: Uuid::new_v4(),
            entity_type: interaction_type,
            x: bx,
            y: by,
            z: bz,
            velocity: (0.0, 0.0, 0.0),
            pitch: 0.0,
            yaw: 0.0,
            head_yaw: 0.0,
            data: 0,
        });
        packets.push(spawn.map_err(|e| e.to_string())?);
        let meta = data.packet_ids.encode(&SetEntityData {
            entity_id: interaction_id,
            entries: vec![
                (META_INTERACTION_WIDTH, TYPE_FLOAT, f32_bytes(0.55)),
                (META_INTERACTION_HEIGHT, TYPE_FLOAT, f32_bytes(0.55)),
                (META_INTERACTION_RESPONSIVE, TYPE_BOOLEAN, vec![1]),
            ],
        });
        packets.push(meta.map_err(|e| e.to_string())?);

        let [tx, ty, tz] = rotate_point(anchor, yaw, [local_u, local_v + 0.32, -0.3]);
        let text_id = next_entity_id();
        display_ids.push(text_id);
        let spawn = data.packet_ids.encode(&SpawnEntity {
            entity_id: text_id,
            uuid: Uuid::new_v4(),
            entity_type: text_type,
            x: tx,
            y: ty,
            z: tz,
            velocity: (0.0, 0.0, 0.0),
            pitch: 0.0,
            yaw: 0.0,
            head_yaw: 0.0,
            data: 0,
        });
        packets.push(spawn.map_err(|e| e.to_string())?);
        let meta = data.packet_ids.encode(&SetEntityData {
            entity_id: text_id,
            entries: vec![
                (META_SCALE, TYPE_VECTOR3, vector3_bytes(0.22)),
                (META_BILLBOARD, TYPE_BYTE, vec![3]),
                (META_TEXT, TYPE_TEXT, text_bytes(SHORT_LABELS[i])),
                (META_TEXT_FLAGS, TYPE_BYTE, vec![0x02]),
            ],
        });
        packets.push(meta.map_err(|e| e.to_string())?);

        buttons.push(PanelButton {
            entity_id: interaction_id,
            command: *command,
        });
    }

    let _ = block_type; // entity type was validated above
    let _ = text_type;
    Ok(ControlPanel {
        owner,
        buttons,
        display_ids,
        packets,
        anchor,
        yaw,
    })
}

fn push_block_display(
    data: &GameData,
    packets: &mut Vec<Vec<u8>>,
    entity_id: i32,
    center: [f64; 3],
    size: [f64; 3],
    state: i32,
    yaw: f32,
) -> Result<(), String> {
    let entity_type = data
        .entity_type_id("block_display")
        .ok_or("no block_display type")?;
    let spawn = data.packet_ids.encode(&SpawnEntity {
        entity_id,
        uuid: Uuid::new_v4(),
        entity_type,
        x: center[0],
        y: center[1],
        z: center[2],
        velocity: (0.0, 0.0, 0.0),
        pitch: 0.0,
        yaw: 0.0,
        head_yaw: 0.0,
        data: 0,
    });
    packets.push(spawn.map_err(|e| e.to_string())?);
    let angle = f64::from(yaw).to_radians();
    let half_x = size[0] / 2.0;
    let half_z = size[2] / 2.0;
    let tx = -(half_x * angle.cos() - half_z * angle.sin());
    let tz = -(half_x * angle.sin() + half_z * angle.cos());
    let ty = -size[1] / 2.0;
    let half = f64::from(yaw).to_radians() / 2.0;
    let meta = data.packet_ids.encode(&SetEntityData {
        entity_id,
        entries: vec![
            (META_TRANSLATION, TYPE_VECTOR3, vector3([tx, ty, tz])),
            (META_SCALE, TYPE_VECTOR3, vector3(size)),
            (
                META_LEFT_ROTATION,
                TYPE_QUATERNION,
                quaternion([0.0, -half.sin() as f32, 0.0, half.cos() as f32]),
            ),
            (META_BLOCK_STATE, TYPE_BLOCK_STATE, varint(state)),
        ],
    });
    packets.push(meta.map_err(|e| e.to_string())?);
    Ok(())
}

fn rotate_point(anchor: [f64; 3], yaw: f32, local: [f64; 3]) -> [f64; 3] {
    let angle = f64::from(yaw).to_radians();
    [
        anchor[0] + local[0] * angle.cos() - local[2] * angle.sin(),
        anchor[1] + local[1],
        anchor[2] + local[0] * angle.sin() + local[2] * angle.cos(),
    ]
}

fn block_state(data: &GameData, name: &str) -> Result<i32, String> {
    data.blocks
        .state(name)
        .map(|s| s as i32)
        .ok_or_else(|| format!("no block state for {name}"))
}

fn vector3(v: [f64; 3]) -> Vec<u8> {
    let mut w = PacketWriter::new();
    w.write_f32(v[0] as f32);
    w.write_f32(v[1] as f32);
    w.write_f32(v[2] as f32);
    w.into_inner()
}

fn vector3_bytes(v: f32) -> Vec<u8> {
    vector3([f64::from(v); 3])
}

fn quaternion(q: [f32; 4]) -> Vec<u8> {
    let mut w = PacketWriter::new();
    for v in q {
        w.write_f32(v);
    }
    w.into_inner()
}

fn f32_bytes(v: f32) -> Vec<u8> {
    let mut w = PacketWriter::new();
    w.write_f32(v);
    w.into_inner()
}

fn text_bytes(text: &str) -> Vec<u8> {
    let mut w = PacketWriter::new();
    w.write_text(&Text::new(text));
    w.into_inner()
}

fn varint(value: i32) -> Vec<u8> {
    let mut w = PacketWriter::new();
    w.write_varint(value);
    w.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn data() -> GameData {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        GameData::load(&dir).unwrap()
    }

    #[test]
    fn panel_has_one_command_per_button() {
        let data = data();
        let mut id = 1000;
        let panel = spawn(&data, Uuid::nil(), [0.0, 66.0, 0.0], 0.0, &mut || {
            id += 1;
            id
        })
        .unwrap();
        assert_eq!(panel.buttons.len(), MVP_COMMANDS.len());
        assert_eq!(panel.display_ids.len(), 1 + MVP_COMMANDS.len() * 3);
        assert!(!panel.packets.is_empty());
        assert_eq!(panel.buttons[0].command, MechCommand::MoveForward);
        assert_eq!(
            panel.command_for(panel.buttons[0].entity_id),
            Some(MechCommand::MoveForward)
        );
    }
}
