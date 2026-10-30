//! Two protocol clients log in at once and must see each other move and leave.

use arcade_network::{Server, ServerConfig};
use arcade_protocol_data::GameData;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

#[test]
fn two_players_see_each_other() {
    let _guard = test_lock();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data_dir = root.join("data/versions/26.3");
    let data = GameData::load(&data_dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25598".into(),
        online_mode: false,
        view_distance: 2,
        simulation_distance: 2,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;

        let (alice, bob) = tokio::join!(
            test_client::login("127.0.0.1:25598", "Alice", 5, &data_dir),
            test_client::login("127.0.0.1:25598", "Bob", 2, &data_dir),
        );
        let alice = alice.expect("Alice login");
        let bob = bob.expect("Bob login");

        assert_eq!(alice.registries, 32);
        assert_eq!(bob.registries, 32);
        assert!(alice.chunks > 0 && bob.chunks > 0);
        assert!(alice.entities >= 1, "Alice did not see Bob: {alice:?}");
        assert!(bob.entities >= 1, "Bob did not see Alice: {bob:?}");
        assert!(
            alice.removals >= 1,
            "Alice did not see Bob leave: {alice:?}"
        );
        assert!(
            alice.movements >= 1,
            "Alice did not see Bob move: {alice:?}"
        );
        task.abort();
    });
}

#[test]
fn flappy_frames_reach_the_client() {
    let _guard = test_lock();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data_dir = root.join("data/versions/26.3");
    let data = GameData::load(&data_dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25597".into(),
        online_mode: false,
        view_distance: 2,
        simulation_distance: 2,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;
        let report = test_client::login_game("127.0.0.1:25597", "Gamer", 3, &data_dir)
            .await
            .expect("game login");
        assert!(report.chunks > 0);
        assert!(
            report.map_updates > 0,
            "no map frames reached the client: {report:?}"
        );
        assert!(report.map_bytes > 0, "empty map patch: {report:?}");
        assert!(report.map_pixels > 0, "empty map area: {report:?}");
        task.abort();
    });
}

#[test]
fn twenty_players_connect_and_load_chunks() {
    let _guard = test_lock();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data_dir = root.join("data/versions/26.3");
    let data = GameData::load(&data_dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25596".into(),
        online_mode: false,
        view_distance: 2,
        simulation_distance: 2,
        max_players: 20,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;

        let mut set = tokio::task::JoinSet::new();
        for i in 0..20 {
            let dir = data_dir.clone();
            set.spawn(async move {
                test_client::login("127.0.0.1:25596", &format!("Player{i:02}"), 4, &dir).await
            });
        }
        let mut loaded = 0;
        while let Some(result) = set.join_next().await {
            let report = result.expect("task").expect("login");
            assert!(report.chunks > 0, "{report:?}");
            assert!(report.entities >= 1, "{report:?}");
            loaded += 1;
        }
        assert_eq!(loaded, 20);
        task.abort();
    });
}

#[test]
fn reconnect_replaces_the_old_session() {
    let _guard = test_lock();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data_dir = root.join("data/versions/26.3");
    let data = GameData::load(&data_dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25594".into(),
        online_mode: false,
        view_distance: 2,
        simulation_distance: 2,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;

        let first_dir = data_dir.clone();
        let first = tokio::spawn(async move {
            test_client::login("127.0.0.1:25594", "Alice", 6, &first_dir).await
        });
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert_eq!(
            test_client::status("127.0.0.1:25594", &data_dir)
                .await
                .unwrap()
                .online,
            1
        );

        let second_dir = data_dir.clone();
        let second = tokio::spawn(async move {
            test_client::login("127.0.0.1:25594", "Alice", 3, &second_dir).await
        });
        tokio::time::sleep(Duration::from_millis(2200)).await;
        assert_eq!(
            test_client::status("127.0.0.1:25594", &data_dir)
                .await
                .unwrap()
                .online,
            1,
            "reconnect must not duplicate the session"
        );
        let _ = first.await;
        second
            .await
            .expect("second login task")
            .expect("second login");
        task.abort();
    });
}

#[test]
fn mech_spawn_replicates_block_displays() {
    let _guard = test_lock();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data_dir = root.join("data/versions/26.3");
    let data = GameData::load(&data_dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25593".into(),
        online_mode: false,
        view_distance: 2,
        simulation_distance: 2,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;
        let report = test_client::login_mech("127.0.0.1:25593", "Pilot", 3, &data_dir)
            .await
            .expect("mech login");
        assert!(report.chunks > 0);
        assert!(
            report.entities >= 10,
            "no block displays reached the client: {report:?}"
        );
        assert!(
            report.set_entity_data >= 10,
            "missing display metadata: {report:?}"
        );
        assert!(report.position_syncs > 0, "mech did not move: {report:?}");
        task.abort();
    });
}

#[test]
fn two_players_get_two_mechs() {
    let _guard = test_lock();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data_dir = root.join("data/versions/26.3");
    let data = GameData::load(&data_dir).unwrap();
    let config = ServerConfig {
        bind: "127.0.0.1:25592".into(),
        online_mode: false,
        view_distance: 2,
        simulation_distance: 2,
        ..ServerConfig::default()
    };
    let server = Server::new(config, data).unwrap();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let task = tokio::spawn(Server::run(server));
        tokio::time::sleep(Duration::from_millis(800)).await;
        let a_dir = data_dir.clone();
        let b_dir = data_dir.clone();
        let (a, b) = tokio::join!(
            test_client::login_mech("127.0.0.1:25592", "PilotA", 4, &a_dir),
            test_client::login_mech("127.0.0.1:25592", "PilotB", 4, &b_dir),
        );
        let a = a.expect("PilotA");
        let b = b.expect("PilotB");
        assert!(a.entities >= 70, "PilotA saw {} displays", a.entities);
        assert!(b.entities >= 70, "PilotB saw {} displays", b.entities);
        assert!(a.set_entity_data >= 70);
        assert!(b.set_entity_data >= 70);
        task.abort();
    });
}
