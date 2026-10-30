//! Mech Wars session state. This module owns instances and their renderers,
//! but never touches TCP or player queues.

use arcade_protocol_data::GameData;
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

use crate::control::{self, ControlPanel};
use crate::mech::{ComponentId, MechCommand, MechInstance};
use crate::rendering::{self, MechRenderer};

pub const MAX_MECHS_PER_ARENA: usize = 2;

#[derive(Debug, Error)]
pub enum MechError {
    #[error("Mech Wars is disabled")]
    Disabled,
    #[error("you already have a mech; use /mech despawn first")]
    AlreadyExists,
    #[error("the arena already has {MAX_MECHS_PER_ARENA} mechs")]
    ArenaFull,
    #[error("you do not have a mech")]
    NoMech,
    #[error("renderer: {0}")]
    Render(String),
}

pub struct ActiveMech {
    pub instance: MechInstance,
    pub renderer: MechRenderer,
    /// Original spawn payloads, replayed to players who join later.
    pub packets: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct MechStatus {
    pub mech_id: u64,
    pub name: &'static str,
    pub alive: bool,
    pub voxels: usize,
    pub display_entities: usize,
    pub reactor: u16,
    pub weapon: u16,
    pub left_leg: u16,
    pub right_leg: u16,
    pub torso: u16,
}

pub struct SpawnResult {
    pub mech_id: u64,
    pub name: &'static str,
    pub packets: Vec<Vec<u8>>,
}

pub struct MechWars {
    pub enabled: bool,
    mechs: HashMap<Uuid, ActiveMech>,
    panels: HashMap<Uuid, ControlPanel>,
    next_mech_id: u64,
}

impl Default for MechWars {
    fn default() -> Self {
        Self::new(true)
    }
}

impl MechWars {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            mechs: HashMap::new(),
            panels: HashMap::new(),
            next_mech_id: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.mechs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mechs.is_empty()
    }

    pub fn get(&self, owner: Uuid) -> Option<&ActiveMech> {
        self.mechs.get(&owner)
    }

    pub fn spawn(
        &mut self,
        data: &GameData,
        owner: Uuid,
        position: [f64; 3],
        panel_anchor: [f64; 3],
        panel_yaw: f32,
        next_entity_id: &mut dyn FnMut() -> i32,
    ) -> Result<SpawnResult, MechError> {
        if !self.enabled {
            return Err(MechError::Disabled);
        }
        if self.mechs.contains_key(&owner) {
            return Err(MechError::AlreadyExists);
        }
        if self.mechs.len() >= MAX_MECHS_PER_ARENA {
            return Err(MechError::ArenaFull);
        }
        let mech_id = self.next_mech_id;
        self.next_mech_id += 1;
        let instance = MechInstance::spawn(mech_id, owner, position);
        let rendered =
            rendering::spawn(data, &instance, next_entity_id).map_err(MechError::Render)?;
        let panel = control::panel::spawn(data, owner, panel_anchor, panel_yaw, next_entity_id)
            .map_err(MechError::Render)?;
        let name = instance.definition.name;
        let mut packets = rendered.packets.clone();
        packets.extend(panel.packets.iter().cloned());
        self.panels.insert(owner, panel);
        self.mechs.insert(
            owner,
            ActiveMech {
                instance,
                renderer: rendered.renderer,
                packets: rendered.packets.clone(),
            },
        );
        Ok(SpawnResult {
            mech_id,
            name,
            packets,
        })
    }

    pub fn despawn(&mut self, data: &GameData, owner: Uuid) -> Result<Vec<Vec<u8>>, MechError> {
        let active = self.mechs.remove(&owner).ok_or(MechError::NoMech)?;
        let mut packets =
            vec![rendering::despawn(data, &active.renderer).map_err(MechError::Render)?];
        if let Some(panel) = self.panels.remove(&owner) {
            if let Some(packet) = panel.remove_packet(data) {
                packets.push(packet);
            }
        }
        Ok(packets)
    }

    pub fn status(&self, owner: Uuid) -> Result<MechStatus, MechError> {
        let active = self.mechs.get(&owner).ok_or(MechError::NoMech)?;
        let mech = &active.instance;
        Ok(MechStatus {
            mech_id: mech.id,
            name: mech.definition.name,
            alive: mech.alive,
            voxels: mech.grid().alive_count(),
            display_entities: active.renderer.displays.len(),
            reactor: mech.component_health(ComponentId::Reactor),
            weapon: mech.component_health(ComponentId::MainWeapon),
            left_leg: mech.component_health(ComponentId::LeftLeg),
            right_leg: mech.component_health(ComponentId::RightLeg),
            torso: mech.component_health(ComponentId::Torso),
        })
    }

    pub fn owners(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.mechs.keys().copied()
    }

    /// Maps a clicked interaction entity to a command, if it belongs to this
    /// owner's panel.
    pub fn interaction(&self, owner: Uuid, entity_id: i32) -> Option<MechCommand> {
        self.panels.get(&owner)?.command_for(entity_id)
    }

    /// Applies a command to the owner's mech. Returns the command label when
    /// accepted.
    pub fn command(&mut self, owner: Uuid, command: MechCommand) -> Option<&'static str> {
        let active = self.mechs.get_mut(&owner)?;
        active.instance.apply_command(command);
        Some(command.label())
    }

    /// Advances every mech and returns the display sync payloads.
    pub fn tick(&mut self, data: &GameData, dt: f32) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        for active in self.mechs.values_mut() {
            active.instance.tick(dt);
            if active.instance.alive {
                packets.extend(rendering::block_display::sync(
                    data,
                    &active.instance,
                    &mut active.renderer,
                ));
            }
        }
        packets
    }

    /// Spawn payloads of every mech, for a player who joined after they were
    /// deployed. The entity ids and UUIDs are intentionally unchanged.
    pub fn replay_packets(&self) -> Vec<Vec<u8>> {
        let mut packets: Vec<Vec<u8>> = self
            .mechs
            .values()
            .flat_map(|active| active.packets.iter().cloned())
            .collect();
        for panel in self.panels.values() {
            packets.extend(panel.packets.iter().cloned());
        }
        packets
    }
}
