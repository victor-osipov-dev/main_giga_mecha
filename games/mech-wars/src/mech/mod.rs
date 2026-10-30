//! Pure voxel mech simulation. No Minecraft packets in this module.

pub mod components;
pub mod definition;
pub mod input;
pub mod instance;
pub mod orders;
pub mod voxel;

pub use components::{ComponentId, MechComponent};
pub use definition::{build_test_mech, MechDefinition};
pub use input::{MechCommand, MVP_COMMANDS};
pub use instance::MechInstance;
pub use orders::{MechOrders, MoveOrder, RotationOrder};
pub use voxel::{grouped_boxes, shell_voxels, Material, MechVoxel, RenderBox, VoxelGrid};
