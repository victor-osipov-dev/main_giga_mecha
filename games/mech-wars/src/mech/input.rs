//! High-level commands issued by a control panel.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MechCommand {
    MoveForward,
    MoveBackward,
    MoveStop,
    TurnLeft,
    TurnRight,
    TurnStop,
    SpeedSlow,
    SpeedNormal,
    TorsoLeft,
    TorsoRight,
    TorsoStop,
    TorsoReset,
    FireOnce,
    AutoFireOn,
    AutoFireOff,
    EmergencyStop,
    SystemStatus,
    ResetCommands,
}

impl MechCommand {
    /// Text interface used until the physical control panel is wired up.
    pub fn from_text(parts: &[&str]) -> Option<Self> {
        Some(match (parts.first().copied()?, parts.get(1).copied()) {
            ("forward", _) | ("move", Some("forward")) => Self::MoveForward,
            ("back", _) | ("backward", _) | ("move", Some("back")) => Self::MoveBackward,
            ("stop", _) | ("move", Some("stop")) => Self::MoveStop,
            ("turn", Some("left")) | ("turnleft", _) => Self::TurnLeft,
            ("turn", Some("right")) | ("turnright", _) => Self::TurnRight,
            ("turn", Some("stop")) | ("turnstop", _) => Self::TurnStop,
            ("speed", Some("slow")) => Self::SpeedSlow,
            ("speed", Some("normal")) | ("speed", Some("fast")) => Self::SpeedNormal,
            ("torso", Some("left")) => Self::TorsoLeft,
            ("torso", Some("right")) => Self::TorsoRight,
            ("torso", Some("stop")) => Self::TorsoStop,
            ("torso", Some("reset")) => Self::TorsoReset,
            ("fire", _) => Self::FireOnce,
            ("auto", Some("on")) => Self::AutoFireOn,
            ("auto", Some("off")) => Self::AutoFireOff,
            ("estop", _) | ("emergency", _) => Self::EmergencyStop,
            ("clearrows", _) | ("resetorders", _) => Self::ResetCommands,
            ("status", _) => Self::SystemStatus,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::MoveForward => "MOVE FORWARD",
            Self::MoveBackward => "MOVE BACK",
            Self::MoveStop => "MOVE STOP",
            Self::TurnLeft => "BODY LEFT",
            Self::TurnRight => "BODY RIGHT",
            Self::TurnStop => "TURN STOP",
            Self::SpeedSlow => "SPEED SLOW",
            Self::SpeedNormal => "SPEED NORMAL",
            Self::TorsoLeft => "TORSO LEFT",
            Self::TorsoRight => "TORSO RIGHT",
            Self::TorsoStop => "TORSO STOP",
            Self::TorsoReset => "TORSO RESET",
            Self::FireOnce => "FIRE ONCE",
            Self::AutoFireOn => "AUTO FIRE ON",
            Self::AutoFireOff => "AUTO FIRE OFF",
            Self::EmergencyStop => "EMERGENCY STOP",
            Self::SystemStatus => "STATUS",
            Self::ResetCommands => "RESET ORDERS",
        }
    }
}

pub const MVP_COMMANDS: [MechCommand; 18] = [
    MechCommand::MoveForward,
    MechCommand::MoveBackward,
    MechCommand::MoveStop,
    MechCommand::TurnLeft,
    MechCommand::TurnRight,
    MechCommand::TurnStop,
    MechCommand::SpeedSlow,
    MechCommand::SpeedNormal,
    MechCommand::TorsoLeft,
    MechCommand::TorsoRight,
    MechCommand::TorsoStop,
    MechCommand::TorsoReset,
    MechCommand::FireOnce,
    MechCommand::AutoFireOn,
    MechCommand::AutoFireOff,
    MechCommand::EmergencyStop,
    MechCommand::SystemStatus,
    MechCommand::ResetCommands,
];
