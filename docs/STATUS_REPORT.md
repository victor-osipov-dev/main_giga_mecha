# Status report — MVP core

Date: 2026-10-29

## 1. Stage

Stages 0–10 of `PLAN.md` are implemented or scaffolded: audit and workspace,
transport, 26.3 status/login/configuration/play, online-mode authentication,
flat world, multiplayer, runtime, map renderer and Flappy Bird. Mech Wars
M1–M4 (voxel model, component hierarchy, block-display renderer, control
panel, persistent orders and movement) is implemented; combat and damage
remain. A
second protocol version (stage 7) is not implemented yet; the adapter seam
and its plan are documented in `docs/VERSION_ADAPTERS.md`.

## 2. Implemented functionality

- Cargo workspace with the planned server crates, a game crate and a test
  client.
- Minecraft 26.3 (protocol 777) packet table generated from the official jar.
- VarInt/VarLong, strings, UUID, NBT, framing, compression, AES/CFB8
  encryption, packet ids and limits.
- Status ping with MOTD, version, player counts and pong.
- Login: name checks, protocol checks, compression, login success.
- Online mode: RSA key exchange, 4-byte token verification, signed SHA-1,
  Mojang `hasJoined`, verified UUID/name/properties, timeouts.
- Configuration: brand, feature flags, empty known packs, 32 dynamic
  registries, all static and dynamic tags, finish configuration.
- Play: join game, dimension type, spawn, teleport + acknowledgement,
  keep-alives, chunk batch, player info, entity spawn/despawn, movement
  replication, chat and commands.
- Static flat world with cached shared chunk packets.
- Bounded per-player queues, connection semaphore, idle/keep-alive timeouts,
  slow-client kicks, graceful shutdown (Ctrl+C and SIGTERM).
- `GameInput`/`GameState`/`GameSession`/`GameLoop`/`SessionManager`.
- Versioned `ProtocolAdapter` seam; the 26.3 adapter maps clicks/punches to
  game input.
- Map renderer with configurable rate (`game_update_hz`, 1–20) and
  dirty-rectangle updates after the first full frame.
- `/flappy` is playable on an unmodified 26.3 client with a filled map.
- Mech Wars: 12×24×8 voxel model, component hierarchy, 598-voxel outer shell
  rendered as 1×1×1 grid-aligned block displays, `/mech spawn|despawn|status|
  reset|help`, replay for late joiners, cleanup on disconnect, two-mech limit.
- Mech Wars control panel: 18 Interaction buttons, block/text displays, owner
  checks, panel rotated with the player.
- Mech Wars orders: forward/back/stop, body turn, torso turn/reset, speed,
  emergency stop; acceleration/deceleration, arena bounds, 20 TPS sync.
- RSS and network/map counters.

## 3. Changed files

Everything under `crates/`, `games/`, `tools/`, `data/versions/26.3/`,
`configs/`, `docs/`, `scripts/`, plus root manifests, `README.md`,
`NOTICE.md`, `LICENSE`, `rust-toolchain.toml` and `.gitignore`.

## 4. Architectural decisions

See `docs/DECISIONS.md` (D1–D11). Key points: adapted MIT Garnet wire layer,
own server logic, Java-free runtime using generated vanilla data, full
registry data with empty known packs, deterministic registry order, flat
world, distance visibility, online-mode safety latch, map renderer, bounded
queues, versioned input adapters.

## 5. Libraries used

See `docs/DEPENDENCIES.md`. Main: Tokio, bytes, serde, thiserror, uuid,
flate2, aes/cfb8, rsa/num-bigint/sha1, reqwest (rustls), `minecraft_data`
4.2.0+26.3, tracing, toml.

## 6. Build result

- `cargo build --workspace`: pass
- `cargo build --release --locked`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo fmt --all -- --check`: pass

## 7. Test result

`cargo test --workspace`: **56 tests passed, 0 failed**. Highlights:

- VarInt known vectors, overflow rejection, string round trips.
- Framing: plain, compressed, encrypted, fragmented reads, multiple packets
  in one read, plus a deterministic arbitrary-bytes robustness test.
- NBT JSON conversion, network round trip and depth limit.
- Text components, UUID offline rule, BlockPos packing.
- 26.3 packet ids, dynamic registries, static/dynamic tags, block/item/
  component ids.
- Map packet byte layout and dirty-rectangle diffing.
- Two real TCP integration tests: status ping; two clients logging in,
  seeing each other, moving and removing the entity; Flappy map frames.
- A 20-client integration test, all loading chunks and seeing peers.
- A reconnect integration test: a second login replaces the first session
  without duplicating it.
- Runtime session/timestep/manager tests.
- World chunk/heightmap tests.
- Flappy physics, spawn, game over/restart tests.
- Mech voxel grouping, component hierarchy and spawn/metadata packet counts.
- Mech protocol integration: one mech with panel (90+ display/interaction
  packets) and two mechs with panels replicated to both clients.
- Panel command mapping, forward acceleration/stop, torso clamp/reset,
  emergency stop and display sync tests.

## 8. Confirmed Minecraft versions

- 26.3 / protocol 777 / data version 5023.
- Official Minecraft 26.3 GUI client: connected to an `online_mode = true`
  server, passed login, configuration, world join and `/flappy` rendering.
- Protocol client: two and twenty sessions, movement, entity removal and map
  frames.
- Not yet checked with real clients: two official clients in one world,
  reconnect under real latency, 10/20 real clients.
- 26.2 and older are unsupported; they receive a clear version rejection.

## 9. Measured memory

Release build, Windows 11, view distance 3, 49 chunks per client:

- idle: 0.6–6.1 MiB across runs (Windows trims idle pages)
- 20 protocol clients: 11.2–16.8 MiB, sampled peak 12.4–17.3 MiB
- after all clients left: about 0.6 MiB

Targets were ≤50 MiB idle and ≤100 MiB at 20 players, so both pass. Linux
release measurement is still pending.

## 10. Known issues / limitations

- No two-real-client verification yet.
- No vanilla world generation, physics, inventories, block editing, chat
  signing, mobs, redstone or survival.
- Movement is not speed/fly validated; a modified client can teleport.
- Map rendering is 128×128, scaled from a 32×24 game frame. Dirty rectangles
  are implemented; partial-update quality/measurements at 5/10/20 Hz are not
  yet documented.
- Only 26.3 is supported.
- Mech Wars renderer, panel and movement have been verified at the packet
  level; the official-client visual and click check is still pending.
- Mech Wars weapons, projectiles, voxel damage and component failure are not
  implemented yet.
- Memory numbers are Windows-only in this report.
- `SessionManager` and `GameLoop` exist and are tested; the server's game hub
  uses `GameLoop` timing but not `SessionManager` as its session store.

## 11. Next task

1. Two official clients: verify visibility, movement, disconnect and
   reconnect; update `docs/COMPATIBILITY.md`.
2. Run the 20-player methodology on Linux release and record every requested
   metric (RSS, peak, CPU, allocations, throughput, queues, connections).
3. Add 26.2 with its own packet table/registry snapshot and adapter, then
   repeat the full compatibility flow.
4. Measure map traffic at 5/10/20 Hz and tune the renderer/update policy.
5. Add CI for build, tests, clippy and the Linux memory run.
