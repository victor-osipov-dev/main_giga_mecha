# Voxel Arcade RS

A small, memory-frugal Minecraft Java-compatible server core in Rust. It
targets **Minecraft Java 26.3 (protocol 777)** and is built as a platform for
server-side 2D arcade games, not vanilla survival.

Status: an official Minecraft 26.3 client has passed online-mode login,
configuration, world join and the map-rendered `/flappy` game. Multiplayer has
been verified with automated protocol clients; two real GUI clients should
still be checked. See `docs/COMPATIBILITY.md` for the exact matrix.

## Highlights

- Native Rust, no JVM, no Java proxy, no client mods.
- Online-mode authentication implemented (RSA + Mojang session server).
- Flat static world with cached, shared chunk packets.
- Real multiplayer: player list, entity spawn/despawn, movement, keep-alives.
- Bounded queues, connection limits and slow-client protection.
- Fixed-timestep game runtime, versioned input adapter and map renderer.
- Configurable game rate (1-20 Hz) and dirty-rectangle map updates.
- `/flappy` playable on a vanilla client (right-click/punch to flap).
- Mech Wars module: `/mech spawn` builds a 24-block voxel mech from vanilla
  `block_display` entities, plus a 18-button Interaction/text-display control
  panel; persistent movement and torso orders are included.
- **11-17 MiB RSS with 20 protocol clients** in a release build.

## Quick start

Requirements: Rust stable, network access for the one-time data preparation
step.

```powershell
# 1. Download the official 26.3 server jar and generate registry/packet data.
powershell -ExecutionPolicy Bypass -File scripts/prepare-data.ps1 -Version 26.3

# 2. Build.
cargo build --release

# 3. Configure. online_mode is true by default.
#    Edit configs/config.toml (motd, port, max_players, ...).

# 4. Run.
target/release/voxel-arcade.exe configs/config.toml
```

The server listens on `0.0.0.0:25565` by default. Connect with an unmodified
Minecraft Java 26.3 client.

Game rendering rate is controlled by `game_update_hz` in the config (1–20,
default 10). Maps are sent as one full frame followed by changed rectangles.

### Development / offline test

Offline mode is allowed only on loopback unless explicitly overridden:

```powershell
target/release/voxel-arcade.exe configs/dev-offline.toml
```

Then use the protocol test client:

```powershell
target/release/test-client.exe status 127.0.0.1:25566
target/release/test-client.exe login 127.0.0.1:25566 Steve 10
target/release/test-client.exe game  127.0.0.1:25566 Steve 10
```

## Tests

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The integration tests start a real TCP server and run two or twenty
protocol clients against it:

- status ping and pong;
- login, compression, configuration with 32 registries;
- chunks, teleport, player load;
- two players seeing each other, movement replication and disconnect removal;
- twenty clients joining and loading chunks at once;
- reconnecting with the same name replaces the old session;
- Flappy Bird map frames reaching the client;
- Mech Wars: one mech (598 shell voxels as 1×1×1 block displays) and two
  mechs replicated to both players.

## Layout

```text
crates/arcade-protocol       wire codec, packets, NBT, text
crates/arcade-protocol-data  26.3 packet ids, registries, tags, static ids
crates/arcade-auth           RSA, server hash, session server
crates/arcade-players        sessions, entity ids, outbound queues
crates/arcade-world          flat world, chunk sections, light
crates/arcade-network        listener, state machine, replication, games
crates/arcade-runtime        GameInput/GameState/GameSession/GameLoop
crates/arcade-render         frame -> map colours
crates/arcade-metrics        counters and RSS
crates/arcade-server         binary and configuration
games/flappy-bird            server-side map game
games/mech-wars              voxel mech simulation + block-display renderer
tools/test-client            protocol-level test client
data/versions/26.3           generated official data
docs/                        architecture, dependencies, decisions, memory
scripts/prepare-data.ps1     reproducible data preparation
```

## Commands in game

- `/spawn` — return to the platform.
- `/flappy` — give yourself the map and start a game.
- `/mech spawn` — deploy your Sentinel-01 mech and its control panel.
- `/mech forward`, `/mech back`, `/mech stop`, `/mech turn left|right`,
  `/mech torso left|right|reset`, `/mech auto on|off`, `/mech estop`.
- `/mech despawn`, `/mech status`, `/mech reset`.
- Right-click or left-click/punch — flap.
- Chat is broadcast as a server chat line.

## Documentation

- `docs/ARCHITECTURE.md`
- `docs/DEPENDENCIES.md`
- `docs/PROTOCOL_VERSIONS.md`
- `docs/MEMORY_BUDGET.md`
- `docs/DECISIONS.md`
- `docs/COMPATIBILITY.md`
- `docs/STATUS_REPORT.md`
- `docs/VERSION_ADAPTERS.md`
- `docs/MECH_WARS.md`
- `NOTICE.md` (MIT attribution for the adapted wire layer)

## License

MIT. Third-party notices are in `NOTICE.md`. Minecraft is a trademark of
Mojang Synergies AB; this project is not affiliated with Mojang or Microsoft.
