//! Tiny dependency-free microbenchmark for the packet codec.
//!
//! Run with:
//!   cargo run --release -p arcade-protocol --example bench_codec

use arcade_protocol::Codec;
use std::time::Instant;

fn main() {
    let sizes = [64usize, 1024, 8192, 128 * 1024];
    for size in sizes {
        let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let iterations = (20_000_000 / size.max(1)).max(1000);

        let mut encoder = Codec::new();
        encoder.enable_compression(256);
        let start = Instant::now();
        let mut bytes = 0usize;
        for _ in 0..iterations {
            bytes += encoder.encode(&payload).len();
        }
        let encode = start.elapsed();

        let wire = encoder.encode(&payload);
        let mut decoder = Codec::new();
        decoder.enable_compression(256);
        let start = Instant::now();
        for _ in 0..iterations {
            decoder.push_received(&wire);
            let _ = decoder.next_packet().expect("frame").expect("packet");
        }
        let decode = start.elapsed();

        println!(
            "{size:>6} B x {iterations:>6}: encode {:>8.1} MiB/s, decode {:>8.1} MiB/s, wire {} B",
            bytes as f64 / encode.as_secs_f64() / (1024.0 * 1024.0),
            (size * iterations) as f64 / decode.as_secs_f64() / (1024.0 * 1024.0),
            wire.len()
        );
    }
}
