//! The first built-in mech definition.

use super::components::{ComponentId, MechComponent};
use super::voxel::{Material, VoxelGrid, GRID_X, GRID_Y, GRID_Z};

#[derive(Clone, Debug)]
pub struct MechDefinition {
    pub name: &'static str,
    pub dims: [usize; 3],
    pub grid: VoxelGrid,
    pub components: Vec<MechComponent>,
}

impl MechDefinition {
    pub fn component(&self, id: ComponentId) -> Option<&MechComponent> {
        self.components.iter().find(|c| c.id == id)
    }

    pub fn component_mut(&mut self, id: ComponentId) -> Option<&mut MechComponent> {
        self.components.iter_mut().find(|c| c.id == id)
    }
}

/// A 24-block-tall test mech with two legs, torso, head, arms, reactor and a
/// right-arm cannon. Coordinates are local voxels: +Y up, +Z forward.
pub fn build_test_mech() -> MechDefinition {
    let mut grid = VoxelGrid::new([GRID_X, GRID_Y, GRID_Z]);
    let c = |id: ComponentId| id as u16;
    let hp = 100u16;

    // Legs.
    grid.fill(
        1..=4,
        0..=9,
        2..=5,
        Material::Steel,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        7..=10,
        0..=9,
        2..=5,
        Material::Steel,
        c(ComponentId::RightLeg),
        hp,
    );
    // Hip block belongs to the torso.
    grid.fill(
        4..=7,
        9..=10,
        2..=5,
        Material::Joint,
        c(ComponentId::Torso),
        hp,
    );
    // Torso.
    grid.fill(
        2..=9,
        11..=19,
        1..=6,
        Material::Armour,
        c(ComponentId::Torso),
        hp,
    );
    // Shoulder joints.
    grid.fill(
        2..=3,
        17..=19,
        2..=5,
        Material::Joint,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        8..=9,
        17..=19,
        2..=5,
        Material::Joint,
        c(ComponentId::Torso),
        hp,
    );
    // Head with eyes on the front face.
    grid.fill(
        4..=7,
        20..=22,
        2..=5,
        Material::Armour,
        c(ComponentId::Head),
        hp,
    );
    grid.fill(
        4..=7,
        21..=21,
        5..=5,
        Material::Eye,
        c(ComponentId::Head),
        hp,
    );
    // Arms.
    grid.fill(
        0..=1,
        12..=19,
        2..=5,
        Material::Armour,
        c(ComponentId::LeftArm),
        hp,
    );
    grid.fill(
        10..=11,
        12..=19,
        2..=5,
        Material::Armour,
        c(ComponentId::RightArm),
        hp,
    );
    // Reactor core in the chest.
    grid.fill(
        5..=6,
        14..=16,
        6..=7,
        Material::Reactor,
        c(ComponentId::Reactor),
        hp,
    );
    // Main cannon on the right arm, pointing forward.
    grid.fill(
        10..=11,
        15..=16,
        6..=7,
        Material::Weapon,
        c(ComponentId::MainWeapon),
        hp,
    );
    // Barrel tip.
    grid.fill(
        10..=11,
        15..=15,
        7..=7,
        Material::Glass,
        c(ComponentId::MainWeapon),
        hp,
    );

    let components = vec![
        MechComponent::new(ComponentId::Root, [0.0, 0.0, 0.0], 100),
        MechComponent::new(ComponentId::LeftLeg, [3.0, 10.0, 4.0], 100),
        MechComponent::new(ComponentId::RightLeg, [9.0, 10.0, 4.0], 100),
        MechComponent::new(ComponentId::Torso, [6.0, 11.0, 4.0], 100),
        MechComponent::new(ComponentId::Head, [6.0, 20.0, 4.0], 100),
        MechComponent::new(ComponentId::LeftArm, [1.0, 19.0, 4.0], 100),
        MechComponent::new(ComponentId::RightArm, [11.0, 19.0, 4.0], 100),
        MechComponent::new(ComponentId::Reactor, [6.0, 15.0, 6.0], 100),
        MechComponent::new(ComponentId::MainWeapon, [11.0, 16.0, 6.0], 100),
    ];

    MechDefinition {
        name: "Sentinel-01",
        dims: [GRID_X, GRID_Y, GRID_Z],
        grid,
        components,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mech_has_expected_scale_and_parts() {
        let mech = build_test_mech();
        assert_eq!(mech.dims, [12, 24, 8]);
        assert!(mech.grid.alive_count() > 900);
        for id in ComponentId::ALL {
            assert!(mech.component(id).is_some(), "missing {}", id.name());
        }
        assert!(mech.component(ComponentId::MainWeapon).unwrap().operational);
    }
}
