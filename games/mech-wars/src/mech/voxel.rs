//! Voxel storage that is independent of Minecraft block states.

pub const GRID_X: usize = 12;
pub const GRID_Y: usize = 24;
pub const GRID_Z: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MechVoxel {
    pub material_id: u16,
    pub health: u16,
    pub component_id: u16,
    pub flags: u8,
}

impl MechVoxel {
    pub const fn new(material: Material, component: u16, health: u16) -> Self {
        Self {
            material_id: material as u16,
            health,
            component_id: component,
            flags: 0,
        }
    }

    pub fn is_alive(self) -> bool {
        self.health > 0
    }

    pub fn material(self) -> Material {
        Material::from_id(self.material_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Material {
    Steel = 0,
    Armour = 1,
    Joint = 2,
    Reactor = 3,
    Weapon = 4,
    Glass = 5,
    Eye = 6,
}

impl Material {
    pub const ALL: [Material; 7] = [
        Material::Steel,
        Material::Armour,
        Material::Joint,
        Material::Reactor,
        Material::Weapon,
        Material::Glass,
        Material::Eye,
    ];

    pub fn from_id(id: u16) -> Self {
        Self::ALL
            .get(id as usize)
            .copied()
            .unwrap_or(Material::Steel)
    }

    /// The vanilla block used by the block-display renderer.
    pub fn block_name(self) -> &'static str {
        match self {
            Material::Steel => "iron_block",
            Material::Armour => "gray_concrete",
            Material::Joint => "copper_block",
            Material::Reactor => "sea_lantern",
            Material::Weapon => "blackstone",
            Material::Glass => "tinted_glass",
            Material::Eye => "redstone_lamp",
        }
    }
}

#[derive(Clone, Debug)]
pub struct VoxelGrid {
    pub dims: [usize; 3],
    voxels: Vec<MechVoxel>,
}

impl VoxelGrid {
    pub fn new(dims: [usize; 3]) -> Self {
        Self {
            dims,
            voxels: vec![MechVoxel::new(Material::Steel, 0, 0); dims[0] * dims[1] * dims[2]],
        }
    }

    pub fn index(&self, x: usize, y: usize, z: usize) -> Option<usize> {
        if x >= self.dims[0] || y >= self.dims[1] || z >= self.dims[2] {
            return None;
        }
        Some((y * self.dims[2] + z) * self.dims[0] + x)
    }

    pub fn get(&self, x: usize, y: usize, z: usize) -> Option<MechVoxel> {
        self.index(x, y, z).map(|i| self.voxels[i])
    }

    pub fn set(&mut self, x: usize, y: usize, z: usize, voxel: MechVoxel) {
        if let Some(i) = self.index(x, y, z) {
            self.voxels[i] = voxel;
        }
    }

    pub fn voxels(&self) -> &[MechVoxel] {
        &self.voxels
    }

    pub fn voxels_mut(&mut self) -> &mut [MechVoxel] {
        &mut self.voxels
    }

    pub fn alive_count(&self) -> usize {
        self.voxels.iter().filter(|v| v.is_alive()).count()
    }

    pub fn fill(
        &mut self,
        x: std::ops::RangeInclusive<usize>,
        y: std::ops::RangeInclusive<usize>,
        z: std::ops::RangeInclusive<usize>,
        material: Material,
        component: u16,
        health: u16,
    ) {
        for yy in y.clone() {
            for zz in z.clone() {
                for xx in x.clone() {
                    self.set(xx, yy, zz, MechVoxel::new(material, component, health));
                }
            }
        }
    }
}

/// Voxels that have at least one exposed face (outer shell). Damage can add
/// backing voxels at newly exposed boundaries later.
pub fn shell_voxels(grid: &VoxelGrid) -> Vec<(usize, usize, usize, MechVoxel)> {
    let [sx, sy, sz] = grid.dims;
    let mut out = Vec::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let Some(v) = grid.get(x, y, z) else { continue };
                if !v.is_alive() {
                    continue;
                }
                let mut is_shell = false;
                for (dx, dy, dz) in [
                    (-1i32, 0, 0),
                    (1, 0, 0),
                    (0, -1, 0),
                    (0, 1, 0),
                    (0, 0, -1),
                    (0, 0, 1),
                ] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;
                    let inside = nx >= 0
                        && ny >= 0
                        && nz >= 0
                        && (nx as usize) < sx
                        && (ny as usize) < sy
                        && (nz as usize) < sz;
                    if !inside
                        || grid
                            .get(nx as usize, ny as usize, nz as usize)
                            .map(|n| !n.is_alive())
                            .unwrap_or(true)
                    {
                        is_shell = true;
                        break;
                    }
                }
                if is_shell {
                    out.push((x, y, z, v));
                }
            }
        }
    }
    out
}

