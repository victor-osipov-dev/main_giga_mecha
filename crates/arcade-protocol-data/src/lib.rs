//! Version data for Minecraft Java 26.3 (protocol 777).
//!
//! Static ids (blocks, entity types, packet ids) are compiled in. Dynamic
//! registry payloads are loaded from the vanilla data pack that ships with
//! the server jar; `scripts/prepare-data.ps1` produces the files once, so the
//! server itself never needs Java at runtime.

use anyhow::{Context, Result};
use arcade_protocol::nbt::NbtTag;
use arcade_protocol::packets::config::{RegistryEntry, TagRegistry};
use arcade_protocol::{Identifier, PacketIds};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const MC_VERSION: &str = "26.3";
pub const PROTOCOL_VERSION: i32 = 777;

/// Registries the client expects in `Registry Data` packets, in the order the
/// vanilla server sends them for 26.3.
pub const SYNCED_REGISTRIES: &[&str] = &[
    "worldgen/biome",
    "chat_type",
    "trim_pattern",
    "trim_material",
    "wolf_variant",
    "wolf_sound_variant",
    "pig_variant",
    "pig_sound_variant",
    "frog_variant",
    "cat_variant",
    "cat_sound_variant",
    "cow_sound_variant",
    "cow_variant",
    "chicken_sound_variant",
    "chicken_variant",
    "zombie_nautilus_variant",
    "painting_variant",
    "sulfur_cube_archetype",
    "dimension_type",
    "damage_type",
    "banner_pattern",
    "enchantment",
    "jukebox_song",
    "instrument",
    "test_environment",
    "test_instance",
    "dialog",
    "world_clock",
    "timeline",
    "decorated_pot_pattern",
    "block_transformer",
    "worldgen/block_state_provider",
];

/// Biome fields the client does not need and that would pull in registries we
/// deliberately do not sync (carvers, placed features, spawn data).
const BIOME_KEYS_NOT_SYNCED: &[&str] = &[
    "features",
    "carvers",
    "spawners",
    "spawn_costs",
    "creature_spawn_probability",
];

/// Static blocks and their state ids, compiled in by `minecraft_data`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Blocks;

impl Blocks {
    pub fn state(&self, name: &str) -> Option<u16> {
        minecraft_data::block::from_str(name)
            .ok()
            .map(|b| b.state_default().id())
    }

    pub fn state_or_air(&self, name: &str) -> u16 {
        self.state(name)
            .or_else(|| self.state("air"))
            .unwrap_or_default()
    }

    pub fn name_of(&self, state: u16) -> Option<&'static str> {
        minecraft_data::block_state::new(state).map(|s| s.to_block().name())
    }

    pub fn is_air(&self, state: u16) -> bool {
        matches!(self.name_of(state), Some("air" | "cave_air" | "void_air"))
    }

    pub fn is_liquid(&self, state: u16) -> bool {
        matches!(self.name_of(state), Some("water" | "lava"))
    }

    /// Bits needed for the direct palette encoding.
    pub fn bits_per_state(&self) -> u8 {
        let count = minecraft_data::block_state::MAX as usize + 1;
        bits_for(count).max(4)
    }
}

fn bits_for(count: usize) -> u8 {
    if count <= 1 {
        return 0;
    }
    (usize::BITS - (count - 1).leading_zeros()) as u8
}

/// One data-driven registry with entries in stable id order.
#[derive(Debug, Clone)]
pub struct DynamicRegistry {
    pub id: Identifier,
    pub entries: Vec<(Identifier, NbtTag)>,
}

impl DynamicRegistry {
    pub fn index_of(&self, name: &str) -> Option<i32> {
        let wanted = Identifier::parse(name)?;
        self.entries
            .iter()
            .position(|(id, _)| *id == wanted)
            .map(|i| i as i32)
    }
}

/// Resolved tags: registry -> (tag name, member ids).
pub type TagRegistryData = Vec<(Identifier, Vec<(Identifier, Vec<i32>)>)>;

