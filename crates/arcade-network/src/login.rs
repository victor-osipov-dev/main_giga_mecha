//! Login state, including online-mode encryption and session verification.

use crate::config_phase;
use crate::connection::Connection;
use arcade_auth::server_hash;
use arcade_protocol::packets::login::{
    EncryptionRequest, EncryptionResponse, LoginStart, LoginSuccess, SetCompression,
};
use arcade_protocol::{GameProfile, PacketReader, ServerboundPacket, State, Text};
use uuid::Uuid;

pub async fn handle(
    conn: &mut Connection,
    name: &str,
    reader: &mut PacketReader<'_>,
) -> anyhow::Result<()> {
    match name {
        "hello" => {
            let start = LoginStart::read(reader)?;
            handle_start(conn, start).await
        }
        "key" => {
            let response = EncryptionResponse::read(reader)?;
            handle_encryption_response(conn, response).await
        }
        "login_acknowledged" => {
            conn.state = State::Config;
            config_phase::begin(conn).await
        }
        _ => Ok(()),
    }
}

fn valid_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

async fn handle_start(conn: &mut Connection, start: LoginStart) -> anyhow::Result<()> {
    if conn.protocol_version != conn.server.data.protocol_version {
        let wanted = conn.server.data.version.clone();
        let text = if conn.protocol_version < conn.server.data.protocol_version {
            format!("Outdated client! Please use {wanted}")
        } else {
            format!("Outdated server! This is {wanted}")
        };
        conn.kick(Text::new(text)).await;
        anyhow::bail!(
            "protocol {} != {}",
            conn.protocol_version,
            conn.server.data.protocol_version
        );
    }
    if !valid_name(&start.name) {
        conn.kick(Text::new("Invalid username")).await;
        anyhow::bail!("invalid username {:?}", start.name);
    }
    conn.claimed_name = start.name.clone();

    if conn.server.config.online_mode {
        let keys = conn
            .server
            .keys
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("online mode without keys"))?;
        conn.send(&EncryptionRequest {
            server_id: String::new(),
            public_key: keys.public_der.clone(),
            verify_token: conn.verify_token.to_vec(),
            should_authenticate: true,
        })
        .await
    } else {
        tracing::warn!(
            "offline-mode login for {} from {} (development only)",
            start.name,
            conn.ip
        );
        finish_login(conn, GameProfile::offline(&start.name)).await
    }
}

async fn handle_encryption_response(
    conn: &mut Connection,
    response: EncryptionResponse,
) -> anyhow::Result<()> {
    let keys = conn
        .server
        .keys
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("encryption response without keys"))?
        .clone();

    let secret = keys
        .decrypt(&response.shared_secret)
        .map_err(|e| anyhow::anyhow!("cannot decrypt shared secret: {e}"))?;
    let token = keys
        .decrypt(&response.verify_token)
        .map_err(|e| anyhow::anyhow!("cannot decrypt verify token: {e}"))?;
    if token != conn.verify_token {
        conn.kick(Text::new("Invalid verify token")).await;
        anyhow::bail!("verify token mismatch");
    }
    let secret: [u8; 16] = secret
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("shared secret is not 16 bytes"))?;

    // From this point on every byte on the wire is encrypted.
    conn.codec.enable_encryption(&secret);

    let hash = server_hash(&secret, &keys.public_der);
    let name = conn.claimed_name.clone();
    match arcade_auth::has_joined(&conn.server.http, &name, &hash, &conn.ip).await {
        Ok(Some(profile)) => finish_login(conn, profile).await,
        Ok(None) => {
            conn.kick(Text::new("Failed to verify username!")).await;
            anyhow::bail!("session server rejected {name}")
        }
        Err(err) => {
            tracing::warn!("session server error for {name}: {err}");
            conn.kick(Text::new(
                "Authentication servers are down. Please try again later.",
            ))
            .await;
            anyhow::bail!("session server error: {err}")
        }
    }
}

async fn finish_login(conn: &mut Connection, mut profile: GameProfile) -> anyhow::Result<()> {
    if profile.name.is_empty() {
        profile.name = conn.claimed_name.clone();
    }

    if conn.server.online() >= conn.server.config.max_players {
        conn.kick(Text::new("The server is full!")).await;
        anyhow::bail!("server full");
    }

    // A second login with the same account replaces the first.
    if let Some(existing) = conn.server.players.get(profile.id) {
        existing.disconnect(Text::new("You logged in from another location"));
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let threshold = conn.server.config.compression_threshold;
    if threshold >= 0 {
        conn.send(&SetCompression { threshold }).await?;
        conn.codec.enable_compression(threshold as usize);
    }

    conn.send(&LoginSuccess {
        uuid: profile.id,
        username: profile.name.clone(),
        properties: profile.properties.clone(),
        session_id: Uuid::new_v4(),
    })
    .await?;

    tracing::info!("{} [{}] logged in as {}", profile.name, conn.ip, profile.id);
    conn.profile = Some(profile);
    Ok(())
}
