//! The listener and shared server state.

use crate::adapter::{ProtocolAdapter, V26_3Adapter};
use crate::config::ServerConfig;
use crate::connection::Connection;
use arcade_auth::EncryptionKeys;
use arcade_metrics::Metrics;
use arcade_players::PlayerManager;
use arcade_protocol::packets::config::{RegistryData, UpdateTags};
use arcade_protocol::{ChunkPos, ClientboundPacket};
use arcade_protocol_data::GameData;
use arcade_world::FlatWorld;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

pub struct Server {
    pub config: ServerConfig,
    pub data: Arc<GameData>,
    pub world: Arc<FlatWorld>,
    pub players: Arc<PlayerManager>,
    pub adapter: Arc<dyn ProtocolAdapter>,
    pub keys: Option<Arc<EncryptionKeys>>,
    pub http: reqwest::Client,
    pub metrics: Arc<Metrics>,
    pub started: Instant,
    /// Registry and tag packets encoded once at startup and shared by every
    /// connecting client.
    registry_payloads: Vec<Bytes>,
    tag_payload: Option<Bytes>,
    pub games: Mutex<crate::game::GameHub>,
    pub mechs: Mutex<mech_wars::MechWars>,
    chunk_cache: Mutex<HashMap<ChunkPos, Bytes>>,
}

impl Server {
    pub fn new(config: ServerConfig, data: GameData) -> anyhow::Result<Arc<Self>> {
        if !config.online_mode && !config.allow_offline_public {
            let addr: std::net::SocketAddr = config.bind.parse()?;
            if !addr.ip().is_loopback() {
                anyhow::bail!(
                    "refusing to run offline-mode on a non-loopback address ({}); \
                     set allow_offline_public = true only for an isolated environment",
                    config.bind
                );
            }
        }
        let keys = if config.online_mode {
            Some(Arc::new(EncryptionKeys::generate()?))
        } else {
            None
        };
        let adapter: Arc<dyn ProtocolAdapter> =
            Arc::new(V26_3Adapter::new(data.packet_ids.clone()));
        let world = Arc::new(FlatWorld::new(&data));
        let registry_payloads = data
            .registry_packets()
            .into_iter()
            .map(|(registry, entries)| {
                data.packet_ids
                    .encode(&RegistryData { registry, entries })
                    .map(Bytes::from)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let tags = data.tag_packets();
        let tag_payload = if tags.is_empty() {
            None
        } else {
            Some(Bytes::from(
                data.packet_ids.encode(&UpdateTags { registries: tags })?,
            ))
        };
        Ok(Arc::new(Self {
            config,
            data: Arc::new(data),
            world,
            players: Arc::new(PlayerManager::new()),
            adapter,
            keys,
            http: arcade_auth::http_client(),
            metrics: Arc::new(Metrics::default()),
            started: Instant::now(),
            registry_payloads,
            tag_payload,
            games: Mutex::new(crate::game::GameHub::default()),
            mechs: Mutex::new(mech_wars::MechWars::default()),
            chunk_cache: Mutex::new(HashMap::new()),
        }))
    }

    /// Encoded `level_chunk_with_light` payload for a chunk, cached because
    /// the flat world is immutable.
    pub fn chunk_payload(&self, pos: ChunkPos) -> Bytes {
        if let Some(hit) = self
            .chunk_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pos)
            .cloned()
        {
            return hit;
        }
        let chunk = self.world.chunk(pos);
        let encoded = match self.data.packet_ids.encode(chunk.as_ref()) {
            Ok(bytes) => Bytes::from(bytes),
            Err(err) => {
                warn!("cannot encode chunk {pos:?}: {err}");
                Bytes::new()
            }
        };
        self.chunk_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pos, encoded.clone());
        encoded
    }

    pub fn registry_payloads(&self) -> &[Bytes] {
        &self.registry_payloads
    }

    pub fn tag_payload(&self) -> Option<Bytes> {
        self.tag_payload.clone()
    }

