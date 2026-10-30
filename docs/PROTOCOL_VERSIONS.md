# Protocol versions

## Supported

| Minecraft | Protocol | Data version | Status |
|---|---:|---:|---|
| Java 26.3 | 777 | 5023 | Implemented and covered by protocol-level integration tests |

Only 26.3 is supported today. The server rejects any other protocol at login
with a clear message. It never pretends to support a version by substituting
the protocol number: packet ids, registry payloads, chunk layout and entity
metadata differ between releases.

## Where the numbers come from

- `version.json` in the official 26.3 server jar: `protocol_version: 777`,
  `world_version: 5023`.
- `generated/reports/packets.json` from Mojang's data generator: all packet
  names and numeric ids for 26.3.
- `datapack/minecraft/...` from the same jar: dynamic registry payloads and
  tags.

The preparation pipeline is `scripts/prepare-data.ps1`; its inputs and
checksum are recorded in `data/versions/26.3/SOURCE.txt`.

## State flow

```
Handshake (protocol, address, port, intent)
  ├─ intent 1 → Status → status_request / ping_request
  ├─ intent 2 → Login
  └─ intent 3 → Login (transfer)

Login
  → login hello (name, claimed uuid)
  → [online] encryption request/response, session check
  → set compression
  → login finished (uuid, name, properties, session uuid)
  → login acknowledged

Configuration
  → server: brand, feature flags, select known packs (empty)
  → client: client information, select known packs
  → server: registry data ×32, update tags, finish configuration
  → client: finish configuration

Play
  → login (join game), difficulty, abilities, teleport
  → player info, time, spawn position, render/simulation distance
  → chunk batch (~49 chunks at view distance 3)
  → player_loaded
  → entity visibility, movement replication, keep-alives
```

## Packet id table

`arcade-protocol-data` embeds the official `packets.json`. The server asks for
packets by vanilla name (`login`, `registry_data`, `move_player_pos_rot`, ...)
and the table resolves the id. There are 164 clientbound and 163 serverbound
26.3 names in the report; only the subset used by this core is encoded or
decoded, while unknown serverbound packets are ignored safely.

## Future versions

Planned order from the plan:

1. 26.2 — protocol 776
2. 26.1.x — protocol 775
3. 1.21.11 — protocol 774
4. 1.21.8 — protocol 772

Each version will need its own `packets.json`, dynamic registry snapshot and
a `ProtocolAdapter` implementation for differences in chunk/registry/entity
formats. Adding them must not change the 26.3 path. A version counts as
supported only after Login → Configuration → Play → Gameplay → Disconnect is
tested; see `COMPATIBILITY.md`.

## Legacy status ping

Legacy `0xFE` pings are ignored rather than parsed. Modern clients use the
handshake + status flow described above.
