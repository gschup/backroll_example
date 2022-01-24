mod game;

use backroll::*;
use backroll_transport_udp::{UdpConnectionConfig, UdpManager};
use bevy_tasks::TaskPool;
use game::{Game, GameState, PlayerInput, FPS};
use macroquad::prelude::*;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, Instant},
};
use structopt::StructOpt;

#[derive(StructOpt)]
struct Opt {
    #[structopt(short, long)]
    local_port: u16,
    #[structopt(short, long)]
    players: Vec<String>,
}

pub struct BackrollConfig;

impl Config for BackrollConfig {
    type Input = PlayerInput;
    type State = GameState;
}

/// returns a window config for macroquad to use
fn window_conf() -> Conf {
    Conf {
        window_title: "Box Game P2P".to_owned(),
        window_width: 600,
        window_height: 800,
        window_resizable: false,
        high_dpi: true,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // bevy task pool
    let pool = TaskPool::new();

    // read cmd line arguments
    let opt = Opt::from_args();
    let mut local_handle = PlayerHandle(0);
    let num_players = opt.players.len();
    assert!(num_players > 0);

    // udp socket
    let listen_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), opt.local_port);
    let socket = UdpManager::bind(pool.clone(), listen_addr)?;

    // create a backroll session
    let mut sess_builder = P2PSession::<BackrollConfig>::build().with_frame_delay(0);

    // add players
    for player_addr in opt.players.iter() {
        // local player
        if player_addr == "localhost" {
            local_handle = sess_builder.add_player(Player::Local);
        } else {
            // remote players
            let peer = socket.connect(UdpConnectionConfig::unbounded(player_addr.parse()?));
            sess_builder.add_player(Player::Remote(peer));
        }
    }

    let sess = sess_builder.start(pool)?;

    // Create a new box game
    let mut game = Game::new(num_players);

    // time variables for tick rate
    let mut last_update = Instant::now();
    let mut accumulator = Duration::ZERO;
    let fps_delta = 1. / FPS as f64;

    loop {
        // frames are only happening if the sessions are synchronized
        if sess.is_synchronized() {
            // get delta time from last iteration and accumulate it
            let delta = Instant::now().duration_since(last_update);
            accumulator = accumulator.saturating_add(delta);
            last_update = Instant::now();

            // if enough time is accumulated, we run a frame
            while accumulator.as_secs_f64() > fps_delta {
                // decrease accumulator
                accumulator = accumulator.saturating_sub(Duration::from_secs_f64(fps_delta));
                let local_input = game.local_input(local_handle);
                sess.add_local_input(local_handle, local_input)?;
                let cmds = sess.advance_frame();
                game.handle_commands(cmds);
            }
        }

        game.render();
        next_frame().await;
    }
}
