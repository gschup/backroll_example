use backroll::{
    command::{Command, Commands},
    GameInput, PlayerHandle,
};
use bytemuck::*;
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

use crate::BackrollConfig;

type Frame = i32;

pub const FPS: f32 = 60.0;
const CHECKSUM_PERIOD: i32 = 100;
const NULL_FRAME: Frame = -1;

const SHIP_HEIGHT: f32 = 50.;
const SHIP_BASE: f32 = 40.;
const WINDOW_HEIGHT: f32 = 800.0;
const WINDOW_WIDTH: f32 = 600.0;

const INPUT_UP: u8 = 1 << 0;
const INPUT_DOWN: u8 = 1 << 1;
const INPUT_LEFT: u8 = 1 << 2;
const INPUT_RIGHT: u8 = 1 << 3;

const MOVEMENT_SPEED: f32 = 15.0 / FPS;
const ROTATION_SPEED: f32 = 2.5 / FPS;
const MAX_SPEED: f32 = 7.0;
const FRICTION: f32 = 0.98;

#[repr(C)]
#[derive(Clone, Copy, Eq, PartialEq, Pod, Zeroable)]
pub struct PlayerInput {
    pub buttons_pressed: u8,
}

// BoxGameState holds all relevant information about the game state
#[derive(Clone, Serialize, Deserialize)]
pub struct GameState {
    pub frame: i32,
    pub num_players: usize,
    pub positions: Vec<(f32, f32)>,
    pub velocities: Vec<(f32, f32)>,
    pub rotations: Vec<f32>,
}

impl GameState {
    pub fn new(num_players: usize) -> Self {
        let mut positions = Vec::new();
        let mut velocities = Vec::new();
        let mut rotations = Vec::new();

        let r = WINDOW_WIDTH / 4.0;

        for i in 0..num_players as i32 {
            let rot = i as f32 / num_players as f32 * 2.0 * std::f32::consts::PI;
            let x = WINDOW_WIDTH / 2.0 + r * rot.cos();
            let y = WINDOW_HEIGHT / 2.0 + r * rot.sin();
            positions.push((x, y));
            velocities.push((0.0, 0.0));
            rotations.push((rot + std::f32::consts::PI) % (2.0 * std::f32::consts::PI));
        }

        Self {
            frame: 0,
            num_players,
            positions,
            velocities,
            rotations,
        }
    }

    pub fn advance(&mut self, inputs: GameInput<PlayerInput>) {
        // increase the frame counter
        self.frame += 1;

        for i in 0..self.num_players {
            let handle = PlayerHandle(i);
            // get input of that player
            let input = if inputs.is_disconnected(handle).unwrap() {
                // disconnected players spin
                INPUT_LEFT
            } else {
                inputs.get(handle).unwrap().buttons_pressed
            };

            // old values
            let (old_x, old_y) = self.positions[i];
            let (old_vel_x, old_vel_y) = self.velocities[i];
            let mut rot = self.rotations[i];

            // slow down
            let mut vel_x = old_vel_x * FRICTION;
            let mut vel_y = old_vel_y * FRICTION;

            // thrust
            if input & INPUT_UP != 0 && input & INPUT_DOWN == 0 {
                vel_x += MOVEMENT_SPEED * rot.cos();
                vel_y += MOVEMENT_SPEED * rot.sin();
            }
            // break
            if input & INPUT_UP == 0 && input & INPUT_DOWN != 0 {
                vel_x -= MOVEMENT_SPEED * rot.cos();
                vel_y -= MOVEMENT_SPEED * rot.sin();
            }
            // turn left
            if input & INPUT_LEFT != 0 && input & INPUT_RIGHT == 0 {
                rot = (rot - ROTATION_SPEED).rem_euclid(2.0 * std::f32::consts::PI);
            }
            // turn right
            if input & INPUT_LEFT == 0 && input & INPUT_RIGHT != 0 {
                rot = (rot + ROTATION_SPEED).rem_euclid(2.0 * std::f32::consts::PI);
            }

            // limit speed
            let magnitude = (vel_x * vel_x + vel_y * vel_y).sqrt();
            if magnitude > MAX_SPEED {
                vel_x = (vel_x * MAX_SPEED) / magnitude;
                vel_y = (vel_y * MAX_SPEED) / magnitude;
            }

            // compute new position
            let mut x = old_x + vel_x;
            let mut y = old_y + vel_y;

            // constrain players to canvas borders
            x = x.max(0.0);
            x = x.min(WINDOW_WIDTH);
            y = y.max(0.0);
            y = y.min(WINDOW_HEIGHT);

            // update all state
            self.positions[i] = (x, y);
            self.velocities[i] = (vel_x, vel_y);
            self.rotations[i] = rot;
        }
    }
}

