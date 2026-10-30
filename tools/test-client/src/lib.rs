//! A small protocol-level client used by integration tests.
//!
//! It is not a Minecraft client. It speaks the same bytes and is enough to
//! prove status, login, configuration, chunks and movement between two
//! server-side sessions. Online-mode login needs a real Mojang session and is
//! therefore exercised with the official client instead.

use anyhow::{bail, Context};
use arcade_protocol::{Codec, PacketReader, PacketWriter, State};
use arcade_protocol_data::GameData;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct StatusReport {
    pub version: String,
    pub protocol: i32,
    pub online: i64,
    pub max: i64,
    pub motd: String,
}

#[derive(Clone, Debug, Default)]
pub struct LoginReport {
    pub username: String,
    pub entity_id: i32,
    pub registries: usize,
    pub chunks: usize,
    pub entities: usize,
    pub movements: usize,
    pub removals: usize,
    pub map_updates: usize,
    pub map_bytes: usize,
    pub map_pixels: usize,
    pub set_entity_data: usize,
    pub position_syncs: usize,
    pub compression: bool,
}

struct Client {
    stream: TcpStream,
    codec: Codec,
    buf: Vec<u8>,
    data: GameData,
    state: State,
}

impl Client {
    async fn connect(addr: &str, data_dir: &Path) -> anyhow::Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        let data = GameData::load(data_dir)?;
        Ok(Self {
            stream,
            codec: Codec::new(),
            buf: vec![0u8; 64 * 1024],
            data,
            state: State::Handshake,
        })
    }

    fn id(&self, state: State, name: &str) -> anyhow::Result<i32> {
        self.data
            .packet_ids
            .serverbound_id(state, name)
            .with_context(|| format!("no serverbound id for {name} in {state:?}"))
    }

    async fn send(&mut self, payload: &[u8]) -> anyhow::Result<()> {
        let frame = self.codec.encode(payload);
        self.stream.write_all(&frame).await?;
        Ok(())
    }

    async fn send_packet(&mut self, state: State, name: &str, body: &[u8]) -> anyhow::Result<()> {
        let id = self.id(state, name)?;
        let mut w = PacketWriter::new();
        w.write_varint(id);
        let mut payload = w.into_inner();
        payload.extend_from_slice(body);
        self.send(&payload).await
    }

    async fn recv(&mut self) -> anyhow::Result<Vec<u8>> {
        loop {
            if let Some(packet) = self.codec.next_packet()? {
                return Ok(packet);
            }
            let n = self.stream.read(&mut self.buf).await?;
            if n == 0 {
                bail!("connection closed");
            }
            self.codec.push_received(&self.buf[..n]);
        }
    }

    fn name_of<'a>(&self, packet: &'a [u8]) -> anyhow::Result<(String, PacketReader<'a>)> {
        let mut r = PacketReader::new(packet);
        let id = r.read_varint()?;
        let name = self
            .data
            .packet_ids
            .clientbound_name(self.state, id)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("unknown_0x{id:02x}"));
        Ok((name, r))
    }

    async fn handshake(&mut self, addr: &str, intent: i32) -> anyhow::Result<()> {
        let mut w = PacketWriter::new();
        w.write_varint(777);
        w.write_string(addr.split(':').next().unwrap_or(addr));
        w.write_u16(
            addr.rsplit(':')
                .next()
                .and_then(|p| p.parse().ok())
                .unwrap_or(25565),
        );
        w.write_varint(intent);
        self.send_packet(State::Handshake, "intention", &w.into_inner())
            .await
    }
}