/// A renderable rectangular group generated from live voxels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderBox {
    pub component_id: u16,
    pub material: Material,
    /// Minimum local voxel coordinate.
    pub min: [usize; 3],
    /// Size in voxels.
    pub size: [usize; 3],
}

impl RenderBox {
    pub fn volume(&self) -> usize {
        self.size[0] * self.size[1] * self.size[2]
    }
}

/// Greedy axis-aligned grouping. Boxes are capped so future damage can
/// rebuild a small region instead of making a whole limb disappear.
pub fn grouped_boxes(grid: &VoxelGrid, max_size: [usize; 3]) -> Vec<RenderBox> {
    let [sx, sy, sz] = grid.dims;
    let mut used = vec![false; sx * sy * sz];
    let mut boxes = Vec::new();
    for y in 0..sy {
        for z in 0..sz {
            for x in 0..sx {
                let i = (y * sz + z) * sx + x;
                if used[i] {
                    continue;
                }
                let Some(voxel) = grid.get(x, y, z) else {
                    continue;
                };
                if !voxel.is_alive() {
                    used[i] = true;
                    continue;
                }
                let material = voxel.material();
                let component = voxel.component_id;
                let mut size = [1usize, 1, 1];
                while size[0] < max_size[0]
                    && x + size[0] < sx
                    && grid
                        .get(x + size[0], y, z)
                        .map(|v| {
                            v.is_alive() && v.component_id == component && v.material() == material
                        })
                        .unwrap_or(false)
                    && !used[(y * sz + z) * sx + x + size[0]]
                {
                    size[0] += 1;
                }
                'layers: while size[2] < max_size[2] && z + size[2] < sz {
                    for xx in x..x + size[0] {
                        let j = (y * sz + z + size[2]) * sx + xx;
                        let Some(v) = grid.get(xx, y, z + size[2]) else {
                            break 'layers;
                        };
                        if used[j]
                            || !v.is_alive()
                            || v.component_id != component
                            || v.material() != material
                        {
                            break 'layers;
                        }
                    }
                    size[2] += 1;
                }
                'tall: while size[1] < max_size[1] && y + size[1] < sy {
                    for zz in z..z + size[2] {
                        for xx in x..x + size[0] {
                            let j = ((y + size[1]) * sz + zz) * sx + xx;
                            let Some(v) = grid.get(xx, y + size[1], zz) else {
                                break 'tall;
                            };
                            if used[j]
                                || !v.is_alive()
                                || v.component_id != component
                                || v.material() != material
                            {
                                break 'tall;
                            }
                        }
                    }
                    size[1] += 1;
                }
                for yy in y..y + size[1] {
                    for zz in z..z + size[2] {
                        for xx in x..x + size[0] {
                            used[(yy * sz + zz) * sx + xx] = true;
                        }
                    }
                }
                boxes.push(RenderBox {
                    component_id: component,
                    material,
                    min: [x, y, z],
                    size,
                });
            }
        }
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_size_is_measured() {
        let mech = crate::mech::build_test_mech();
        let shell = shell_voxels(&mech.grid);
        println!("SHELL_COUNT={}", shell.len());
        assert!(!shell.is_empty());
    }

    #[test]
    fn grouping_covers_every_live_voxel_exactly_once() {
        let mut grid = VoxelGrid::new([8, 8, 8]);
        grid.fill(1..=4, 1..=4, 1..=4, Material::Armour, 2, 100);
        grid.fill(5..=6, 1..=2, 1..=2, Material::Weapon, 3, 100);
        let boxes = grouped_boxes(&grid, [4, 4, 4]);
        let volume: usize = boxes.iter().map(RenderBox::volume).sum();
        assert_eq!(volume, grid.alive_count());
        assert!(!boxes.is_empty());
    }
}