/// computes the fletcher16 checksum, copied from wikipedia: <https://en.wikipedia.org/wiki/Fletcher%27s_checksum>
fn fletcher16(data: &[u8]) -> u16 {
    let mut sum1: u16 = 0;
    let mut sum2: u16 = 0;

    for index in 0..data.len() {
        sum1 = (sum1 + data[index] as u16) % 255;
        sum2 = (sum2 + sum1) % 255;
    }

    (sum2 << 8) | sum1
}

// BoxGame will handle rendering, gamestate, inputs and GGRSRequests
pub struct Game {
    num_players: usize,
    game_state: GameState,
    last_checksum: (Frame, u16),
    periodic_checksum: (Frame, u16),
}

impl Game {
    pub fn new(num_players: usize) -> Self {
        assert!(num_players <= 4);
        Self {
            num_players,
            game_state: GameState::new(num_players),
            last_checksum: (NULL_FRAME, 0),
            periodic_checksum: (NULL_FRAME, 0),
        }
    }

    pub fn handle_commands(&mut self, cmds: Commands<BackrollConfig>) {
        for cmd in cmds.into_iter() {
            match cmd {
                Command::Save(save) => save.save_without_hash(self.game_state.clone()),
                Command::Load(load) => self.game_state = load.load(),
                Command::AdvanceFrame(inputs) => self.advance_frame(inputs),
                Command::Event(event) => println!("Event: {:?}", event),
            }
        }
    }

    fn advance_frame(&mut self, inputs: GameInput<PlayerInput>) {
        // advance the game state
        self.game_state.advance(inputs);

        // remember checksum to render it later
        // it is very inefficient to serialize the gamestate here just for the checksum
        let buffer = bincode::serialize(&self.game_state).unwrap();
        let checksum = fletcher16(&buffer);
        self.last_checksum = (self.game_state.frame, checksum);
        if self.game_state.frame % CHECKSUM_PERIOD == 0 {
            self.periodic_checksum = (self.game_state.frame, checksum);
        }
    }

    // renders the game to the window
    pub fn render(&self) {
        clear_background(BLACK);

        // render players
        for i in 0..self.num_players {
            let color = match i {
                0 => GOLD,
                1 => BLUE,
                2 => GREEN,
                3 => RED,
                _ => WHITE,
            };
            let (x, y) = self.game_state.positions[i];
            let rotation = self.game_state.rotations[i] + std::f32::consts::PI / 2.0;
            let v1 = Vec2::new(
                x + rotation.sin() * SHIP_HEIGHT / 2.,
                y - rotation.cos() * SHIP_HEIGHT / 2.,
            );
            let v2 = Vec2::new(
                x - rotation.cos() * SHIP_BASE / 2. - rotation.sin() * SHIP_HEIGHT / 2.,
                y - rotation.sin() * SHIP_BASE / 2. + rotation.cos() * SHIP_HEIGHT / 2.,
            );
            let v3 = Vec2::new(
                x + rotation.cos() * SHIP_BASE / 2. - rotation.sin() * SHIP_HEIGHT / 2.,
                y + rotation.sin() * SHIP_BASE / 2. + rotation.cos() * SHIP_HEIGHT / 2.,
            );
            draw_triangle(v1, v2, v3, color);
        }

        // render checksums
        let last_checksum_str = format!(
            "Frame {}: Checksum {}",
            self.last_checksum.0, self.last_checksum.1
        );
        let periodic_checksum_str = format!(
            "Frame {}: Checksum {}",
            self.periodic_checksum.0, self.periodic_checksum.1
        );
        draw_text(&last_checksum_str, 20.0, 20.0, 30.0, WHITE);
        draw_text(&periodic_checksum_str, 20.0, 40.0, 30.0, WHITE);
    }

    // creates a compact representation of currently pressed keys
    pub fn local_input(&self, handle: PlayerHandle) -> PlayerInput {
        let mut buttons_pressed: u8 = 0;

        // player 1 with WASD
        if handle.0 == 0 {
            if is_key_down(KeyCode::W) {
                buttons_pressed |= INPUT_UP;
            }
            if is_key_down(KeyCode::A) {
                buttons_pressed |= INPUT_LEFT;
            }
            if is_key_down(KeyCode::S) {
                buttons_pressed |= INPUT_DOWN;
            }
            if is_key_down(KeyCode::D) {
                buttons_pressed |= INPUT_RIGHT;
            }
        }
        // player 2 with arrow keys
        if handle.0 == 1 {
            if is_key_down(KeyCode::Up) {
                buttons_pressed |= INPUT_UP;
            }
            if is_key_down(KeyCode::Left) {
                buttons_pressed |= INPUT_LEFT;
            }
            if is_key_down(KeyCode::Down) {
                buttons_pressed |= INPUT_DOWN;
            }
            if is_key_down(KeyCode::Right) {
                buttons_pressed |= INPUT_RIGHT;
            }
        }

        PlayerInput { buttons_pressed }
    }
}
