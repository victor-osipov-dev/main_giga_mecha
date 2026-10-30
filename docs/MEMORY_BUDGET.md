# Memory budget

## Targets from the plan

| Scenario | Target | Measured | Result |
|---|---:|---:|---|
| Idle RSS | ≤ 50 MiB | 0.6–6.1 MiB (OS-trimmed) | pass |
| 20 players, simple world | ≤ 100 MiB | 11–17 MiB (peak 17.3) | pass |
| Peak RSS during 20 joins | measured | 12.4–17.3 MiB | pass |

## Measurement setup

- Build: `cargo build --release -p arcade-server -p test-client`
- OS: Windows 11, x86_64, Rust 1.99.1
- Server: exactly one process, one Tokio runtime, one task per connection
- Config: view distance 3, simulation distance 3, compression 256,
  offline mode used only because the test clients are synthetic
- Clients: 20 copies of the protocol-level `test-client`, each logged in,
  loaded 49 chunks, held for 20 s, and moved every 500 ms. The same scenario
  is automated by `twenty_players_connect_and_load_chunks`.
- Metric: OS working set of the `voxel-arcade` process (`GetProcessMemoryInfo`
  on Windows; the same `arcade-metrics` code reads `/proc/self/statm` on Linux)

This is a Windows measurement, not the plan's preferred Linux release run.
Linux numbers are expected to be in the same range or lower; a Linux run
should be repeated in CI before a public release. The measurement code is
platform-independent.

## Results

### Idle

| Build | RSS |
|---|---:|
| Debug | 9.6 MiB |
| Release | 0.6–6.1 MiB across runs (Windows trims idle pages) |

The release binary keeps static tables mapped and is the honest baseline.
Startup parses the registry data pack once (about 0.7 MB JSON).

### 20 players

| Build | RSS with 20 | Sampled peak |
|---|---:|---:|
| Debug | 19.1 MiB | not sampled |
| Release | 11.2–16.8 MiB | 12.4–17.3 MiB |

All 20 clients received 49 chunks and saw 19 other entities. Each reported
hundreds of movement updates and 1-2 removals when a peer left.

## Why the numbers stay low

- Empty chunk sections are stored as palettes, not block arrays.
- Each chunk payload is encoded once and shared as `Bytes`; 49 chunks at
  view distance 3 is about 2.6 MB of shared packet data.
- Registry and tag payloads are encoded once at startup, not per login.
- Per-player state is a few hundred bytes plus the visibility set and a
  bounded outbound queue.
- No per-player process or OS thread.
- No generated world, block entities, or loaded vanilla registries.
- Rust release build with thin LTO and stripped symbols.

## Byte cost per player

| Item | Size |
|---|---:|
| Dynamic registry data | ~0.7 MB before compression |
| 49 chunks | ~2.6 MB before compression |
| Outbound queue capacity | 512 messages, bounded |
| Map frame (first) | 16 KB raw (128×128) |
| Map patch (Flappy, after first) | ~1.6–4.3 KB raw per update in a short run |
| Default game rate | 10 Hz, configurable 1–20 Hz |

Compression substantially reduces registry and chunk traffic on the wire.
The in-memory chunk cache is the largest fixed allocation; it is bounded and
cleared above 4096 entries.

## Known deviations / risks

- The official GUI client may use more server-side memory than the synthetic
  client (larger registry negotiation, more chat/session packets), but the
  server's own state does not depend on client rendering.
- 20 players were connected on loopback. Real-world latency does not increase
  server RSS, but slower clients hold queued chunks longer. The bounded queue
  caps that at 512 messages per player and kicks a client that overflows.
- A future vanilla terrain world would blow the budget; this core deliberately
  stays with static chunks.

## Reproduce

```powershell
cargo build --release -p arcade-server -p test-client
# terminal 1
target/release/voxel-arcade.exe configs/dev-offline.toml
# terminal 2: 20 clients
1..20 | % {
  Start-Process target/release/test-client.exe `
    "login 127.0.0.1:25566 Bot$_ 20"
}
# read Get-Process voxel-arcade | Select WorkingSet64
```

The server logs an RSS/player/packet snapshot every 60 seconds.
