//! CLI wrapper around the protocol-level test client.

use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("status");
    let addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:25565");
    let hold_seconds: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(10);
    let data_dir = args
        .get(5)
        .map(String::as_str)
        .unwrap_or("data/versions/26.3");
    let data_dir = Path::new(data_dir);

    match command {
        "status" => {
            let report = test_client::status(addr, data_dir).await?;
            println!(
                "status: version={} protocol={} online={} max={} motd={}",
                report.version, report.protocol, report.online, report.max, report.motd
            );
        }
        "login" => {
            let name = args.get(3).map(String::as_str).unwrap_or("TestBot");
            let report = test_client::login(addr, name, hold_seconds, data_dir).await?;
            println!(
                "login: {} entity={} registries={} chunks={} other_entities={} movements={} removals={} compression={}",
                report.username,
                report.entity_id,
                report.registries,
                report.chunks,
                report.entities,
                report.movements,
                report.removals,
                report.compression
            );
        }
        "game" => {
            let name = args.get(3).map(String::as_str).unwrap_or("TestBot");
            let report = test_client::login_game(addr, name, hold_seconds, data_dir).await?;
            println!(
                "game: {} entity={} chunks={} other_entities={} map_updates={} map_bytes={} map_pixels={} compression={}",
                report.username,
                report.entity_id,
                report.chunks,
                report.entities,
                report.map_updates,
                report.map_bytes,
                report.map_pixels,
                report.compression
            );
        }
        "mech" => {
            let name = args.get(3).map(String::as_str).unwrap_or("TestBot");
            let report = test_client::login_mech(addr, name, hold_seconds, data_dir).await?;
            println!(
                "mech: {} entity={} chunks={} display_entities={} set_entity_data={} position_syncs={} compression={}",
                report.username,
                report.entity_id,
                report.chunks,
                report.entities,
                report.set_entity_data,
                report.position_syncs,
                report.compression
            );
        }
        other => anyhow::bail!("unknown command {other}; use status, login, game or mech"),
    }
    Ok(())
}
