# Dependencies

All versions are resolved in `Cargo.lock`. The table lists the direct
dependencies and the versions actually built for this revision.

## Direct runtime dependencies

| Crate | Resolved | License | Why |
|---|---:|---|---|
| `tokio` | 1.54.1 | MIT | TCP listener, one task per connection, timers, bounded channels |
| `bytes` | 1.12.1 | MIT | Shared chunk/registry packet buffers |
| `serde` / `serde_json` | 1.0.229 / 1.0.154 | MIT OR Apache-2.0 | Configuration, status JSON, registry JSON |
| `thiserror` | 2.0.21 | MIT OR Apache-2.0 | Typed protocol/auth errors |
| `uuid` | 1.29.0 | MIT OR Apache-2.0 | Offline UUID rule, session id, entity/player identity |
| `flate2` | 1.1.10 | MIT OR Apache-2.0 | zlib packet compression |
| `aes` | 0.9.3 | MIT OR Apache-2.0 | AES-128 for online-mode CFB8 |
| `cfb8` | 0.9.1 | MIT OR Apache-2.0 | Minecraft's stream cipher mode |
| `rsa` | 0.9.10 | MIT OR Apache-2.0 | 1024-bit login key and PKCS#1 v1.5 decryption |
| `num-bigint` | 0.5.1 | MIT OR Apache-2.0 | Signed-hex SHA-1 server hash |
| `sha1` | 0.11.0 | MIT OR Apache-2.0 | Minecraft server hash |
| `reqwest` | 0.13.5 | MIT OR Apache-2.0 | Mojang session server (rustls, no OpenSSL) |
| `minecraft_data` | 4.2.0+26.3 | MIT OR Apache-2.0 | Static 26.3 block/item/entity/component ids |
| `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 | MIT | Structured logs |
| `toml` | 1.1.8 | MIT OR Apache-2.0 | Server configuration |
| `anyhow` | 1.0.104 | MIT OR Apache-2.0 | Binary-level error context |

`rand_core` 0.6 (MIT OR Apache-2.0) provides `OsRng` for RSA key generation.

## Vendored source

`crates/arcade-protocol/src` is adapted from Garnet (MIT). The upstream
revision and license text are in `NOTICE.md`. No GPL project code is used.

## Data dependencies (not crates)

- Official Minecraft 26.3 server jar for `packets.json`, registries, tags.
  Checked by SHA-1 in `data/versions/26.3/SOURCE.txt`; generated locally by
  `scripts/prepare-data.ps1`.
- `minecraft_data` crate data is compiled into Rust enums and tables.

## Deliberate omissions

- No async runtime beyond Tokio.
- No database, no web framework, no ORM.
- No `sysinfo`: RSS is read from procfs on Linux and a small PSAPI call on
  Windows.
- No `criterion` in the server binary; the dependency-free codec benchmark is\n  `cargo run --release -p arcade-protocol --example bench_codec`.
- No TLS listener: Minecraft Java uses its own AES/CFB8 login encryption.

## Reproducibility

Use the pinned toolchain or a newer stable, then:

```text
cargo build --release --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`Cargo.lock` is committed. Data generation is pinned by the server jar SHA-1
and the recorded generator command.
