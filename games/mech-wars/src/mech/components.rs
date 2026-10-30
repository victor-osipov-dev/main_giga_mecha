//! Component hierarchy and functional state.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ComponentId {
    Root = 0,
    LeftLeg = 1,
    RightLeg = 2,
    Torso = 3,
    Head = 4,
    LeftArm = 5,
    RightArm = 6,
    Reactor = 7,
    MainWeapon = 8,
}

impl ComponentId {
    pub const ALL: [ComponentId; 9] = [
        Self::Root,
        Self::LeftLeg,
        Self::RightLeg,
        Self::Torso,
        Self::Head,
        Self::LeftArm,
        Self::RightArm,
        Self::Reactor,
        Self::MainWeapon,
    ];

    pub fn from_id(id: u16) -> Self {
        Self::ALL.get(id as usize).copied().unwrap_or(Self::Root)
    }

    pub fn parent(self) -> Option<Self> {
        match self {
            Self::Root => None,
            Self::LeftLeg | Self::RightLeg | Self::Torso => Some(Self::Root),
            Self::Head | Self::LeftArm | Self::RightArm | Self::Reactor | Self::MainWeapon => {
                Some(Self::Torso)
            }
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::LeftLeg => "left_leg",
            Self::RightLeg => "right_leg",
            Self::Torso => "torso",
            Self::Head => "head",
            Self::LeftArm => "left_arm",
            Self::RightArm => "right_arm",
            Self::Reactor => "reactor",
            Self::MainWeapon => "main_weapon",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MechComponent {
    pub id: ComponentId,
    /// Local pivot relative to the parent component.
    pub pivot: [f32; 3],
    pub max_health: u16,
    pub health: u16,
    pub operational: bool,
}

impl MechComponent {
    pub fn new(id: ComponentId, pivot: [f32; 3], max_health: u16) -> Self {
        Self {
            id,
            pivot,
            max_health,
            health: max_health,
            operational: true,
        }
    }
}
