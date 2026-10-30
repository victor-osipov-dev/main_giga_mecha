# Mech Wars — MVP status

Mech Wars is a module of Voxel Arcade RS, not a separate server. It uses the
core transport, player manager, runtime primitives and the same vanilla 26.3
client.

## Implemented milestones

### M1 — Voxel mech

- `MechVoxel`: material, health, component id, flags; independent of
  Minecraft block states.
- `VoxelGrid` 12×24×8 (width × height × depth).
- Programmatic `Sentinel-01` model: two legs, torso, head, two arms, reactor
  and a right-arm cannon.
- Component hierarchy: root, legs, torso, head, arms, reactor, main weapon.
- Materials map to vanilla blocks: iron block, gray concrete, copper block,
  sea lantern, blackstone, tinted glass, redstone lamp.
- Only the outer shell is rendered. The current shell is **598 voxels**.
  The 500-display experimental limit was raised to 700 for this model; two
  mechs still fit the intended two-player test.
- Interior voxels are not displayed yet. Damaged boundaries will need backing
  voxels so the hollow interior is not visible; that belongs to the damage
  milestone.

### M2 — Visible mech

- One vanilla `block_display` entity per shell voxel.
- Each display is exactly **1×1×1** and remains axis-aligned to the world
  block grid, like a normal block.
- 26.3 metadata index used for the block: `23` (`BlockState`, type `14`).
- Movement and yaw are quantized onto whole-block cells: displays are
  re-rasterized to integer cell coordinates, not smoothly rotated.
- No resource pack or mods.
- Spawn payloads are cached; new players receive a replay of all active
  mechs and panels after `player_loaded`.
- Disconnecting the owner removes the mech, panel and display entities.

### M3 — Control panel

`/mech spawn` also deploys a personal panel in front of the player:

- 18 copper button displays in a 6×3 grid on a blackstone backboard.
- Invisible `interaction` entities with `responsive = true` are the click
  targets.
- Short `text_display` labels above each button.
- The panel is rotated to the player's yaw.
- Only the owner's `interact` packets map to commands; other players are
  rejected.
- The panel and its entities are removed with `/mech despawn`.
- A text command fallback exists: `/mech forward`, `/mech torso left`, etc.
- The panel uses scaled display boxes (not 1×1×1); the 1×1×1 requirement
  applies to the mech body.

### M4 — Orders and movement

- Persistent orders: move forward/back/stop, body turn left/right/stop,
  torso turn left/right/stop/reset, auto-fire flag.
- Movement is quantized to one-block steps. The accumulator preserves the
  configured speed (1.5 blocks/s forward, 0.75 reverse); diagonal 45° steps
  cost their Euclidean length.
- Body rotation 5°/tick and torso rotation 10°/tick internally, but visual
  orientation is quantized to **45°** and voxel centres are rasterized onto
  integer cells. Torso limit is ±45°.
- `EMERGENCY_STOP` clears movement, turns, torso reset and auto-fire.
- 20 TPS mech loop. Moved displays get `entity_position_sync`; no scale or
  rotation metadata is used for the mech body anymore.

## Commands

```text
/mech spawn
/mech despawn
/mech status
/mech reset
/mech help

/mech forward | back | stop
/mech turn left | right | stop
/mech torso left | right | stop | reset
/mech speed slow | normal
/mech fire | auto on | off | estop | clearorders
```

`spawn` places the mech 12 blocks in front of the player and the panel 4
blocks ahead. Only one mech per owner and two mechs per arena are allowed.

## Units

- Movement: blocks per second. Forward 1.5, reverse 0.75, acceleration
  0.5 blocks/s².
- Rotation: degrees per server tick at 20 TPS internally. Body 5°/tick,
  torso 10°/tick; rendering quantizes to 45°.
- Torso limit: ±45° relative to the body.
- Display size: exactly 1×1×1 block; positions are integer cells.

## Tests

- Voxel grouping covers every live voxel exactly once.
- Shell extraction (598 cells for the test mech).
- Model dimensions and component hierarchy.
- Spawn builds one display and one metadata packet per shell voxel.
- Forward acceleration, one-block movement, full stop, torso clamp/reset,
  emergency stop.
- Panel has one interaction entity and one command per button.
- Grid movement produces display position updates.
- Protocol integration: one mech with panel; two mechs (and two panels)
  replicated to both players.

`cargo test -p mech-wars` plus the `login_multiplayer` integration tests
cover these paths.

## Not done yet

- Official-client visual confirmation of the 1×1×1 block model, panel,
  clicks and 45° rasterization.
- Damage, backing voxels at newly exposed interior boundaries, and component
  failure.
- Weapons, projectiles and segment-vs-voxel collision.
- Full two-mech battle with real clients.
- Mech Wars-specific memory/CPU measurements.
- Config file section for mech tuning; current values are constants in
  `mech/instance.rs` and `rendering/block_display.rs`.

## Next milestone

M5: projectiles, auto-fire and segment-vs-voxel collision. Damage must remove
only affected 1×1×1 displays and reveal/fill backing voxels at the cavity
boundary.
