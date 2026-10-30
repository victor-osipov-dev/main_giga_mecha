//! A tiny, deterministic flat world used by the arcade core.
//!
//! The world is 16x16 blocks per chunk, 384 blocks tall. Every chunk in view
//! contains a grass platform at y = 64, so players can walk forever without
//! any vanilla terrain generation.

use arcade_protocol::packets::play::clientbound::{ChunkData, LightData};
use arcade_protocol::ChunkPos;
use arcade_protocol_data::{Blocks, GameData};
use std::collections::HashMap;

pub const MIN_Y: i32 = -64;
pub const HEIGHT: i32 = 384;
pub const SECTION_COUNT: usize = (HEIGHT / 16) as usize;
pub const PLATFORM_Y: i32 = 64;

const SECTION_VOLUME: usize = 16 * 16 * 16;
const BLOCK_INDIRECT_MIN_BITS: u8 = 4;
const BLOCK_INDIRECT_MAX_BITS: u8 = 8;

/// A statically generated world. Chunk rows are identical, so the only
/// per-chunk state is the position passed to the encoder.
pub struct FlatWorld {
    pub spawn: (f64, f64, f64),
    pub sea_level: i32,
    pub biome_id: i32,
    pub dimension_type_id: i32,
    blocks: Blocks,
    /// Cached, fully encoded chunk payloads keyed by chunk position.
    cache: std::sync::Mutex<HashMap<ChunkPos, Arc<ChunkData>>>,
}

use std::sync::Arc;

