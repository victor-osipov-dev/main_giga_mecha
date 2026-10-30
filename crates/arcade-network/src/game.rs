//! Per-player arcade games rendered through a vanilla filled map.
//!
//! The game never touches packets. It receives normalized inputs and emits a
//! frame; this module turns that frame into `map_item_data` updates. Only the
//! changed rectangle of the 128x128 map is sent.

use arcade_protocol::packets::play::clientbound::{
    MapItemData, MapPatch, SetHeldSlot, SetPlayerInventory, SystemChat,
};
use arcade_protocol::packets::play::items::ItemStack;
use arcade_protocol::{PacketWriter, Text};
use arcade_render::{MapFrame, MapRenderer};
use arcade_runtime::{GameInput, GameLoop, GameState};
use flappy_bird::FlappyBird;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::server::Server;

const MAP_SIZE: u16 = 128;

struct Session {
    game: FlappyBird,
    map_id: i32,
    pending: Vec<GameInput>,
    last: MapFrame,
    renderer: MapRenderer,
}

#[derive(Default)]
pub struct GameHub {
    sessions: HashMap<Uuid, Session>,
    next_map_id: i32,
}

impl GameHub {
    pub fn start(&mut self, uuid: Uuid) -> (i32, MapItemData) {
        if self.next_map_id <= 0 {
            self.next_map_id = 1;
        }
        let map_id = self.next_map_id;
        self.next_map_id = self.next_map_id.wrapping_add(1);
        let game = FlappyBird::new();
        let renderer = MapRenderer::new(MAP_SIZE, MAP_SIZE);
        let frame = renderer.render(game.frame());
        let data = full_frame(map_id, &frame);
        self.sessions.insert(
            uuid,
            Session {
                game,
                map_id,
                pending: Vec::new(),
                last: frame,
                renderer,
            },
        );
        (map_id, data)
    }

