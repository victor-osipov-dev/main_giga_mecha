//! Authoritative player registry and per-player outbound queues.

use arcade_protocol::packets::play::GameMode;
use arcade_protocol::{ClientboundPacket, GameProfile, Text};
use arcade_protocol_data::GameData;
use bytes::Bytes;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::Instant;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Capacity of one player's outbound queue. A client that cannot keep up is
/// kicked rather than allowed to grow the server's memory without bound.
/// The block-built Atlas MK-II bursts thousands of spawn/step packets, so the
/// cap is generous enough to absorb walking and rejoin bursts without kicking.
pub const OUTBOUND_QUEUE: usize = 16384;

pub enum Outbound {
    Packet(Bytes),
    Disconnect(Text),
}

pub struct PlayerState {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub game_mode: GameMode,
    pub view_distance: i32,
    pub center_chunk: arcade_protocol::ChunkPos,
    pub loaded_chunks: HashSet<arcade_protocol::ChunkPos>,
    pub visible_players: HashSet<Uuid>,
    pub pending_teleport: Option<i32>,
    pub teleport_counter: i32,
    pub keepalive_id: i64,
    pub keepalive_sent: Instant,
    pub awaiting_keepalive: bool,
    pub latency_ms: i32,
    pub loaded: bool,
    pub last_activity: Instant,
    pub slow: bool,
}

impl PlayerState {
    pub fn chunk(&self) -> arcade_protocol::ChunkPos {
        arcade_protocol::ChunkPos::from_block(self.x.floor() as i32, self.z.floor() as i32)
    }
}

pub struct Player {
    pub uuid: Uuid,
    pub profile: GameProfile,
    pub entity_id: i32,
    pub ip: String,
    pub joined_at: Instant,
    pub data: Arc<GameData>,
    state: Mutex<PlayerState>,
    outbound: mpsc::Sender<Outbound>,
}

impl Player {
    pub fn new(
        profile: GameProfile,
        entity_id: i32,
        ip: String,
        data: Arc<GameData>,
        outbound: mpsc::Sender<Outbound>,
        spawn: (f64, f64, f64),
        game_mode: GameMode,
    ) -> Self {
        let now = Instant::now();
        let chunk =
            arcade_protocol::ChunkPos::from_block(spawn.0.floor() as i32, spawn.2.floor() as i32);
        Self {
            uuid: profile.id,
            profile,
            entity_id,
            ip,
            joined_at: now,
            data,
            state: Mutex::new(PlayerState {
                x: spawn.0,
                y: spawn.1,
                z: spawn.2,
                yaw: 0.0,
                pitch: 0.0,
                on_ground: false,
                game_mode,
                view_distance: 2,
                center_chunk: chunk,
                loaded_chunks: HashSet::new(),
                visible_players: HashSet::new(),
                pending_teleport: None,
                teleport_counter: 0,
                keepalive_id: 0,
                keepalive_sent: now,
                awaiting_keepalive: false,
                latency_ms: 0,
                loaded: false,
                last_activity: now,
                slow: false,
            }),
            outbound,
        }
    }

    pub fn name(&self) -> &str {
        &self.profile.name
    }

    pub fn lock(&self) -> MutexGuard<'_, PlayerState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Encodes and queues a packet. Never blocks the caller.
    pub fn send<P: ClientboundPacket>(&self, packet: &P) {
        match self.data.packet_ids.encode(packet) {
            Ok(bytes) => self.send_raw(Bytes::from(bytes)),
            Err(err) => {
                tracing::error!("cannot encode {} for {}: {err}", P::NAME, self.name())
            }
        }
    }

    pub fn send_raw(&self, payload: Bytes) {
        match self.outbound.try_send(Outbound::Packet(payload)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                let mut state = self.lock();
                state.slow = true;
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {}
        }
    }

    pub fn disconnect(&self, reason: Text) {
        let _ = self.outbound.try_send(Outbound::Disconnect(reason));
    }

