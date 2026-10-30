//! Mech Wars: voxel battle mechs rendered with vanilla Minecraft displays.
//!
//! - `mech` is a pure simulation (voxels, parts, health).
//! - `rendering` converts a mech into block-display packets.
//! - `game` owns per-player mechs and enforces arena limits.

pub mod control;
pub mod game;
pub mod mech;
pub mod rendering;

pub use game::{MechError, MechStatus, MechWars, SpawnResult};
pub use mech::{
    ComponentId, Material, MechCommand, MechInstance, MechOrders, MechVoxel, MoveOrder,
    RotationOrder,
};
