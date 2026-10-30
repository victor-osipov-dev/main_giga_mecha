//! Voxel Arcade RS server entry point.

use arcade_network::{Server, ServerConfig};
use arcade_protocol_data::GameData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if let Err(err) = run().await {
        error!("{err:#}");
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let config_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "configs/config.toml".to_owned()),
    );
    let config = ServerConfig::load_or_default(&config_path)?;
    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&config_path, toml::to_string_pretty(&config)?)?;
        info!("wrote default configuration to {}", config_path.display());
    }

    let version_dir = Path::new(&config.data_dir).join("versions/26.3");
    if !version_dir.exists() {
        anyhow::bail!(
            "Minecraft data not found at {}.\n\
             Run scripts/prepare-data.ps1 once to download the official 26.3 data pack.",
            version_dir.display()
        );
    }
    let data = GameData::load(&version_dir)?;
    info!(
        "loaded Minecraft {} (protocol {}, data version {}): {} dynamic registries, {} tags",
        data.version,
        data.protocol_version,
        data.data_version,
        data.registries.len(),
        data.tags.len()
    );

    let server = Server::new(config.clone(), data)?;
    let metrics = Arc::clone(&server.metrics);
    let metrics_task = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let snapshot = metrics.snapshot();
            match snapshot.rss_bytes {
                Some(rss) => info!(
                    "metrics: rss={:.1} MiB players={} connections={} packets_in={} packets_out={} map_frames={} map_bytes={}",
                    rss as f64 / (1024.0 * 1024.0),
                    snapshot.players,
                    snapshot.active_connections,
                    snapshot.packets_in,
                    snapshot.packets_out,
                    snapshot.map_frames,
                    snapshot.map_bytes
                ),
                None => warn!(
                    "metrics: rss=unavailable players={} connections={}",
                    snapshot.players, snapshot.active_connections
                ),
            }
        }
    });

    let result = Server::run(server).await;
    metrics_task.abort();
    result
}
