# Architectural decisions

Each decision records the context, the choice, and the consequences.

## D1 — Protocol: reuse a proven wire layer (approach A), own server

**Context.** The plan allows a library-based protocol layer if it is maintained,
lightweight, and actually supports server-side 26.3. Valence is pinned to
1.20.1; Azalea's protocol crate covers 26.1; Garnet is MIT and targets 26.3.

**Decision.** Reuse the wire layer from Garnet (MIT, revision recorded in
`NOTICE.md`) as `arcade-protocol`, and write the server, data loading, world,
players, runtime and renderer ourselves. No GPL code from Pumpkin is copied.
Garnet is used only as an MIT-licensed protocol implementation and as a
reference for server flow.

**Consequences.** Correct 26.3 framing, NBT, compression and packet layouts
without inventing them. We own all game logic. The adaptation is pinned and
reviewable; the license notice is in `NOTICE.md`. Future protocol versions can
reuse the same crate layout with separate packet tables.

## D2 — Version data generated from the official jar, server is Java-free

**Context.** 26.3 dynamic registries and packet ids are version-specific.
Hand-copying them is error-prone. Minecraft server jars are bundlers and the
data generator is built in.

**Decision.** `scripts/prepare-data.ps1` downloads the official 26.3 server
jar, verifies its SHA-1, extracts the inner jar and data pack, and runs
Mojang's `net.minecraft.data.Main --reports`. Only the resulting JSON/NBT
payloads and packet table are committed. The Rust server does not launch Java.

**Consequences.** Data is reproducible and auditable (`SOURCE.txt`). The
server binary needs no JVM, no network at runtime, and no game jar. Version
bumps require running the script again, not rewriting packet code.

## D3 — Static ids from `minecraft_data` instead of shipping blocks.json

**Context.** `blocks.json` from the generator is 7.3 MB of JSON and would cost
parsing time and transient memory at startup. The server needs a handful of
block states and item/component ids.

**Decision.** Use the MIT/Apache `minecraft_data 4.2.0+26.3` crate, which
compiles compact static tables. Dynamic registries still come from the
official data pack.

**Consequences.** Small startup cost and RSS. The crate uses `unsafe`
internally; it is a well-scoped, version-pinned dependency. Block-state and
item ids are validated by tests against known vanilla entries.

## D4 — Always send full registry data, empty known packs

**Context.** Known-pack negotiation normally lets a server skip registry
entries the client already has. That requires knowing exactly what each
client build ships and invites subtle version coupling.

**Decision.** Send an empty `select_known_packs`, then full `Registry Data`
for all 32 synced registries. Registry payloads are encoded once at startup
and shared across connections.

**Consequences.** More bytes per login (about 0.7 MB before compression), but
simple, version-independent correctness. Compression keeps the wire cost
reasonable. The payloads are cached, so CPU and memory do not grow per
player. Tags are sent for every registry the data pack defines, including
static block/item/entity tags referenced by dimension types and enchantments.

## D5 — Deterministic alphabetic registry order

**Context.** Registry ids are assigned by packet order, not by a global
constant.

**Decision.** Sort entries by identifier before assigning indices. Send
registry packets in a fixed list order. Use the same indices for chunks and
dimension types.

**Consequences.** Stable ids across restarts and platforms. We do not depend
on Mojang's internal registry iteration order. Any future version adapter must
follow the same rule.

## D6 — Flat world in memory, encoded chunks shared as `Bytes`

**Context.** Vanilla generation is out of scope and would dominate CPU, memory
and complexity.

**Decision.** One 384-block overworld with a grass platform at y=64 in every
chunk. Generate sections from palettes, encode `level_chunk_with_light` once
per chunk position, and cache the payload. Cache is bounded and shared by all
players.

**Consequences.** Very low RSS (about 53 KB per cached chunk with full-bright
sky arrays), no disk I/O, no generation tasks. The world is intentionally
limited; gameplay is the arcade runtime. If the cache is cleared, chunks are
regenerated deterministically.

## D7 — Authoritative presence, distance-based visibility

**Context.** The plan requires a real multiplayer experience without vanilla
physics or survival.

**Decision.** The server owns position/rotation and player sessions. Chunk
load, movement and visibility are explicit sets on each player. Entities are
spawned only after `player_loaded`. Movement is broadcast to watchers only.

**Consequences.** Two clients see each other, movement and disconnects without
duplicating state. No physics validation yet, so a modified client can move
freely; this is acceptable for an arcade server but should be tightened before
public competitive use.

## D8 — Online mode by default; offline mode has a safety latch

**Context.** Public servers must not silently accept offline identities.
Integration tests need offline mode.

**Decision.** `online_mode = true` by default. Offline mode on a non-loopback
address is refused unless `allow_offline_public = true`. Session server calls
have connect/read timeouts.

**Consequences.** Tests run on loopback without special configuration. A
misconfigured public deployment fails loudly instead of becoming an account
spoofing hole.

## D9 — Games render through a vanilla filled map

**Context.** The plan asks for a visual game on an unmodified client. Map item
data is a built-in mechanism with a small, stable packet.

**Decision.** `/flappy` gives the player a filled map in hotbar slot 0 and
sends a full 128×128 frame once, then only the smallest changed rectangle.
The rate is configurable (1–20 Hz). Right-click or punch is the jump input.
Sessions are per player and removed on disconnect.

**Consequences.** No client mods and no custom protocol. The map is limited to
128×128 pixels and full-frame updates are used for now. Input is coarse
(right-click/punch), which is acceptable for Flappy Bird and documented for
future 2D games.

## D10 — Bounded queues and one task per connection

**Context.** A slow or malicious client must not stall the server or grow
memory without bound.

**Decision.** One Tokio task per connection, a bounded outbound channel, a
global connection semaphore, idle timeout, keep-alive timeout, and a slow-
client kick. No per-player OS threads and no per-player process.

**Consequences.** Stable behaviour under disconnects and backpressure.
Throughput is limited by the client's queue; that is intentional.

## D11 — No unsafe in project code

**Context.** The plan forbids unsafe without proven need.

**Decision.** Project crates contain no `unsafe` blocks except the small
Windows RSS FFI in `arcade-metrics` (a direct, size-checked PSAPI call) and
whatever dependencies use internally. The latter is documented and pinned.

**Consequences.** Audit surface is small. The Windows metric call is isolated
and tested only for absence of crashes; on Linux the procfs path is the
supported production measurement.
