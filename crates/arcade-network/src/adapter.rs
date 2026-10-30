//! Versioned input adapters.
//!
//! The rest of the server speaks in [`GameInput`]. Each Minecraft protocol
//! version has its own packet ids and action layout, so it gets a small
//! adapter instead of sprinkling version checks through the play handler.

use arcade_protocol::{PacketIds, PacketReader, ProtocolError, State};
use arcade_runtime::GameInput;

pub trait ProtocolAdapter: Send + Sync {
    fn protocol_version(&self) -> i32;

    /// Maps a serverbound play packet (`id + body`) to a normalized input.
    /// `None` means the packet is not a game action.
    fn decode_input(&self, packet: &[u8]) -> Result<Option<GameInput>, ProtocolError>;
}

/// Minecraft 26.3 (protocol 777) input adapter.
pub struct V26_3Adapter {
    ids: PacketIds,
}

impl V26_3Adapter {
    pub fn new(ids: PacketIds) -> Self {
        Self { ids }
    }
}

impl ProtocolAdapter for V26_3Adapter {
    fn protocol_version(&self) -> i32 {
        777
    }

    fn decode_input(&self, packet: &[u8]) -> Result<Option<GameInput>, ProtocolError> {
        let mut reader = PacketReader::new(packet);
        let id = reader.read_varint()?;
        let input = match self.ids.serverbound_name(State::Play, id) {
            // Right-click (air or block) and left-click/punch.
            Some("use_item" | "use_item_on" | "punch") => Some(GameInput::Jump),
            _ => None,
        };
        Ok(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcade_protocol::PacketWriter;
    use arcade_protocol_data::GameData;
    use std::path::Path;

    fn adapter() -> V26_3Adapter {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        let data = GameData::load(&dir).unwrap();
        V26_3Adapter::new(data.packet_ids.clone())
    }

    fn packet(ids: &PacketIds, name: &str) -> Vec<u8> {
        let mut w = PacketWriter::new();
        w.write_varint(ids.serverbound_id(State::Play, name).unwrap());
        w.into_inner()
    }

    #[test]
    fn maps_clicks_to_jump() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        let ids = GameData::load(&dir).unwrap().packet_ids;
        let adapter = adapter();
        assert_eq!(adapter.protocol_version(), 777);
        for name in ["use_item", "use_item_on", "punch"] {
            assert_eq!(
                adapter.decode_input(&packet(&ids, name)).unwrap(),
                Some(GameInput::Jump)
            );
        }
        assert_eq!(
            adapter
                .decode_input(&packet(&ids, "move_player_pos"))
                .unwrap(),
            None
        );
    }
}