pub async fn status(addr: &str, data_dir: &Path) -> anyhow::Result<StatusReport> {
    let mut client = Client::connect(addr, data_dir).await?;
    client.handshake(addr, 1).await?;
    client.state = State::Status;
    client
        .send_packet(State::Status, "ping_request", &{
            let mut w = PacketWriter::new();
            w.write_i64(123456);
            w.into_inner()
        })
        .await?;
    client
        .send_packet(State::Status, "status_request", &[])
        .await?;

    let mut report = None;
    let mut got_pong = false;
    for _ in 0..4 {
        let packet = client.recv().await?;
        let (name, mut r) = client.name_of(&packet)?;
        match name.as_str() {
            "status_response" => {
                let json = r.read_string()?;
                let parsed: serde_json::Value = serde_json::from_str(&json)?;
                if parsed["version"]["protocol"] != 777 {
                    bail!("unexpected protocol version");
                }
                report = Some(StatusReport {
                    version: parsed["version"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    protocol: parsed["version"]["protocol"].as_i64().unwrap_or(0) as i32,
                    online: parsed["players"]["online"].as_i64().unwrap_or(0),
                    max: parsed["players"]["max"].as_i64().unwrap_or(0),
                    motd: parsed["description"].to_string(),
                });
            }
            "pong_response" => {
                if r.read_i64()? != 123456 {
                    bail!("pong payload mismatch");
                }
                got_pong = true;
            }
            _ => {}
        }
        if report.is_some() && got_pong {
            break;
        }
    }
    if !got_pong {
        bail!("no pong response");
    }
    report.context("no status response")
}

pub async fn login(
    addr: &str,
    name: &str,
    hold_seconds: u64,
    data_dir: &Path,
) -> anyhow::Result<LoginReport> {
    login_inner(addr, name, hold_seconds, data_dir, None).await
}

/// Logs in and starts `/flappy`, exercising the map renderer path.
pub async fn login_game(
    addr: &str,
    name: &str,
    hold_seconds: u64,
    data_dir: &Path,
) -> anyhow::Result<LoginReport> {
    login_inner(addr, name, hold_seconds, data_dir, Some("flappy")).await
}

/// Logs in and asks the server to deploy the player's mech.
pub async fn login_mech(
    addr: &str,
    name: &str,
    hold_seconds: u64,
    data_dir: &Path,
) -> anyhow::Result<LoginReport> {
    login_inner(addr, name, hold_seconds, data_dir, Some("mech spawn")).await
}

async fn login_inner(
    addr: &str,
    name: &str,
    hold_seconds: u64,
    data_dir: &Path,
    action: Option<&str>,
) -> anyhow::Result<LoginReport> {
    let mut client = Client::connect(addr, data_dir).await?;
    client.handshake(addr, 2).await?;
    client.state = State::Login;
    let mut w = PacketWriter::new();
    w.write_string(name);
    w.write_uuid(&Uuid::new_v4());
    client
        .send_packet(State::Login, "hello", &w.into_inner())
        .await?;

    let mut compression_enabled = false;
    loop {
        let packet = client.recv().await?;
        let (packet_name, mut r) = client.name_of(&packet)?;
        match packet_name.as_str() {
            "login_compression" => {
                let threshold = r.read_varint()?;
                client.codec.enable_compression(threshold.max(0) as usize);
                compression_enabled = true;
            }
            "login_finished" => {
                r.read_uuid()?;
                r.read_string()?;
                client
                    .send_packet(State::Login, "login_acknowledged", &[])
                    .await?;
                client.state = State::Config;
                break;
            }
            "login_disconnect" => {
                bail!("login rejected: {}", r.read_string()?)
            }
            _ => {}
        }
    }

    let mut registries = 0usize;
    loop {
        let packet = client.recv().await?;
        let (packet_name, _r) = client.name_of(&packet)?;
        match packet_name.as_str() {
            "select_known_packs" => {
                let mut info = PacketWriter::new();
                info.write_string("en_us");
                info.write_i8(8);
                info.write_varint(0);
                info.write_bool(true);
                info.write_u8(0x7f);
                info.write_varint(1);
                info.write_bool(false);
                info.write_bool(true);
                info.write_varint(0);
                client
                    .send_packet(State::Config, "client_information", &info.into_inner())
                    .await?;
                let mut known = PacketWriter::new();
                known.write_varint(0);
                client
                    .send_packet(State::Config, "select_known_packs", &known.into_inner())
                    .await?;
            }
            "registry_data" => registries += 1,
            "finish_configuration" => {
                client
                    .send_packet(State::Config, "finish_configuration", &[])
                    .await?;
                client.state = State::Play;
                break;
            }
            "disconnect" => bail!("configuration rejected"),
            _ => {}
        }
    }

    let mut report = LoginReport {
        username: name.to_owned(),
        registries,
        compression: compression_enabled,
        ..Default::default()
    };
    let mut entities = HashSet::new();
    let mut sent_loaded = false;
    let mut sent_command = false;
    let mut sent_drive = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(hold_seconds.max(1));
    let mut next_move = tokio::time::Instant::now() + Duration::from_millis(500);
    let mut x = 1.5f64;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        let wait = (deadline - now).min(Duration::from_millis(500));
        let packet = match tokio::time::timeout(wait, client.recv()).await {
            Ok(Ok(p)) => Some(p),
            Ok(Err(err)) => bail!("connection error: {err}"),
            Err(_) => None,
        };
        if let Some(packet) = packet {
            let (packet_name, mut r) = client.name_of(&packet)?;
            match packet_name.as_str() {
                "login" => {
                    report.entity_id = r.read_i32()?;
                    if !sent_loaded {
                        client
                            .send_packet(State::Play, "player_loaded", &[])
                            .await?;
                        sent_loaded = true;
                    }
                }
                "player_position" => {
                    let teleport_id = r.read_varint()?;
                    let mut w = PacketWriter::new();
                    w.write_varint(teleport_id);
                    client
                        .send_packet(State::Play, "accept_teleportation", &w.into_inner())
                        .await?;
                }
                "keep_alive" => {
                    let id = r.read_i64()?;
                    let mut w = PacketWriter::new();
                    w.write_i64(id);
                    client
                        .send_packet(State::Play, "keep_alive", &w.into_inner())
                        .await?;
                }
                "level_chunk_with_light" => report.chunks += 1,
                "set_entity_data" => report.set_entity_data += 1,
                "entity_position_sync" => report.position_syncs += 1,
                "map_item_data" => {
                    report.map_updates += 1;
                    let _map_id = r.read_varint()?;
                    let _scale = r.read_i8()?;
                    let _locked = r.read_bool()?;
                    if r.read_bool()? {
                        let icons = r.read_varint()?;
                        for _ in 0..icons {
                            let _kind = r.read_varint()?;
                            let _x = r.read_i8()?;
                            let _z = r.read_i8()?;
                            let _direction = r.read_u8()?;
                        }
                    }
                    let columns = r.read_u8()?;
                    if columns > 0 {
                        let rows = r.read_u8()?;
                        let _x = r.read_u8()?;
                        let _z = r.read_u8()?;
                        let bytes = r.read_byte_array()?;
                        report.map_bytes += bytes.len();
                        report.map_pixels += columns as usize * rows as usize;
                    }
                }
                "add_entity" => {
                    entities.insert(r.read_varint()?);
                }
                "remove_entities" => report.removals += 1,
                "move_entity_pos" | "move_entity_pos_rot" | "move_entity_rot" => {
                    report.movements += 1
                }
                "disconnect" => bail!("disconnected during play"),
                _ => {}
            }
        }
        if sent_loaded && tokio::time::Instant::now() >= next_move {
            x = if x > 6.0 { 1.5 } else { x + 0.5 };
            let mut w = PacketWriter::new();
            w.write_f64(x);
            w.write_f64(65.0);
            w.write_f64(0.5);
            w.write_f32(90.0);
            w.write_f32(0.0);
            w.write_u8(1);
            client
                .send_packet(State::Play, "move_player_pos_rot", &w.into_inner())
                .await?;
            next_move = tokio::time::Instant::now() + Duration::from_millis(500);
            if let Some(action) = action {
                if !sent_command {
                    let mut cmd = PacketWriter::new();
                    cmd.write_string(action);
                    client
                        .send_packet(State::Play, "chat_command", &cmd.into_inner())
                        .await?;
                    sent_command = true;
                }
                if action == "mech spawn" && !sent_drive {
                    let mut drive = PacketWriter::new();
                    drive.write_string("mech forward");
                    client
                        .send_packet(State::Play, "chat_command", &drive.into_inner())
                        .await?;
                    sent_drive = true;
                }
                if action == "flappy" {
                    client.send_packet(State::Play, "punch", &[]).await?;
                }
            }
        }
    }
    report.entities = entities.len();
    if report.chunks == 0 {
        bail!("no chunks received");
    }
    if !sent_loaded {
        bail!("world packet was not received");
    }
    Ok(report)
}