    pub fn input(&mut self, uuid: Uuid, input: GameInput) -> bool {
        match self.sessions.get_mut(&uuid) {
            Some(session) => {
                session.pending.push(input);
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, uuid: Uuid) {
        self.sessions.remove(&uuid);
    }

    pub fn is_playing(&self, uuid: Uuid) -> bool {
        self.sessions.contains_key(&uuid)
    }

    pub fn tick(&mut self, dt: f32) -> Vec<(Uuid, MapItemData)> {
        let mut updates = Vec::new();
        for (uuid, session) in &mut self.sessions {
            let inputs: Vec<(u64, GameInput)> =
                session.pending.drain(..).map(|i| (0u64, i)).collect();
            session.game.tick(dt, &inputs);
            if session.game.frame().revision == session.last.revision {
                continue;
            }
            let frame = session.renderer.render(session.game.frame());
            if let Some(patch) = diff_patch(&session.last, &frame) {
                updates.push((*uuid, packet(session.map_id, patch)));
            }
            session.last = frame;
        }
        updates
    }
}

fn packet(map_id: i32, patch: MapPatch) -> MapItemData {
    MapItemData {
        map_id,
        scale: 0,
        locked: true,
        icons: None,
        patch: Some(patch),
    }
}

fn full_frame(map_id: i32, frame: &MapFrame) -> MapItemData {
    packet(
        map_id,
        MapPatch {
            columns: frame.width as u8,
            rows: frame.height as u8,
            x: 0,
            z: 0,
            data: frame.pixels.clone(),
        },
    )
}

/// Smallest rectangle containing every changed pixel, or `None` if the frames
/// are identical.
fn diff_patch(previous: &MapFrame, next: &MapFrame) -> Option<MapPatch> {
    if previous.width != next.width || previous.height != next.height {
        return Some(MapPatch {
            columns: next.width as u8,
            rows: next.height as u8,
            x: 0,
            z: 0,
            data: next.pixels.clone(),
        });
    }
    let (mut min_x, mut max_x) = (u16::MAX, 0u16);
    let (mut min_z, mut max_z) = (u16::MAX, 0u16);
    for z in 0..next.height {
        for x in 0..next.width {
            let i = (z * next.width + x) as usize;
            if previous.pixels[i] != next.pixels[i] {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_z = min_z.min(z);
                max_z = max_z.max(z);
            }
        }
    }
    if min_x == u16::MAX {
        return None;
    }
    let columns = (max_x - min_x + 1) as u8;
    let rows = (max_z - min_z + 1) as u8;
    let mut data = Vec::with_capacity(columns as usize * rows as usize);
    for z in min_z..=max_z {
        let start = (z * next.width + min_x) as usize;
        let end = start + columns as usize;
        data.extend_from_slice(&next.pixels[start..end]);
    }
    Some(MapPatch {
        columns,
        rows,
        x: min_x as u8,
        z: min_z as u8,
        data,
    })
}

/// Puts a filled map in hotbar slot 0 and sends the first frame.
pub fn start_for_player(server: &Arc<Server>, player: &Arc<arcade_players::Player>) {
    let (map_id, first_frame) = {
        let mut hub = server.games.lock().unwrap_or_else(|e| e.into_inner());
        hub.start(player.uuid)
    };
    let Some(item_id) = server.data.item_id("filled_map") else {
        player.send(&SystemChat {
            content: Text::new("This server has no filled_map item id"),
            overlay: false,
        });
        return;
    };
    let component = server.data.component_id("map_id").unwrap_or(0);
    let mut patch = PacketWriter::new();
    patch.write_varint(1); // one added component
    patch.write_varint(0); // none removed
    patch.write_varint(component);
    patch.write_varint(map_id);
    let item = ItemStack {
        item: item_id,
        count: 1,
        patch: patch.into_inner(),
    };
    // `set_player_inventory` uses raw player-inventory indices: hotbar 0 is 0.
    player.send(&SetPlayerInventory { slot: 0, item });
    player.send(&SetHeldSlot { slot: 0 });
    player.send(&first_frame);
    server.metrics.map_frames.fetch_add(1, Ordering::Relaxed);
    server.metrics.map_bytes.fetch_add(
        first_frame
            .patch
            .as_ref()
            .map(|p| p.data.len() as u64)
            .unwrap_or(0),
        Ordering::Relaxed,
    );
    player.send(&SystemChat {
        content: Text::new("Flappy Bird: right-click or punch to flap"),
        overlay: true,
    });
}

/// Ticks every game session at the configured rate and sends changed patches.
pub async fn game_loop(server: Arc<Server>) {
    let hz = server.config.game_update_hz.clamp(1, 20);
    let mut clock = GameLoop::new(hz);
    let step = clock.step_seconds();
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs_f32(step));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last = Instant::now();
    loop {
        ticker.tick().await;
        let elapsed = last.elapsed();
        last = Instant::now();
        let steps = clock.advance(elapsed).max(1);
        for _ in 0..steps {
            let updates = {
                let mut hub = server.games.lock().unwrap_or_else(|e| e.into_inner());
                hub.tick(step)
            };
            for (uuid, packet) in updates {
                if let Some(player) = server.players.get(uuid) {
                    server.metrics.map_frames.fetch_add(1, Ordering::Relaxed);
                    server.metrics.map_bytes.fetch_add(
                        packet
                            .patch
                            .as_ref()
                            .map(|p| p.data.len() as u64)
                            .unwrap_or(0),
                        Ordering::Relaxed,
                    );
                    player.send(&packet);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_finds_small_rectangle() {
        let mut a = vec![0u8; 12];
        let mut b = a.clone();
        b[6] = 7;
        b[10] = 8;
        let previous = MapFrame {
            width: 4,
            height: 3,
            pixels: a.clone(),
            revision: 0,
        };
        let next = MapFrame {
            width: 4,
            height: 3,
            pixels: b,
            revision: 1,
        };
        let patch = diff_patch(&previous, &next).expect("changed");
        assert_eq!((patch.x, patch.z, patch.columns, patch.rows), (2, 1, 1, 2));
        assert_eq!(patch.data, vec![7, 8]);
        a.fill(0);
        let same = MapFrame {
            width: 4,
            height: 3,
            pixels: a,
            revision: 2,
        };
        assert!(diff_patch(&previous, &same).is_none());
    }

    #[test]
    fn starter_frame_is_full_size() {
        let game = FlappyBird::new();
        let renderer = MapRenderer::new(MAP_SIZE, MAP_SIZE);
        let frame = renderer.render(game.frame());
        let packet = full_frame(1, &frame);
        let patch = packet.patch.unwrap();
        assert_eq!(
            (patch.columns, patch.rows, patch.x, patch.z),
            (128, 128, 0, 0)
        );
        assert_eq!(patch.data.len(), 128 * 128);
    }
}