impl FlatWorld {
    pub fn new(data: &GameData) -> Self {
        let biome_id = data
            .registry_id("worldgen/biome", "plains")
            .or_else(|| data.registry_id("worldgen/biome", "the_void"))
            .unwrap_or(0);
        let dimension_type_id = data.registry_id("dimension_type", "overworld").unwrap_or(0);
        Self {
            spawn: (0.5, (PLATFORM_Y + 1) as f64, 0.5),
            sea_level: PLATFORM_Y,
            biome_id,
            dimension_type_id,
            blocks: Blocks,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn chunk(&self, pos: ChunkPos) -> Arc<ChunkData> {
        if let Some(hit) = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pos)
            .cloned()
        {
            return hit;
        }
        let chunk = Arc::new(self.build_chunk(pos));
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        // Bounded: a view radius of 16 needs at most ~1089 entries; anything
        // more is stale and can be dropped.
        if cache.len() > 4096 {
            cache.clear();
        }
        cache.insert(pos, Arc::clone(&chunk));
        chunk
    }

    fn build_chunk(&self, pos: ChunkPos) -> ChunkData {
        let air = self.blocks.state_or_air("air");
        let stone = self.blocks.state_or_air("stone");
        let grass = self.blocks.state_or_air("grass_block");

        let mut sections = Vec::with_capacity(SECTION_COUNT * 64);
        for section in 0..SECTION_COUNT {
            let section_floor = MIN_Y + (section as i32) * 16;
            if section_floor > PLATFORM_Y || section_floor + 15 < PLATFORM_Y - 1 {
                encode_uniform(&mut sections, air, self.biome_id);
            } else {
                encode_ground_layer(
                    &mut sections,
                    section_floor,
                    air,
                    stone,
                    grass,
                    self.biome_id,
                );
            }
        }

        let heights = [PLATFORM_Y - MIN_Y + 1; 256].map(|h| h as u32);
        let packed = pack_heightmap(&heights, HEIGHT);
        ChunkData {
            chunk_x: pos.x,
            chunk_z: pos.z,
            heightmaps: vec![(1, packed.clone()), (4, packed.clone()), (5, packed)],
            sections,
            block_entities: Vec::new(),
            light: full_bright_light(SECTION_COUNT),
        }
    }
}

fn encode_uniform(out: &mut Vec<u8>, state: u16, biome: i32) {
    // non-air, liquid
    out.extend_from_slice(&0i16.to_be_bytes());
    out.extend_from_slice(&0i16.to_be_bytes());
    out.push(0); // block state bits-per-entry 0 = uniform
    write_varint(out, state as i32);
    out.push(0); // biome bits-per-entry 0 = uniform
    write_varint(out, biome);
}

fn encode_ground_layer(
    out: &mut Vec<u8>,
    section_floor: i32,
    air: u16,
    stone: u16,
    grass: u16,
    biome: i32,
) {
    let mut states = vec![air; SECTION_VOLUME];
    for z in 0..16 {
        for x in 0..16 {
            let global_63 = 63 - section_floor;
            if (0..16).contains(&global_63) {
                states[index(x, global_63 as usize, z)] = stone;
            }
            let global_64 = 64 - section_floor;
            if (0..16).contains(&global_64) {
                states[index(x, global_64 as usize, z)] = grass;
            }
        }
    }

    let mut palette: Vec<u16> = Vec::new();
    let mut indices = vec![0u16; SECTION_VOLUME];
    for (i, &state) in states.iter().enumerate() {
        let idx = match palette.iter().position(|&s| s == state) {
            Some(p) => p,
            None => {
                palette.push(state);
                palette.len() - 1
            }
        };
        indices[i] = idx as u16;
    }

    let non_air = states.iter().filter(|&&s| s != air).count() as i16;
    out.extend_from_slice(&non_air.to_be_bytes());
    out.extend_from_slice(&0i16.to_be_bytes());

    if palette.len() == 1 {
        out.push(0);
        write_varint(out, palette[0] as i32);
    } else {
        let (bits, packed) = if palette.len() <= (1 << BLOCK_INDIRECT_MAX_BITS) {
            let bits = bits_for(palette.len()).max(BLOCK_INDIRECT_MIN_BITS);
            out.push(bits);
            write_varint(out, palette.len() as i32);
            for &state in &palette {
                write_varint(out, state as i32);
            }
            (bits, indices.iter().map(|&i| i as u64).collect::<Vec<_>>())
        } else {
            let bits = bits_for(4096).max(8);
            out.push(bits);
            (
                bits,
                indices
                    .iter()
                    .map(|&i| palette[i as usize] as u64)
                    .collect(),
            )
        };
        write_packed(out, &packed, SECTION_VOLUME, bits);
    }

    out.push(0);
    write_varint(out, biome);
}

fn index(x: usize, y: usize, z: usize) -> usize {
    (y << 8) | (z << 4) | x
}

fn bits_for(count: usize) -> u8 {
    if count <= 1 {
        return 0;
    }
    (usize::BITS - (count - 1).leading_zeros()) as u8
}

fn write_varint(out: &mut Vec<u8>, mut value: i32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value = ((value as u32) >> 7) as i32;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn write_packed(out: &mut Vec<u8>, values: &[u64], count: usize, bits: u8) {
    let per_long = 64 / bits as usize;
    let long_count = count.div_ceil(per_long);
    let mut longs = vec![0u64; long_count];
    for (i, &value) in values.iter().enumerate() {
        let long_index = i / per_long;
        let shift = (i % per_long) * bits as usize;
        longs[long_index] |= value << shift;
    }
    for long in longs {
        out.extend_from_slice(&long.to_be_bytes());
    }
}

fn pack_heightmap(heights: &[u32; 256], world_height: i32) -> Vec<i64> {
    let bits = bits_for(world_height as usize + 1).max(1);
    let values: Vec<u64> = heights.iter().map(|&h| h as u64).collect();
    let mut bytes = Vec::new();
    write_packed(&mut bytes, &values, 256, bits);
    bytes
        .chunks(8)
        .map(|c| i64::from_be_bytes(c.try_into().unwrap()))
        .collect()
}

/// Full sky light so a custom world looks lit without running a light engine.
fn full_bright_light(section_count: usize) -> LightData {
    let bits = section_count + 2;
    let words = mask_words(bits);
    LightData {
        sky_mask: words.clone(),
        block_mask: Vec::new(),
        empty_sky_mask: Vec::new(),
        empty_block_mask: words,
        sky_arrays: (0..bits).map(|_| vec![0xFF; 2048]).collect(),
        block_arrays: Vec::new(),
    }
}

fn mask_words(bits: usize) -> Vec<u64> {
    let mut words = vec![0u64; bits.div_ceil(64)];
    for bit in 0..bits {
        words[bit / 64] |= 1 << (bit % 64);
    }
    words
}

/// Number of chunks in a square view.
pub fn view_chunks(center: ChunkPos, radius: i32) -> Vec<ChunkPos> {
    let mut out = Vec::with_capacity(((radius * 2 + 1) * (radius * 2 + 1)) as usize);
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            out.push(ChunkPos::new(center.x + dx, center.z + dz));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcade_protocol::PacketWriter;
    use std::path::Path;

    fn data() -> GameData {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        GameData::load(&dir).unwrap()
    }

    #[test]
    fn chunk_encodes_without_panicking() {
        let data = data();
        let world = FlatWorld::new(&data);
        let chunk = world.chunk(ChunkPos::new(0, 0));
        assert_eq!(chunk.chunk_x, 0);
        assert!(!chunk.sections.is_empty());
        // Encode the packet body to exercise the writer path.
        let mut w = PacketWriter::new();
        w.write_i32(chunk.chunk_x);
        w.write_i32(chunk.chunk_z);
        assert!(!w.into_inner().is_empty());
    }

    #[test]
    fn heightmap_is_stable() {
        let h = [PLATFORM_Y - MIN_Y + 1; 256].map(|v| v as u32);
        let packed = pack_heightmap(&h, HEIGHT);
        assert_eq!(packed.len(), 37);
    }
}
