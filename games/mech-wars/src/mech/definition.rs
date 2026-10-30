//! The built-in mech definitions.

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

/// A 34-block-tall heavy mech: detailed legs with feet and knee plates, a
/// layered torso with a chest reactor, shoulder pads, backpack boosters,
/// articulated arms and a head with an antenna. Coordinates are local voxels:
/// +Y up, +Z forward.
pub fn build_test_mech() -> MechDefinition {
    let mut grid = VoxelGrid::new([GRID_X, GRID_Y, GRID_Z]);
    let c = |id: ComponentId| id as u16;
    let hp = 100u16;

    // --- Feet with toes ---
    grid.fill(
        1..=5,
        0..=1,
        2..=7,
        Material::Steel,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        10..=14,
        0..=1,
        2..=7,
        Material::Steel,
        c(ComponentId::RightLeg),
        hp,
    );
    grid.fill(
        3..=4,
        0..=1,
        7..=8,
        Material::Steel,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=12,
        0..=1,
        7..=8,
        Material::Steel,
        c(ComponentId::RightLeg),
        hp,
    );

    // --- Ankles, shins, knees, thighs ---
    grid.fill(
        2..=4,
        2..=3,
        3..=6,
        Material::Joint,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=13,
        2..=3,
        3..=6,
        Material::Joint,
        c(ComponentId::RightLeg),
        hp,
    );
    grid.fill(
        2..=4,
        4..=11,
        3..=6,
        Material::Armour,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=13,
        4..=11,
        3..=6,
        Material::Armour,
        c(ComponentId::RightLeg),
        hp,
    );
    grid.fill(
        2..=4,
        4..=9,
        6..=7,
        Material::Panel,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=13,
        4..=9,
        6..=7,
        Material::Panel,
        c(ComponentId::RightLeg),
        hp,
    );
    grid.fill(
        2..=4,
        12..=13,
        3..=6,
        Material::Joint,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=13,
        12..=13,
        3..=6,
        Material::Joint,
        c(ComponentId::RightLeg),
        hp,
    );
    grid.fill(
        2..=4,
        14..=17,
        3..=6,
        Material::Armour,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=13,
        14..=17,
        3..=6,
        Material::Armour,
        c(ComponentId::RightLeg),
        hp,
    );
    grid.fill(
        2..=4,
        14..=16,
        6..=7,
        Material::Panel,
        c(ComponentId::LeftLeg),
        hp,
    );
    grid.fill(
        11..=13,
        14..=16,
        6..=7,
        Material::Panel,
        c(ComponentId::RightLeg),
        hp,
    );

    // --- Hip / pelvis ---
    grid.fill(
        5..=10,
        17..=18,
        3..=6,
        Material::Joint,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        5..=10,
        17..=18,
        6..=7,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );

    // --- Torso: belt, chest plate, reactor, intakes, back vents ---
    grid.fill(
        4..=11,
        19..=26,
        2..=7,
        Material::Armour,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        5..=10,
        19..=19,
        2..=7,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        5..=10,
        21..=25,
        6..=7,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        6..=7,
        22..=24,
        7..=8,
        Material::Reactor,
        c(ComponentId::Reactor),
        hp,
    );
    grid.fill(
        6..=7,
        24..=25,
        7..=8,
        Material::Glass,
        c(ComponentId::Reactor),
        hp,
    );
    grid.fill(
        4..=4,
        21..=24,
        3..=6,
        Material::Dark,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        11..=11,
        21..=24,
        3..=6,
        Material::Dark,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        5..=10,
        21..=24,
        2..=2,
        Material::Dark,
        c(ComponentId::Torso),
        hp,
    );

    // --- Backpack with boosters ---
    grid.fill(
        5..=10,
        21..=24,
        1..=2,
        Material::Dark,
        c(ComponentId::Backpack),
        hp,
    );
    grid.fill(
        5..=6,
        23..=24,
        1..=2,
        Material::Steel,
        c(ComponentId::Backpack),
        hp,
    );
    grid.fill(
        9..=10,
        23..=24,
        1..=2,
        Material::Steel,
        c(ComponentId::Backpack),
        hp,
    );
    grid.fill(
        5..=6,
        24..=24,
        1..=2,
        Material::Glow,
        c(ComponentId::Backpack),
        hp,
    );
    grid.fill(
        9..=10,
        24..=24,
        1..=2,
        Material::Glow,
        c(ComponentId::Backpack),
        hp,
    );

    // --- Neck ---
    grid.fill(
        5..=10,
        28..=28,
        3..=6,
        Material::Steel,
        c(ComponentId::Head),
        hp,
    );

    // --- Shoulder pads ---
    grid.fill(
        2..=3,
        27..=29,
        3..=6,
        Material::Joint,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        12..=13,
        27..=29,
        3..=6,
        Material::Joint,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        1..=3,
        29..=30,
        2..=7,
        Material::Armour,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        12..=14,
        29..=30,
        2..=7,
        Material::Armour,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        1..=3,
        30..=31,
        2..=7,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        12..=14,
        30..=31,
        2..=7,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        1..=1,
        28..=30,
        3..=6,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );
    grid.fill(
        14..=14,
        28..=30,
        3..=6,
        Material::Panel,
        c(ComponentId::Torso),
        hp,
    );

    // --- Arms with elbows and fists ---
    grid.fill(
        1..=2,
        24..=27,
        3..=6,
        Material::Armour,
        c(ComponentId::LeftArm),
        hp,
    );
    grid.fill(
        13..=14,
        24..=27,
        3..=6,
        Material::Armour,
        c(ComponentId::RightArm),
        hp,
    );
    grid.fill(
        1..=2,
        23..=23,
        3..=6,
        Material::Joint,
        c(ComponentId::LeftArm),
        hp,
    );
    grid.fill(
        13..=14,
        23..=23,
        3..=6,
        Material::Joint,
        c(ComponentId::RightArm),
        hp,
    );
    grid.fill(
        1..=2,
        20..=22,
        3..=6,
        Material::Armour,
        c(ComponentId::LeftArm),
        hp,
    );
    grid.fill(
        13..=14,
        20..=22,
        3..=6,
        Material::Armour,
        c(ComponentId::RightArm),
        hp,
    );
    grid.fill(
        1..=2,
        19..=19,
        3..=6,
        Material::Steel,
        c(ComponentId::LeftArm),
        hp,
    );
    grid.fill(
        13..=14,
        19..=19,
        3..=6,
        Material::Steel,
        c(ComponentId::RightArm),
        hp,
    );

    // --- Main cannon on the right forearm ---
    grid.fill(
        13..=14,
        21..=22,
        7..=8,
        Material::Weapon,
        c(ComponentId::MainWeapon),
        hp,
    );
    grid.fill(
        13..=14,
        21..=22,
        8..=9,
        Material::Glass,
        c(ComponentId::MainWeapon),
        hp,
    );

    // --- Head with visor and antenna ---
    grid.fill(
        5..=10,
        30..=32,
        3..=6,
        Material::Armour,
        c(ComponentId::Head),
        hp,
    );
    grid.fill(
        6..=7,
        31..=31,
        6..=6,
        Material::Eye,
        c(ComponentId::Head),
        hp,
    );
    grid.fill(
        5..=5,
        31..=31,
        6..=6,
        Material::Dark,
        c(ComponentId::Head),
        hp,
    );
    grid.fill(
        10..=10,
        31..=31,
        6..=6,
        Material::Dark,
        c(ComponentId::Head),
        hp,
    );
    grid.fill(
        5..=10,
        32..=32,
        3..=6,
        Material::Panel,
        c(ComponentId::Head),
        hp,
    );
    grid.fill(
        7..=8,
        33..=33,
        4..=5,
        Material::Steel,
        c(ComponentId::Antenna),
        hp,
    );
    grid.fill(
        7..=8,
        33..=33,
        4..=5,
        Material::Glow,
        c(ComponentId::Antenna),
        hp,
    );

    let components = vec![
        MechComponent::new(ComponentId::Root, [0.0, 0.0, 0.0], 100),
        MechComponent::new(ComponentId::LeftLeg, [3.0, 18.0, 4.0], 100),
        MechComponent::new(ComponentId::RightLeg, [12.0, 18.0, 4.0], 100),
        MechComponent::new(ComponentId::Torso, [8.0, 19.0, 4.0], 100),
        MechComponent::new(ComponentId::Head, [8.0, 30.0, 4.0], 100),
        MechComponent::new(ComponentId::LeftArm, [1.0, 27.0, 4.0], 100),
        MechComponent::new(ComponentId::RightArm, [14.0, 27.0, 4.0], 100),
        MechComponent::new(ComponentId::Reactor, [7.0, 23.0, 7.0], 100),
        MechComponent::new(ComponentId::MainWeapon, [14.0, 22.0, 8.0], 100),
        MechComponent::new(ComponentId::Backpack, [8.0, 22.0, 1.0], 100),
        MechComponent::new(ComponentId::Antenna, [8.0, 33.0, 4.0], 100),
    ];

    MechDefinition {
        name: "Atlas MK-II",
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
        assert_eq!(mech.dims, [16, 34, 10]);
        assert_eq!(mech.name, "Atlas MK-II");
        assert!(mech.grid.alive_count() > 1000);
        for id in ComponentId::ALL {
            assert!(mech.component(id).is_some(), "missing {}", id.name());
        }
        assert!(mech.component(ComponentId::MainWeapon).unwrap().operational);
    }
}