/// Name -> protocol id tables for static (built-in) registries.
///
/// Tags are not limited to synced registries: enchantments reference item
/// tags, dimension types reference block tags, and so on. The client needs
/// those tags in the configuration state, and their members must use the
/// official protocol ids.
#[derive(Debug, Default)]
pub struct StaticRegistries {
    map: HashMap<String, HashMap<String, i32>>,
}

impl StaticRegistries {
    pub fn from_report(json: &Value) -> Result<Self> {
        let root = json
            .as_object()
            .context("registries.json is not an object")?;
        let mut map = HashMap::with_capacity(root.len());
        for (registry_name, def) in root {
            let mut entries = HashMap::new();
            if let Some(values) = def.get("entries").and_then(|e| e.as_object()) {
                for (name, data) in values {
                    if let Some(id) = data.get("protocol_id").and_then(|v| v.as_i64()) {
                        entries.insert(with_namespace(name), id as i32);
                    }
                }
            }
            map.insert(with_namespace(registry_name), entries);
        }
        Ok(Self { map })
    }

    pub fn contains(&self, registry: &str) -> bool {
        self.map.contains_key(&with_namespace(registry))
    }

    pub fn id_of(&self, registry: &str, name: &str) -> Option<i32> {
        self.map
            .get(&with_namespace(registry))?
            .get(&with_namespace(name))
            .copied()
    }
}

/// Everything the server needs to know about Minecraft 26.3.
pub struct GameData {
    pub version: String,
    pub protocol_version: i32,
    pub data_version: i32,
    pub packet_ids: PacketIds,
    pub registries: Vec<DynamicRegistry>,
    pub static_registries: StaticRegistries,
    pub tags: TagRegistryData,
    pub blocks: Blocks,
}

impl GameData {
    /// Loads a prepared version directory, normally `data/versions/26.3`.
    pub fn load(version_dir: &Path) -> Result<Self> {
        let version_json: Value = read_json(&version_dir.join("version.json"))
            .context("reading version.json (run scripts/prepare-data.ps1)")?;
        let protocol_version = version_json["protocol_version"]
            .as_i64()
            .unwrap_or(PROTOCOL_VERSION as i64) as i32;
        let data_version = version_json["world_version"].as_i64().unwrap_or(0) as i32;
        let version = version_json["id"].as_str().unwrap_or(MC_VERSION).to_owned();

        let packets: Value = serde_json::from_str(include_str!("../data/packets-26.3.json"))
            .context("parsing embedded packets.json")?;
        let packet_ids = PacketIds::from_report(&packets, protocol_version)
            .map_err(|e| anyhow::anyhow!("packet id table: {e}"))?;

        let registries = load_registries(&version_dir.join("datapack"))?;
        let static_registries = StaticRegistries::from_report(&read_json(
            &version_dir.join("generated/reports/registries.json"),
        )?)?;
        let tags = load_tags(
            &version_dir.join("datapack"),
            &registries,
            &static_registries,
        )?;

        Ok(Self {
            version,
            protocol_version,
            data_version,
            packet_ids,
            registries,
            static_registries,
            tags,
            blocks: Blocks,
        })
    }

    pub fn registry(&self, name: &str) -> Option<&DynamicRegistry> {
        let wanted = Identifier::parse(name)?;
        self.registries.iter().find(|r| r.id == wanted)
    }

    pub fn registry_id(&self, registry: &str, entry: &str) -> Option<i32> {
        self.registry(registry)?.index_of(entry)
    }

    /// Numeric id of a static entity type, e.g. `player` or `block_display`.
    pub fn entity_type_id(&self, name: &str) -> Option<i32> {
        minecraft_data::entity_type::from_str(name)
            .ok()
            .map(|entity| entity.id() as i32)
    }

    /// Numeric id of `minecraft:player` in the static entity type registry.
    pub fn player_entity_type_id(&self) -> i32 {
        minecraft_data::entity_type::player.id() as i32
    }

    /// Numeric id of a static item, e.g. `filled_map`.
    pub fn item_id(&self, name: &str) -> Option<i32> {
        minecraft_data::item::from_str(name)
            .ok()
            .map(|item| item.id() as i32)
    }

