# Benchmarks

The first benchmark is dependency-free and lives next to the code it measures:

```powershell
cargo run --release -p arcade-protocol --example bench_codec
```

It reports compressed encode and decode throughput for 64 B to 128 KiB
payloads. The server reports live packet/byte counters through
`arcade-metrics`; use `docs/MEMORY_BUDGET.md` for the 20-player methodology.
