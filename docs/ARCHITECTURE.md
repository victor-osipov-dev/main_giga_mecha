# Voxel Arcade RS — Architecture

## 1. Scope

Voxel Arcade RS is a Minecraft Java-compatible server core written in Rust.
It does **not** implement vanilla survival: no terrain generation, mobs,
crafting, redstone, or survival physics. It provides the transport, login,
configuration, a small flat world, authoritative multiplayer presence, and a
game runtime that can expose 2D games through the unmodified client.

The first target is **Minecraft Java 26.3, protocol 777**.

## 2. Data flow

```
TCP bytes
  │  arcade-network::Connection
  │  ├─ Codec (length, compression, AES/CFB8)
  │  ├─ PacketIds (name ↔ numeric id from packets.json)
  │  └─ state machine: Handshake → Status | Login → Configuration → Play
  │
  ├─ Status   → status response / pong
  ├─ Login    → arcade-auth (RSA, Mojang session server)
  ├─ Config   → arcade-protocol-data (registries, tags)
  └─ Play     → arcade-world (flat chunks)
                arcade-players (sessions, outbound queues)
                arcade-network::game (map-rendered games)
                arcade-network::adapter (versioned input mapping)
                arcade-runtime (game sessions, fixed timestep)
```

Game logic never sees Minecraft packets. It receives `GameInput` values and
returns a `Frame`. `arcade-render` converts frames to map colours and
`arcade-network::game` wraps them into `map_item_data` packets.

## 3. Crates

| Crate | Responsibility |
|---|---|
| `arcade-protocol` | Wire types, VarInt, framing, compression, encryption, NBT, packets for 26.3 |
| `arcade-protocol-data` | Version data: packet id table, dynamic registries, tags, static block/item ids |
| `arcade-auth` | RSA key pair, Minecraft signed hash, online-mode session check |
| `arcade-players` | Player sessions, entity ids, per-player bounded outbound queues, visibility sets |
| `arcade-world` | Deterministic flat world, paletted chunk sections, heightmaps, light data |
| `arcade-network` | Listener, state machine, status/login/config/play, replication, versioned input adapter, game rendering |
| `arcade-runtime` | `GameInput`, `GameState`, `GameSession`, `GameLoop`, `SessionManager`, `Frame` |
| `arcade-render` | Frame → 128×128 map colour buffer |
| `arcade-metrics` | Atomic counters, RSS reader (Linux procfs / Windows PSAPI) |
| `arcade-server` | Binary, configuration, logging, metrics ticker |
| `flappy-bird` | Server-side Flappy Bird-like game |
| `mech-wars` | Voxel mechs: pure simulation + block-display packets |
| `test-client` | Protocol-level client used by integration tests and manual checks |

## 4. Connection state machine

Each `Connection` has exactly one `State`:

1. **Handshake** — one packet. `Intent::Status` enters status, `Intent::Login`
   or `Intent::Transfer` enters login.
2. **Status** — server list ping and pong. It never reaches play.
3. **Login** — name validation, protocol check, optional encryption, session
   verification, compression negotiation, `LoginSuccess`.
4. **Configuration** — brand, feature flags, known packs, full dynamic
   registries, all vanilla tags (static and dynamic), `FinishConfiguration`.
5. **Play** — join game, chunk batch, player list/entities, movement,
   keep-alives, chat/commands, maps.

Packets that are unknown in the current state are ignored when the client
would reasonably send them; malformed packets end the connection with a kick.
A packet id that does not exist for the state is a protocol error.

## 5. Protocol approach

The project follows approach **A** from the plan: reuse a proven wire layer
instead of re-implementing Minecraft's protocol from scratch. The wire layer
is adapted from Garnet (MIT, pinned revision in `NOTICE.md`). The server
logic, version data loading, world, player management, runtime, renderer and
game are original code.

Packet *ids* are never hard-coded. `packets.json` from Mojang's data generator
is embedded in `arcade-protocol-data` and turned into bidirectional lookup
tables. Protocol fields live in one place per direction; each version gets
its own `ProtocolAdapter` and packet modules. See `docs/VERSION_ADAPTERS.md`.

## 6. Version data

`data/versions/26.3` contains:

- `version.json` — protocol 777, world version 5023.
- `generated/reports/packets.json` — official packet names and ids.
- `datapack/minecraft/...` — dynamic registries and tags extracted from the
  official server jar.