    pub fn is_connected(&self) -> bool {
        !self.outbound.is_closed()
    }

    pub fn is_slow(&self) -> bool {
        self.lock().slow
    }
}

#[derive(Default)]
pub struct PlayerManager {
    players: RwLock<HashMap<Uuid, Arc<Player>>>,
    next_entity_id: AtomicI32,
}

impl PlayerManager {
    pub fn new() -> Self {
        Self {
            players: RwLock::new(HashMap::new()),
            next_entity_id: AtomicI32::new(1),
        }
    }

    pub fn allocate_entity_id(&self) -> i32 {
        let id = self.next_entity_id.fetch_add(1, Ordering::Relaxed);
        if id <= 0 {
            self.next_entity_id.store(2, Ordering::Relaxed);
            1
        } else {
            id
        }
    }

    /// Adds a player, returning any previous session with the same UUID.
    pub fn add(&self, player: Arc<Player>) -> Option<Arc<Player>> {
        self.players
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(player.uuid, player)
    }

    pub fn remove(&self, uuid: Uuid) -> Option<Arc<Player>> {
        self.players
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&uuid)
    }

    /// Removes `player` only if it is still the registered session for its
    /// UUID. A reconnecting client may already have replaced the old session;
    /// the old connection must not evict the new one.
    pub fn remove_player(&self, player: &Arc<Player>) -> bool {
        let mut players = self.players.write().unwrap_or_else(|e| e.into_inner());
        match players.get(&player.uuid) {
            Some(existing) if Arc::ptr_eq(existing, player) => {
                players.remove(&player.uuid);
                true
            }
            _ => false,
        }
    }

    pub fn get(&self, uuid: Uuid) -> Option<Arc<Player>> {
        self.players
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&uuid)
            .cloned()
    }

    pub fn all(&self) -> Vec<Arc<Player>> {
        self.players
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    pub fn count(&self) -> usize {
        self.players.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn by_entity(&self, entity_id: i32) -> Option<Arc<Player>> {
        self.players
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .find(|p| p.entity_id == entity_id)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcade_protocol_data::GameData;
    use std::path::Path;

    fn data() -> Arc<GameData> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        Arc::new(GameData::load(&dir).unwrap())
    }

    #[test]
    fn sessions_are_unique() {
        let data = data();
        let (tx, _rx) = mpsc::channel(OUTBOUND_QUEUE);
        let profile = GameProfile::offline("Steve");
        let player = Arc::new(Player::new(
            profile,
            1,
            "127.0.0.1".into(),
            data,
            tx,
            (0.5, 65.0, 0.5),
            GameMode::Creative,
        ));
        let manager = PlayerManager::new();
        assert!(manager.add(Arc::clone(&player)).is_none());
        assert_eq!(manager.count(), 1);
        assert!(manager.add(player).is_some());
        assert_eq!(manager.count(), 1);
    }

    #[test]
    fn stale_session_does_not_evict_reconnect() {
        let data = data();
        let (tx1, _rx1) = mpsc::channel(OUTBOUND_QUEUE);
        let (tx2, _rx2) = mpsc::channel(OUTBOUND_QUEUE);
        let profile = GameProfile::offline("Alex");
        let old = Arc::new(Player::new(
            profile.clone(),
            1,
            "127.0.0.1".into(),
            Arc::clone(&data),
            tx1,
            (0.5, 65.0, 0.5),
            GameMode::Creative,
        ));
        let new = Arc::new(Player::new(
            profile,
            2,
            "127.0.0.1".into(),
            data,
            tx2,
            (0.5, 65.0, 0.5),
            GameMode::Creative,
        ));
        let manager = PlayerManager::new();
        manager.add(Arc::clone(&old));
        manager.add(Arc::clone(&new));
        assert!(!manager.remove_player(&old));
        assert_eq!(manager.count(), 1);
        assert!(manager.remove_player(&new));
        assert_eq!(manager.count(), 0);
    }
}
