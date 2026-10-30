//! Minimal, transport-agnostic game runtime.
//!
//! Games never see Minecraft packets. They receive normalized [`GameInput`]
//! values and produce a low-resolution [`Frame`] that a renderer can expose
//! through the vanilla client.

use std::collections::HashMap;
use std::time::Duration;

pub type PlayerId = u64;
pub type SessionId = u64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameInput {
    Jump,
    PrimaryAction,
    SecondaryAction,
    Move { forward: f32, sideways: f32 },
    SelectSlot(u8),
}

#[derive(Clone, Debug)]
pub struct PlayerContext {
    pub id: PlayerId,
    pub name: String,
    pub joined_tick: u64,
}

/// A tiny RGB frame. Games write pixels; the renderer turns them into blocks
/// or map colours without knowing anything about the game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 3]>,
    pub revision: u64,
}

impl Frame {
    pub fn solid(width: u32, height: u32, color: [u8; 3]) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; (width * height) as usize],
            revision: 0,
        }
    }

    pub fn set(&mut self, x: u32, y: u32, color: [u8; 3]) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 3] {
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: [u8; 3]) {
        for yy in y..(y + h).min(self.height) {
            for xx in x..(x + w).min(self.width) {
                self.set(xx, yy, color);
            }
        }
    }
}

/// The simulation side of a game. Implementations must be deterministic for
/// the same inputs and timestep.
pub trait GameState: Send {
    fn tick(&mut self, dt: f32, inputs: &[(PlayerId, GameInput)]);
    fn frame(&self) -> &Frame;
    fn revision(&self) -> u64 {
        self.frame().revision
    }
}

pub struct GameSession {
    pub id: SessionId,
    pub game: Box<dyn GameState>,
    pub players: HashMap<PlayerId, PlayerContext>,
    pub tick: u64,
    pub running: bool,
}

impl GameSession {
    pub fn new(id: SessionId, game: Box<dyn GameState>) -> Self {
        Self {
            id,
            game,
            players: HashMap::new(),
            tick: 0,
            running: true,
        }
    }

    pub fn join(&mut self, id: PlayerId, name: impl Into<String>) -> bool {
        if self.players.contains_key(&id) {
            return false;
        }
        self.players.insert(
            id,
            PlayerContext {
                id,
                name: name.into(),
                joined_tick: self.tick,
            },
        );
        true
    }

    pub fn leave(&mut self, id: PlayerId) -> bool {
        self.players.remove(&id).is_some()
    }

    /// Advances exactly one simulation step.
    pub fn step(&mut self, dt: f32, inputs: &[(PlayerId, GameInput)]) {
        if !self.running {
            return;
        }
        self.game.tick(dt, inputs);
        self.tick += 1;
    }
}

/// Fixed-timestep accumulator. Networking may deliver a jittery delta; the
/// simulation always sees the same `dt`.
pub struct GameLoop {
    tick_rate_hz: u32,
    accumulator: f32,
    max_catch_up: u32,
}

impl GameLoop {
    pub fn new(tick_rate_hz: u32) -> Self {
        Self {
            tick_rate_hz: tick_rate_hz.max(1),
            accumulator: 0.0,
            max_catch_up: 5,
        }
    }

    pub fn tick_rate(&self) -> u32 {
        self.tick_rate_hz
    }

    pub fn step_seconds(&self) -> f32 {
        1.0 / self.tick_rate_hz as f32
    }

    /// Adds elapsed time and returns how many simulation steps to run.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        let step = self.step_seconds();
        self.accumulator += elapsed.as_secs_f32().min(0.25);
        let mut ticks = 0;
        while self.accumulator >= step && ticks < self.max_catch_up {
            self.accumulator -= step;
            ticks += 1;
        }
        if ticks == self.max_catch_up {
            self.accumulator = 0.0;
        }
        ticks
    }
}

#[derive(Default)]
pub struct SessionManager {
    sessions: HashMap<SessionId, GameSession>,
    next_session: SessionId,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            next_session: 1,
        }
    }

    pub fn create(&mut self, game: Box<dyn GameState>) -> SessionId {
        let id = self.next_session;
        self.next_session += 1;
        self.sessions.insert(id, GameSession::new(id, game));
        id
    }

    pub fn get(&self, id: SessionId) -> Option<&GameSession> {
        self.sessions.get(&id)
    }

    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut GameSession> {
        self.sessions.get_mut(&id)
    }

    pub fn destroy(&mut self, id: SessionId) -> bool {
        self.sessions.remove(&id).is_some()
    }

    pub fn sessions(&self) -> impl Iterator<Item = &GameSession> {
        self.sessions.values()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Counter {
        frame: Frame,
        ticks: u32,
        last: Option<GameInput>,
    }

    impl GameState for Counter {
        fn tick(&mut self, _dt: f32, inputs: &[(PlayerId, GameInput)]) {
            self.ticks += 1;
            if let Some((_, input)) = inputs.first() {
                self.last = Some(*input);
            }
            self.frame.revision = self.ticks as u64;
        }

        fn frame(&self) -> &Frame {
            &self.frame
        }
    }

    #[test]
    fn session_tracks_players_and_input() {
        let mut session = GameSession::new(
            1,
            Box::new(Counter {
                frame: Frame::solid(8, 8, [0, 0, 0]),
                ticks: 0,
                last: None,
            }),
        );
        assert!(session.join(7, "Alex"));
        assert!(!session.join(7, "Alex"));
        session.step(0.05, &[(7, GameInput::Jump)]);
        assert_eq!(session.tick, 1);
        assert!(session.leave(7));
    }

    #[test]
    fn fixed_timestep_accumulates() {
        let mut lp = GameLoop::new(20);
        assert_eq!(lp.advance(Duration::from_millis(10)), 0);
        assert_eq!(lp.advance(Duration::from_millis(45)), 1);
        assert_eq!(lp.advance(Duration::from_millis(100)), 2);
    }

    #[test]
    fn manager_creates_and_destroys() {
        let mut manager = SessionManager::new();
        let id = manager.create(Box::new(Counter {
            frame: Frame::solid(2, 2, [1, 2, 3]),
            ticks: 0,
            last: None,
        }));
        assert_eq!(manager.len(), 1);
        assert!(manager.destroy(id));
        assert!(manager.is_empty());
    }
}
