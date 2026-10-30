//! Server list ping.

use crate::connection::Connection;
use arcade_protocol::packets::status::{
    PingRequest, PongResponse, StatusJson, StatusPlayerSample, StatusPlayers, StatusResponse,
    StatusVersion,
};
use arcade_protocol::{PacketReader, ServerboundPacket};

pub async fn handle(
    conn: &mut Connection,
    name: &str,
    reader: &mut PacketReader<'_>,
) -> anyhow::Result<()> {
    match name {
        "status_request" => {
            let json = build_status_json(conn);
            conn.send(&StatusResponse { json }).await
        }
        "ping_request" => {
            let ping = PingRequest::read(reader)?;
            conn.send(&PongResponse {
                payload: ping.payload,
            })
            .await
        }
        _ => Ok(()),
    }
}

fn build_status_json(conn: &Connection) -> String {
    let server = &conn.server;
    let players = server.players.all();
    let sample: Vec<StatusPlayerSample> = players
        .iter()
        .take(12)
        .map(|p| StatusPlayerSample {
            name: p.name().to_owned(),
            id: p.uuid.to_string(),
        })
        .collect();
    let status = StatusJson {
        version: StatusVersion {
            name: format!("Voxel Arcade {}", server.data.version),
            protocol: server.data.protocol_version,
        },
        players: StatusPlayers {
            max: server.config.max_players as i32,
            online: players.len() as i32,
            sample,
        },
        description: serde_json::json!({ "text": server.config.motd }),
        favicon: None,
        enforces_secure_chat: false,
    };
    serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_owned())
}