The data is generated once by `scripts/prepare-data.ps1`. The Rust server
never launches Java. Static ids (blocks, items, entity types, component
types) come from the compact `minecraft_data` crate for 26.3.

Registry entries are sorted by identifier and sent in a fixed order. This
makes numeric ids deterministic without copying vanilla's internal ordering.
Chunk biome ids and the dimension type id are computed from the same order.

## 7. World

The world is one dimension (`minecraft:overworld`), 384 blocks tall, with a
grass platform at y=64 in every chunk. Chunks are generated in memory and
encoded once, then cached as shared `Bytes`:

- one chunk column = 24 sections;
- uniform sections use a 0-bit palette;
- ground sections use a 4-bit indirect palette;
- heightmaps: world surface, motion blocking, motion blocking no leaves;
- light: full-bright sky light, no block light.

The flat world is intentionally boring and cheap: no block arrays for empty
sections, no disk I/O, no generation tasks.

## 8. Players and replication

`PlayerManager` owns the authoritative session map. Each `Player` has:

- verified UUID and profile;
- server-assigned entity id;
- position, rotation, game mode, loaded chunks;
- set of visible player UUIDs;
- bounded outbound queue (512 messages).

Visibility is distance-based (Chebyshev chunk distance). When a player
finishes loading, it spawns every other loaded player within view, and all
other loaded players receive the newcomer. Movement is broadcast only to
players that currently see the mover. Disconnects send `player_info_remove`
and `remove_entities`.

A slow client never blocks the tick loop: `try_send` marks the session as
slow, and its connection task is kicked on the next iteration. Queues are
bounded so a stalled client cannot grow memory without limit.

## 9. Authentication

Online mode is the default. Login flow:

1. Server sends an empty server id, a DER public key and a 4-byte token.
2. Client encrypts a 16-byte shared secret and the token with RSA PKCS#1 v1.5.
3. Server decrypts, checks the token, and enables AES-128/CFB8.
4. Server computes Minecraft's signed-hex SHA-1 of `secret || public_key`,
   asks `sessionserver.mojang.com/session/minecraft/hasJoined`, and uses the
   returned UUID/name/properties. The client's claimed UUID is ignored.
5. Failures are kicked before configuration.

Offline mode requires `online_mode = false` and is refused on a non-loopback
address unless `allow_offline_public = true` is set explicitly. It exists for
integration tests and isolated development only.

## 10. Game runtime and rendering

`GameSession` holds a boxed `GameState`, player contexts and a tick counter.
`GameLoop` is a fixed-timestep accumulator (default 10 Hz for the map
experiment) and is independent of TCP.

The first renderer uses a vanilla filled map: `/flappy` puts a filled map in
hotbar slot 0 and sends a 128x128 frame through `map_item_data`. The game rate
is configurable (`game_update_hz`, 1-20). After the first full frame, only the
smallest changed rectangle is sent. Input is mapped by `ProtocolAdapter`
(`use_item`, `use_item_on`, `punch` -> `GameInput::Jump`). Each player has an
independent session and map id. This avoids client mods and custom protocols.

## 11. Limits and failure handling

| Limit | Value |
|---|---|
| Minecraft packet size | 2 MiB |
| String length | per field (name 16, chat 256, command 32767) |
| TCP connections | configurable, default 128 |
| Players | configurable, default 20 |
| Outbound queue per player | 512 messages |
| Idle timeout | 30 s |
| Keep-alive period / timeout | 15 s / 35 s |
| View distance | 2–12, default 3 |
| Game update rate | 1–20 Hz, default 10 |
| Chunk cache | bounded by observed positions; cleared above 4096 entries |

`VarInt` overflow, negative lengths, truncated reads, invalid UTF-8, invalid
NBT and decompression size mismatches are typed `ProtocolError`s. I/O errors
end only the affected task.

## 12. What is not implemented yet

- Vanilla world generation, block breaking/placing, inventories, combat,
  physics, mobs, redstone, chat signing.
- Per-version adapters (26.2, 26.1, 1.21.x).
- Rendering optimizations (partial map patches, change detection).
- Linux release benchmark automation (the included metrics are measured
  manually; see `MEMORY_BUDGET.md`).
- Verification with an official GUI client in this environment. All
  protocol-level flows are covered by the automated test client; the
  compatibility matrix states this honestly.
