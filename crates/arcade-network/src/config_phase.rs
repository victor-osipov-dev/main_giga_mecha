//! Configuration state: send registries and tags, then hand over to play.

use crate::connection::Connection;
use crate::play;
use arcade_protocol::packets::config::{
    ClientInformation, ClientboundKnownPacks, ClientboundPluginMessage, FeatureFlags,
    FinishConfiguration, ServerboundKnownPacks,
};
use arcade_protocol::{Identifier, PacketReader, ServerboundPacket};

pub async fn begin(conn: &mut Connection) -> anyhow::Result<()> {
    let mut brand = arcade_protocol::PacketWriter::new();
    brand.write_string("voxel-arcade");
    conn.send(&ClientboundPluginMessage {
        channel: Identifier::minecraft("brand"),
        data: brand.into_inner(),
    })
    .await?;
    conn.send(&FeatureFlags {
        features: vec![Identifier::minecraft("vanilla")],
    })
    .await?;
    // We always send full registry data, so the client does not need to
    // share any known packs.
    conn.send(&ClientboundKnownPacks { packs: Vec::new() })
        .await
}

pub async fn handle(
    conn: &mut Connection,
    name: &str,
    reader: &mut PacketReader<'_>,
) -> anyhow::Result<()> {
    match name {
        "client_information" => {
            conn.client_info = Some(ClientInformation::read(reader)?);
            Ok(())
        }
        "select_known_packs" => {
            let _ = ServerboundKnownPacks::read(reader)?;
            send_registries(conn).await
        }
        "finish_configuration" => {
            if !conn.config_sent() {
                conn.kick(arcade_protocol::Text::new(
                    "Configuration finished before registries were sent",
                ))
                .await;
                anyhow::bail!("client finished configuration too early");
            }
            play::begin(conn).await
        }
        _ => Ok(()),
    }
}

async fn send_registries(conn: &mut Connection) -> anyhow::Result<()> {
    let payloads = conn.server.registry_payloads().to_vec();
    for payload in &payloads {
        conn.send_encoded(payload).await?;
    }
    if let Some(tags) = conn.server.tag_payload() {
        conn.send_encoded(&tags).await?;
    }
    conn.send(&FinishConfiguration).await?;
    conn.mark_config_sent();
    Ok(())
}
