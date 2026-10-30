//! A tiny server-side Flappy Bird-like game.
//!
//! The whole game is deterministic: the same input sequence and timestep
//! always produces the same result. Rendering is only a `Frame`.

use arcade_runtime::{Frame, GameInput, GameState, PlayerId};

pub const WIDTH: u32 = 32;
pub const HEIGHT: u32 = 24;
const GRAVITY: f32 = 18.0;
const FLAP: f32 = -6.5;
const PIPE_SPEED: f32 = 4.0;
const PIPE_GAP: f32 = 7.0;
const PIPE_WIDTH: f32 = 3.0;
const PIPE_SPACING: f32 = 14.0;
const BIRD_X: f32 = 6.0;
const BIRD_RADIUS: f32 = 1.0;

pub const SKY: [u8; 3] = [96, 170, 255];
pub const BIRD: [u8; 3] = [255, 220, 40];
pub const PIPE: [u8; 3] = [70, 190, 70];
pub const GROUND: [u8; 3] = [190, 150, 80];
pub const TEXT: [u8; 3] = [255, 255, 255];

#[derive(Clone, Copy, Debug, PartialEq)]
struct Pipe {
    x: f32,
    gap_y: f32,
    scored: bool,
}

pub struct FlappyBird {
    frame: Frame,
    bird_y: f32,
    velocity: f32,
    pipes: Vec<Pipe>,
    spawn_timer: f32,
    score: u32,
    best: u32,
    over: bool,
    rng: u64,
    revision: u64,
}

impl Default for FlappyBird {
    fn default() -> Self {
        Self::new()
    }
}

impl FlappyBird {
    pub fn new() -> Self {
        let mut game = Self {
            frame: Frame::solid(WIDTH, HEIGHT, SKY),
            bird_y: HEIGHT as f32 / 2.0,
            velocity: 0.0,
            pipes: Vec::new(),
            spawn_timer: 0.0,
            score: 0,
            best: 0,
            over: false,
            rng: 0x9E3779B97F4A7C15,
            revision: 0,
        };
        game.draw();
        game
    }

    pub fn score(&self) -> u32 {
        self.score
    }

    pub fn best(&self) -> u32 {
        self.best
    }

    pub fn is_over(&self) -> bool {
        self.over
    }

    pub fn bird_y(&self) -> f32 {
        self.bird_y
    }

    fn reset(&mut self) {
        self.bird_y = HEIGHT as f32 / 2.0;
        self.velocity = 0.0;
        self.pipes.clear();
        self.spawn_timer = 0.0;
        self.score = 0;
        self.over = false;
    }

    fn flap(&mut self) {
        if self.over {
            self.reset();
        } else {
            self.velocity = FLAP;
        }
    }

    fn next_random(&mut self) -> f32 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.rng >> 33) as f32) / (u32::MAX as f32 / 2.0)
    }

    fn collides(&self) -> bool {
        if self.bird_y - BIRD_RADIUS < 0.0 || self.bird_y + BIRD_RADIUS > (HEIGHT - 2) as f32 {
            return true;
        }
        for pipe in &self.pipes {
            let within_x =
                (BIRD_X + BIRD_RADIUS) > pipe.x && (BIRD_X - BIRD_RADIUS) < pipe.x + PIPE_WIDTH;
            if !within_x {
                continue;
            }
            let top = pipe.gap_y;
            let bottom = pipe.gap_y + PIPE_GAP;
            if self.bird_y - BIRD_RADIUS < top || self.bird_y + BIRD_RADIUS > bottom {
                return true;
            }
        }
        false
    }

    fn draw(&mut self) {
        self.frame.fill_rect(0, 0, WIDTH, HEIGHT, SKY);
        for pipe in &self.pipes {
            let x = pipe.x.max(0.0) as u32;
            let w = PIPE_WIDTH.ceil() as u32;
            let gap_top = pipe.gap_y.max(0.0) as u32;
            let gap_bottom = (pipe.gap_y + PIPE_GAP).min(HEIGHT as f32) as u32;
            self.frame.fill_rect(x, 0, w, gap_top, PIPE);
            self.frame
                .fill_rect(x, gap_bottom, w, HEIGHT.saturating_sub(gap_bottom), PIPE);
        }
        // Ground strip.
        self.frame.fill_rect(0, HEIGHT - 2, WIDTH, 2, GROUND);
        let by = self.bird_y.max(0.0) as u32;
        self.frame.fill_rect(BIRD_X as u32, by, 2, 2, BIRD);
        // Score pips: one white pixel per ten points, capped to the width.
        for i in 0..self.score.min(WIDTH / 2) {
            self.frame.set(i * 2, 1, TEXT);
        }
        self.revision += 1;
        self.frame.revision = self.revision;
    }
}

impl GameState for FlappyBird {
    fn tick(&mut self, dt: f32, inputs: &[(PlayerId, GameInput)]) {
        let jumped = inputs
            .iter()
            .any(|(_, input)| matches!(input, GameInput::Jump | GameInput::PrimaryAction));
        if jumped {
            self.flap();
        }
        if self.over {
            self.draw();
            return;
        }

        self.velocity += GRAVITY * dt;
        self.bird_y += self.velocity * dt;

        self.spawn_timer += dt;
        if self.spawn_timer >= PIPE_SPACING / PIPE_SPEED {
            self.spawn_timer = 0.0;
            let gap_y = 4.0 + self.next_random() * (HEIGHT as f32 - PIPE_GAP - 8.0);
            self.pipes.push(Pipe {
                x: WIDTH as f32,
                gap_y,
                scored: false,
            });
        }

        for pipe in &mut self.pipes {
            pipe.x -= PIPE_SPEED * dt;
            if !pipe.scored && pipe.x + PIPE_WIDTH < BIRD_X {
                pipe.scored = true;
                self.score += 1;
                self.best = self.best.max(self.score);
            }
        }
        self.pipes.retain(|p| p.x + PIPE_WIDTH > -2.0);

        if self.collides() {
            self.over = true;
        }
        self.draw();
    }

    fn frame(&self) -> &Frame {
        &self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bird_falls_and_flaps() {
        let mut game = FlappyBird::new();
        let start = game.bird_y();
        game.tick(0.1, &[]);
        assert!(game.bird_y() > start);
        game.tick(0.01, &[(0, GameInput::Jump)]);
        assert!(game.velocity < 0.0);
    }

    #[test]
    fn pipes_spawn() {
        let mut game = FlappyBird::new();
        // One large step crosses the first spawn interval. Physics may end
        // the run, but the pipe is spawned before collision is checked.
        game.tick(3.6, &[]);
        assert!(!game.pipes.is_empty());
    }

    #[test]
    fn game_over_and_restart() {
        let mut game = FlappyBird::new();
        for _ in 0..200 {
            game.tick(0.05, &[]);
            if game.is_over() {
                break;
            }
        }
        assert!(game.is_over());
        game.tick(0.05, &[(0, GameInput::Jump)]);
        assert!(!game.is_over());
        assert_eq!(game.score(), 0);
    }
}