    /// Numeric id of a data component type, e.g. `map_id`.
    pub fn component_id(&self, name: &str) -> Option<i32> {
        minecraft_data::data_component_type::from_str(name)
            .ok()
            .map(|component| component.id() as i32)
    }

    /// `Registry Data` payloads for the configuration state.
    pub fn registry_packets(&self) -> Vec<(Identifier, Vec<RegistryEntry>)> {
        self.registries
            .iter()
            .map(|reg| {
                let entries = reg
                    .entries
                    .iter()
                    .map(|(id, nbt)| RegistryEntry {
                        id: id.clone(),
                        data: Some(nbt.clone()),
                    })
                    .collect();
                (reg.id.clone(), entries)
            })
            .collect()
    }

    /// `Update Tags` payloads for every registry the data pack defines tags
    /// for, including static registries such as blocks and items.
    pub fn tag_packets(&self) -> Vec<TagRegistry> {
        self.tags
            .iter()
            .map(|(registry, tags)| TagRegistry {
                registry: registry.clone(),
                tags: tags.clone(),
            })
            .collect()
    }
}

fn read_json(path: &Path) -> Result<Value> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn load_registries(datapack: &Path) -> Result<Vec<DynamicRegistry>> {
    let mut out = Vec::with_capacity(SYNCED_REGISTRIES.len());
    for reg_name in SYNCED_REGISTRIES {
        let mut entries = Vec::new();
        for namespace_dir in read_dirs(datapack)? {
            let namespace = namespace_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("minecraft")
                .to_owned();
            let reg_dir = namespace_dir.join(reg_name);
            if !reg_dir.is_dir() {
                continue;
            }
            for file in json_files(&reg_dir)? {
                let rel = file.strip_prefix(&reg_dir)?.with_extension("");
                let entry_name = rel.to_string_lossy().replace('\\', "/");
                let mut json = read_json(&file)?;
                if *reg_name == "worldgen/biome" {
                    if let Some(obj) = json.as_object_mut() {
                        for key in BIOME_KEYS_NOT_SYNCED {
                            obj.remove(*key);
                        }
                    }
                }
                entries.push((
                    Identifier::new(&namespace, &entry_name),
                    NbtTag::from_json(&json),
                ));
            }
        }
        entries.sort_by_key(|a| a.0.clone());
        out.push(DynamicRegistry {
            id: Identifier::minecraft(reg_name),
            entries,
        });
    }
    Ok(out)
}

fn load_tags(
    datapack: &Path,
    registries: &[DynamicRegistry],
    static_registries: &StaticRegistries,
) -> Result<TagRegistryData> {
    // registry path -> tag name -> raw values
    let mut per_registry: BTreeMap<String, BTreeMap<String, Vec<(String, bool)>>> = BTreeMap::new();

    for namespace_dir in read_dirs(datapack)? {
        let namespace = namespace_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("minecraft")
            .to_owned();
        let tags_dir = namespace_dir.join("tags");
        if !tags_dir.is_dir() {
            continue;
        }
        for file in json_files(&tags_dir)? {
            let rel = file.strip_prefix(&tags_dir)?.with_extension("");
            let rel = rel.to_string_lossy().replace('\\', "/");
            let Some((registry_path, tag_path)) =
                split_registry_and_tag(&rel, registries, static_registries)
            else {
                continue;
            };
            let json = read_json(&file)?;
            let values = json
                .get("values")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(raw_tag_value).collect())
                .unwrap_or_default();
            per_registry
                .entry(registry_path.to_owned())
                .or_default()
                .insert(format!("{namespace}:{tag_path}"), values);
        }
    }

    let mut out = Vec::new();
    for (registry_path, tags) in per_registry {
        let registry_ident = Identifier::minecraft(&registry_path);
        let dynamic = registries.iter().find(|r| r.id == registry_ident);
        if dynamic.is_none() && !static_registries.contains(&registry_path) {
            continue;
        }
        let lookup = |name: &str| {
            dynamic
                .and_then(|r| r.index_of(name))
                .or_else(|| static_registries.id_of(&registry_path, name))
        };
        let mut cache: HashMap<String, Vec<i32>> = HashMap::new();
        let mut resolved = Vec::new();
        for tag_name in tags.keys() {
            let mut visiting = HashSet::new();
            let ids = resolve_tag(tag_name, &tags, &lookup, &mut cache, &mut visiting);
            if let Some(id) = Identifier::parse(tag_name) {
                resolved.push((id, ids));
            }
        }
        out.push((registry_ident, resolved));
    }
    Ok(out)
}

