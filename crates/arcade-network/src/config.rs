//! Server configuration. Everything here is safe to edit in `config.toml`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to listen on.
    pub bind: String,
    /// Shown in the multiplayer server list.
    pub motd: String,
    /// Hard cap on players that may complete login.
    pub max_players: usize,
    /// Hard cap on TCP connections, including status pings.
    pub max_connections: usize,
    /// Online-mode authentication is on by default. Only disable this in an
    /// isolated development environment.
    pub online_mode: bool,
    /// Explicitly permit offline mode on a non-loopback address. This is a
    /// safety latch, not a supported public-server configuration.
    pub allow_offline_public: bool,
    /// Negative disables packet compression.
    pub compression_threshold: i32,
    /// Chunks sent around the player.
    pub view_distance: i32,
    /// Simulation distance reported to the client.
    pub simulation_distance: i32,
    /// `survival`, `creative`, `adventure` or `spectator`.
    pub game_mode: String,
    /// Map game simulation/render rate. Allowed range is 1..=20 Hz.
    pub game_update_hz: u32,
    /// Seconds without a packet before a connection is dropped.
    pub idle_timeout_seconds: u64,
    /// Keep-alive period in the play state.
    pub keepalive_seconds: u64,
    /// Data directory containing `versions/26.3`.
    pub data_dir: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:25565".into(),
            motd: "Voxel Arcade RS".into(),
            max_players: 20,
            max_connections: 128,
            online_mode: true,
            allow_offline_public: false,
            compression_threshold: 256,
            view_distance: 3,
            simulation_distance: 3,
            game_mode: "creative".into(),
            game_update_hz: 10,
            idle_timeout_seconds: 30,
            keepalive_seconds: 15,
            data_dir: "data".into(),
        }
    }
}

impl ServerConfig {
    pub fn load_or_default(path: &std::path::Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}