    /// Sends already encoded payloads to every connected player.
    pub fn send_raw_all(&self, payloads: &[Vec<u8>]) {
        for player in self.players.all() {
            for payload in payloads {
                player.send_raw(Bytes::from(payload.clone()));
                self.metrics.packets_out.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn send_raw_to(&self, player: &Arc<arcade_players::Player>, payloads: &[Vec<u8>]) {
        for payload in payloads {
            player.send_raw(Bytes::from(payload.clone()));
            self.metrics.packets_out.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn broadcast<P: ClientboundPacket>(&self, packet: &P, skip: Option<Uuid>) {
        let encoded = match self.data.packet_ids.encode(packet) {
            Ok(bytes) => Bytes::from(bytes),
            Err(err) => {
                warn!("cannot encode {}: {err}", P::NAME);
                return;
            }
        };
        for player in self.players.all() {
            if Some(player.uuid) == skip {
                continue;
            }
            player.send_raw(encoded.clone());
            self.metrics.packets_out.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn online(&self) -> usize {
        self.players.count()
    }

    pub async fn run(self: Arc<Self>) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.config.bind).await?;
        info!(
            "Voxel Arcade {} (protocol {}) listening on {}",
            self.data.version, self.data.protocol_version, self.config.bind
        );
        info!(
            "online-mode: {}, max players: {}, view distance: {}, game rate: {} Hz",
            self.config.online_mode,
            self.config.max_players,
            self.config.view_distance,
            self.config.game_update_hz.clamp(1, 20)
        );

        let limiter = Arc::new(Semaphore::new(self.config.max_connections.max(1)));
        let mut tasks = tokio::task::JoinSet::new();
        let games_task = tokio::spawn(crate::game::game_loop(Arc::clone(&self)));
        let mech_task = tokio::spawn(mech_loop(Arc::clone(&self)));

        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (socket, addr) = match accepted {
                        Ok(pair) => pair,
                        Err(err) => {
                            warn!("accept failed: {err}");
                            continue;
                        }
                    };
                    let Ok(permit) = Arc::clone(&limiter).try_acquire_owned() else {
                        self.metrics.rejected_connections.fetch_add(1, Ordering::Relaxed);
                        warn!("rejecting {addr}: connection limit reached");
                        continue;
                    };
                    let server = Arc::clone(&self);
                    self.metrics.active_connections.fetch_add(1, Ordering::Relaxed);
                    tasks.spawn(async move {
                        let _permit = permit;
                        let mut connection = Connection::new(Arc::clone(&server), socket, addr);
                        connection.run().await;
                        server.metrics.active_connections.fetch_sub(1, Ordering::Relaxed);
                    });
                    while let Some(done) = tasks.try_join_next() {
                        if let Err(err) = done {
                            if !err.is_cancelled() {
                                warn!("connection task failed: {err}");
                            }
                        }
                    }
                }
                _ = shutdown_signal() => {
                    info!("shutdown requested");
                    break;
                }
            }
        }

        games_task.abort();
        mech_task.abort();
        // Give active connections a brief chance to finish their kick packet.
        let deadline = tokio::time::sleep(std::time::Duration::from_secs(2));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => {
                    tasks.abort_all();
                    break;
                }
                next = tasks.join_next() => {
                    if next.is_none() {
                        break;
                    }
                }
            }
        }
        info!("server stopped");
        Ok(())
    }
}

/// 20 TPS simulation for voxel mechs; packets are sent only when a display
/// actually moves or rotates.
async fn mech_loop(server: Arc<Server>) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(50));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last = Instant::now();
    loop {
        ticker.tick().await;
        let dt = last.elapsed().as_secs_f32().min(0.2);
        last = Instant::now();
        let updates = {
            let mut mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
            mechs.tick(&server.data, dt)
        };
        if !updates.is_empty() {
            server.send_raw_all(&updates);
        }
    }
}

/// Resolves on Ctrl+C, and on SIGTERM on Unix (systemd/container shutdown).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = match signal(SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