fn split_registry_and_tag<'a>(
    rel: &'a str,
    registries: &[DynamicRegistry],
    static_registries: &StaticRegistries,
) -> Option<(&'a str, &'a str)> {
    let mut splits: Vec<usize> = rel.match_indices('/').map(|(i, _)| i).collect();
    splits.reverse();
    for i in splits {
        let (reg, tag) = (&rel[..i], &rel[i + 1..]);
        if registries.iter().any(|r| r.id.path == reg) || static_registries.contains(reg) {
            return Some((reg, tag));
        }
    }
    None
}

fn raw_tag_value(v: &Value) -> Option<(String, bool)> {
    match v {
        Value::String(s) => Some((s.clone(), true)),
        Value::Object(o) => Some((
            o.get("id")?.as_str()?.to_owned(),
            o.get("required").and_then(|r| r.as_bool()).unwrap_or(true),
        )),
        _ => None,
    }
}

fn resolve_tag(
    tag_name: &str,
    tags: &BTreeMap<String, Vec<(String, bool)>>,
    lookup: &dyn Fn(&str) -> Option<i32>,
    cache: &mut HashMap<String, Vec<i32>>,
    visiting: &mut HashSet<String>,
) -> Vec<i32> {
    if let Some(ids) = cache.get(tag_name) {
        return ids.clone();
    }
    if !visiting.insert(tag_name.to_owned()) {
        return Vec::new();
    }
    let mut ids = Vec::new();
    if let Some(values) = tags.get(tag_name) {
        for (value, _) in values {
            if let Some(nested) = value.strip_prefix('#') {
                let nested = with_namespace(nested);
                ids.extend(resolve_tag(&nested, tags, lookup, cache, visiting));
            } else if let Some(id) = lookup(value) {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    visiting.remove(tag_name);
    cache.insert(tag_name.to_owned(), ids.clone());
    ids
}

fn with_namespace(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn read_dirs(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        anyhow::bail!("{} is not a directory", dir.display());
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().map(|e| e == "json").unwrap_or(false) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/versions/26.3");
        GameData::load(&dir).expect("prepared data")
    }

    #[test]
    fn loads_26_3_packet_ids() {
        let d = data();
        assert_eq!(d.protocol_version, 777);
        assert_eq!(d.data_version, 5023);
        assert_eq!(
            d.packet_ids
                .clientbound_id(arcade_protocol::State::Play, "login"),
            Some(50)
        );
    }

    #[test]
    fn static_and_dynamic_tags_are_sent() {
        let d = data();
        let has = |name: &str| d.tags.iter().any(|(r, _)| r.to_string() == name);
        assert!(has("minecraft:block"), "block tags missing");
        assert!(has("minecraft:item"), "item tags missing");
        assert!(has("minecraft:enchantment"), "enchantment tags missing");
        assert!(has("minecraft:worldgen/biome"), "biome tags missing");
    }

    #[test]
    fn dynamic_registries_are_present() {
        let d = data();
        assert!(d.registry_id("dimension_type", "overworld").is_some());
        assert!(d.registry_id("worldgen/biome", "plains").is_some());
        let biomes = d.registry("worldgen/biome").unwrap();
        assert!(biomes.entries.len() > 50);
    }

    #[test]
    fn map_item_and_component_ids() {
        let d = data();
        assert_eq!(d.item_id("filled_map"), Some(1238));
        assert_eq!(d.component_id("map_id"), Some(48));
    }

    #[test]
    fn block_states_resolve() {
        let b = Blocks;
        assert!(b.state("stone").is_some());
        assert!(b.state("grass_block").is_some());
        assert!(b.is_air(b.state_or_air("air")));
        assert!(b.bits_per_state() >= 4);
    }
}
