# Compatibility matrix

Status legend:

- **Tested** — exercised by the automated protocol client against a real TCP
  server (and, where stated, manually).
- **Implemented** — code exists and is exercised indirectly, but the exact
  scenario has not been verified end to end.
- **Not implemented** — no code path.
- **Unsupported** — the server rejects the version or scenario.

| Version | Status | Login | Online auth | Configuration | World loading | Movement | Multiplayer | Game input | Rendering | Overall |
|---|---|---|---|---|---|---|---:|---|---|
| 26.3 (777) | Working | Tested (official client + protocol client) | Tested (official client, online_mode=true) | Tested (official client) | Tested (official client + 49 chunks) | Tested (protocol client) | Tested with protocol clients (2 and 20); real two-client check pending | Tested (official client: `/flappy`) | Tested (official client, filled map) | **Working on 26.3** |
| 26.2 (776) | Not implemented | — | — | — | — | — | — | — | — | Unsupported |
| 26.1.x (775) | Not implemented | — | — | — | — | — | — | — | — | Unsupported |
| 1.21.11 (774) | Not implemented | — | — | — | — | — | — | — | — | Unsupported |
| 1.21.8 (772) | Not implemented | — | — | — | — | — | — | — | — | Unsupported |

## Official client checks (2026-10-29)

A real Minecraft Java 26.3 client connected to the server with
`online_mode = true` and reached the play state. It loaded the flat world and
displayed the `/flappy` map game after the map-packet and inventory-slot
fixes. The registry-tag failure found during that session was fixed by sending
static block/item/entity tags as well as dynamic tags.

Still to do with real clients: two clients seeing each other, movement under
real latency, reconnect, and the 10/20-player scenarios.

## What "protocol-tested" means

`tools/test-client` is an independent implementation that speaks the 26.3
wire protocol. It performs:

1. Handshake intent 1, status request, and a ping/pong round trip.
2. Handshake intent 2, login hello, compression negotiation, login success
   and login acknowledgement.
3. Configuration: client information, empty known packs, receipt of all 32
   registry packets, finish configuration acknowledgement.
4. Play: join game, teleport acknowledgement, chunk batch, player list,
   `player_loaded`, keep-alives, movement, entity spawn/despawn.
5. Map game: `/flappy`, map item and map frame updates.

`crates/arcade-network/tests/status_ping.rs` and
`crates/arcade-network/tests/login_multiplayer.rs` automate these flows.

## What has NOT been verified

- **Two official clients in the same world.** Multiplayer is covered by two
  and twenty protocol-level clients; a manual two-GUI-client check is pending.
- **Movement on the official client under adversarial conditions.** Basic
  movement is implemented and protocol-tested; speed/fly validation is not.
- **Linux release run.** The memory report is a Windows release measurement.
- **Second protocol version.** 26.2 and older are not implemented yet.

## Version policy

A version is added only with its own packet table, registry snapshot and
adapter; protocol numbers are never substituted. The target order remains
26.2 → 26.1.x → 1.21.11 → 1.21.8 as stated in the plan.
