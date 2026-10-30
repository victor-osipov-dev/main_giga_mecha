# Version adapters

## Goal

One server state and one game runtime, multiple Minecraft protocol versions.
A version is not supported by changing a number: packet ids, registry data,
chunk layout, tags, item components and entity metadata differ.

## Interface

The live trait is in `crates/arcade-network/src/adapter.rs`:

```rust
pub trait ProtocolAdapter: Send + Sync {
    fn protocol_version(&self) -> i32;

    fn decode_input(
        &self,
        packet: &[u8],
    ) -> Result<Option<GameInput>, ProtocolError>;
}
```

`V26_3Adapter` maps `use_item`, `use_item_on` and `punch` in the play state to
`GameInput::Jump`. The play handler asks the server's adapter before matching
packets, so adding a version does not add version checks to the game logic.

The server stores `Arc<dyn ProtocolAdapter>` and logs the active adapter's
protocol version.

## What a new version needs

1. **Packet table.** Run `scripts/prepare-data.ps1 -Version <version>` and put
   the generated `packets.json` in `crates/arcade-protocol-data/data/`.
   `PacketIds` loads it by name, not number.
2. **Dynamic registries and tags.** Prepare `<version>/datapack`. Registry
   entry order is sorted by identifier; keep that rule so indices are stable.
3. **Static ids.** Add the matching `minecraft_data` version for blocks,
   items, entity types and data components, or load `registries.json`
   generically.
4. **Packet differences.** Diff the version's packet fields against 26.3.
   Keep differences inside the version's packet modules, not in handlers.
5. **Chunk/light format.** World encoding is version-specific. The current
   `ChunkData` type is 26.3-shaped; a new version gets its own encoder.
6. **Entity/player details.** Player info actions, add-entity fields and
   movement delta formats must be checked per version.
7. **Input adapter.** Implement `ProtocolAdapter` for the version and return
   only actions the wire format can actually distinguish.
8. **Tests.** Run Login → Configuration → Play → Movement → Multiplayer →
   Disconnect with both the protocol client and the official client.

## Planned order

| Version | Protocol | State |
|---|---:|---|
| 26.3 | 777 | Working |
| 26.2 | 776 | Next; data/table not prepared yet |
| 26.1.x | 775 | Planned |
| 1.21.11 | 774 | Planned |
| 1.21.8 | 772 | Planned |

## Non-goals

- No packet id translation by subtraction/addition.
- No fake protocol number in status/login.
- No shared "one true chunk format" across versions.
- A version is not listed as supported until the full flow is tested.
