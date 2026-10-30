//! Play state: entering the world, chunk streaming, movement and visibility.

use crate::connection::Connection;
use crate::game;
use crate::server::Server;
use arcade_players::{Outbound, Player, OUTBOUND_QUEUE};
use arcade_protocol::packets::play::clientbound as cb;
use arcade_protocol::packets::play::serverbound as sb;
use arcade_protocol::packets::play::GameMode;
use arcade_protocol::{BlockPos, ChunkPos, Identifier, PacketReader, ServerboundPacket, Text};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

const KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(35);

pub async fn begin(conn: &mut Connection) -> anyhow::Result<()> {
    let Some(profile) = conn.profile.clone() else {
        conn.kick(Text::new("Login did not complete")).await;
        anyhow::bail!("no profile at end of configuration");
    };
    let mode = GameMode::parse(&conn.server.config.game_mode).unwrap_or(GameMode::Creative);
    let spawn = conn.server.world.spawn;
    let entity_id = conn.server.players.allocate_entity_id();
    let (tx, rx) = mpsc::channel::<Outbound>(OUTBOUND_QUEUE);
    let player = Arc::new(Player::new(
        profile,
        entity_id,
        conn.ip.clone(),
        Arc::clone(&conn.server.data),
        tx,
        spawn,
        mode,
    ));
    if let Some(info) = &conn.client_info {
        player.lock().view_distance =
            (info.view_distance as i32).clamp(2, conn.server.config.view_distance.clamp(2, 12));
    }
    if let Some(previous) = conn.server.players.add(Arc::clone(&player)) {
        // Should have been handled during login; be defensive anyway.
        previous.disconnect(Text::new("You logged in from another location"));
    }
    conn.server
        .metrics
        .players
        .store(conn.server.players.count() as i64, Ordering::Relaxed);
    conn.enter_play(Arc::clone(&player), rx);
    on_join(&conn.server, &player).await;
    Ok(())
}

pub async fn on_join(server: &Arc<Server>, player: &Arc<Player>) {
    let state = player.lock();
    let (x, y, z, yaw, pitch, center) = (
        state.x,
        state.y,
        state.z,
        state.yaw,
        state.pitch,
        state.chunk(),
    );
    drop(state);

    player.send(&cb::Login {
        entity_id: player.entity_id,
        is_hardcore: false,
        dimension_names: vec![Identifier::minecraft("overworld")],
        max_players: server.config.max_players as i32,
        view_distance: server.config.view_distance,
        simulation_distance: server.config.simulation_distance,
        reduced_debug_info: false,
        enable_respawn_screen: true,
        do_limited_crafting: false,
        spawn: cb::SpawnInfo {
            dimension_type: server.world.dimension_type_id,
            dimension_name: Identifier::minecraft("overworld"),
            hashed_seed: 0,
            game_mode: GameMode::Creative,
            previous_game_mode: None,
            is_debug: false,
            is_flat: true,
            death_location: None,
            portal_cooldown: 0,
            sea_level: server.world.sea_level,
        },
        online_mode: server.config.online_mode,
        enforces_secure_chat: false,
    });
    player.send(&cb::ChangeDifficulty {
        difficulty: 1,
        locked: false,
    });
    player.send(&cb::PlayerAbilities::for_game_mode(GameMode::Creative));
    player.send(&cb::SetHeldSlot { slot: 0 });
    player.send(&cb::EntityEvent {
        entity_id: player.entity_id,
        status: 24,
    });

    teleport(server, player, x, y, z, yaw, pitch);

    // Tab list: everyone the client knows about, including itself.
    let players = server.players.all();
    let mut entries = Vec::with_capacity(players.len());
    for other in &players {
        let (mode, latency) = {
            let s = other.lock();
            (s.game_mode, s.latency_ms)
        };
        entries.push(cb::PlayerInfoEntry {
            uuid: other.uuid,
            profile: Some(other.profile.clone()),
            game_mode: Some(mode),
            listed: Some(true),
            latency_ms: Some(latency),
            ..Default::default()
        });
    }
    use cb::player_info_action::*;
    player.send(&cb::PlayerInfoUpdate {
        actions: ADD_PLAYER | UPDATE_GAME_MODE | UPDATE_LISTED | UPDATE_LATENCY,
        entries,
    });
    server.broadcast(
        &cb::PlayerInfoUpdate::add(player.profile.clone(), GameMode::Creative, 0),
        Some(player.uuid),
    );

    player.send(&cb::SetTime {
        world_age: 0,
        time_of_day: 6000,
        advancing: true,
    });
    player.send(&cb::SetDefaultSpawnPosition {
        dimension: Identifier::minecraft("overworld"),
        position: BlockPos::new(0, server.world.sea_level, 0),
        yaw: 0.0,
        pitch: 0.0,
    });
    player.send(&cb::GameEvent {
        event: cb::GameEventKind::StartWaitingForChunks,
        value: 0.0,
    });
    player.send(&cb::SetCenterChunk {
        chunk_x: center.x,
        chunk_z: center.z,
    });
    player.send(&cb::SetRenderDistance {
        distance: server.config.view_distance,
    });
    player.send(&cb::SetSimulationDistance {
        distance: server.config.simulation_distance,
    });
    player.send(&cb::SetHealth {
        health: 20.0,
        food: 20,
        saturation: 5.0,
    });
    player.send(&cb::SetExperience {
        bar: 0.0,
        level: 0,
        total: 0,
    });

    stream_chunks(server, player, center, server.config.view_distance);
    player.send(&cb::SystemChat {
        content: Text::new("Welcome to Voxel Arcade RS"),
        overlay: false,
    });
    tracing::info!("{} joined ({} online)", player.name(), server.online());
}

fn stream_chunks(server: &Server, player: &Arc<Player>, center: ChunkPos, radius: i32) {
    let radius = radius.clamp(1, 12);
    let positions = arcade_world::view_chunks(center, radius);
    let missing: Vec<ChunkPos> = {
        let state = player.lock();
        positions
            .into_iter()
            .filter(|p| !state.loaded_chunks.contains(p))
            .collect()
    };
    if missing.is_empty() {
        return;
    }
    player.send(&cb::ChunkBatchStart);
    for pos in &missing {
        player.send_raw(server.chunk_payload(*pos));
    }
    player.send(&cb::ChunkBatchFinished {
        batch_size: missing.len() as i32,
    });
    let mut state = player.lock();
    for pos in missing {
        state.loaded_chunks.insert(pos);
    }
}

fn teleport(server: &Server, player: &Arc<Player>, x: f64, y: f64, z: f64, yaw: f32, pitch: f32) {
    let id = {
        let mut state = player.lock();
        state.teleport_counter = state.teleport_counter.wrapping_add(1);
        state.pending_teleport = Some(state.teleport_counter);
        state.x = x;
        state.y = y;
        state.z = z;
        state.yaw = yaw;
        state.pitch = pitch;
        state.teleport_counter
    };
    let _ = server;
    player.send(&cb::PlayerPosition {
        teleport_id: id,
        x,
        y,
        z,
        velocity_x: 0.0,
        velocity_y: 0.0,
        velocity_z: 0.0,
        yaw,
        pitch,
        relative_flags: 0,
    });
}

pub async fn keepalive(server: &Arc<Server>, player: &Arc<Player>) {
    let id = {
        let mut state = player.lock();
        if state.awaiting_keepalive {
            return;
        }
        let id = server.started.elapsed().as_millis() as i64;
        state.keepalive_id = id;
        state.keepalive_sent = Instant::now();
        state.awaiting_keepalive = true;
        id
    };
    player.send(&cb::KeepAlive { id });
}

pub fn keepalive_expired(player: &Arc<Player>) -> bool {
    let state = player.lock();
    state.awaiting_keepalive && state.keepalive_sent.elapsed() > KEEPALIVE_TIMEOUT
}

pub async fn on_quit(server: &Arc<Server>, player: &Arc<Player>) {
    if !server.players.remove_player(player) {
        return;
    }
    server
        .metrics
        .players
        .store(server.players.count() as i64, Ordering::Relaxed);
    server
        .games
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(player.uuid);
    if let Ok(packets) = server
        .mechs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .despawn(&server.data, player.uuid)
    {
        server.send_raw_all(&packets);
    }
    server.broadcast(
        &cb::PlayerInfoRemove {
            uuids: vec![player.uuid],
        },
        None,
    );
    for other in server.players.all() {
        let saw = other.lock().visible_players.remove(&player.uuid);
        if saw {
            other.send(&cb::RemoveEntities {
                entity_ids: vec![player.entity_id],
            });
        }
    }
    server.broadcast(
        &cb::SystemChat {
            content: Text::new(format!("{} left the game", player.name())).color("yellow"),
            overlay: false,
        },
        None,
    );
    tracing::info!("{} left ({} online)", player.name(), server.online());
}

pub async fn handle(
    server: &Arc<Server>,
    player: &Arc<Player>,
    name: &str,
    reader: &mut PacketReader<'_>,
    packet: &[u8],
) -> anyhow::Result<()> {
    player.lock().last_activity = Instant::now();
    if let Some(input) = server.adapter.decode_input(packet)? {
        let mut games = server.games.lock().unwrap_or_else(|e| e.into_inner());
        if games.is_playing(player.uuid) {
            games.input(player.uuid, input);
        }
        return Ok(());
    }
    match name {
        "accept_teleportation" => {
            let packet = sb::ConfirmTeleport::read(reader)?;
            let mut state = player.lock();
            if state.pending_teleport == Some(packet.teleport_id) {
                state.pending_teleport = None;
            }
        }
        "keep_alive" => {
            let packet = sb::KeepAlive::read(reader)?;
            let mut state = player.lock();
            if state.awaiting_keepalive && state.keepalive_id == packet.id {
                state.awaiting_keepalive = false;
                state.latency_ms = state.keepalive_sent.elapsed().as_millis() as i32;
            }
        }
        "move_player_pos" => {
            let packet = sb::MovePlayerPos::read(reader)?;
            apply_move(
                server,
                player,
                Some((packet.x, packet.y, packet.z)),
                None,
                packet.on_ground,
            );
        }
        "move_player_pos_rot" => {
            let packet = sb::MovePlayerPosRot::read(reader)?;
            apply_move(
                server,
                player,
                Some((packet.x, packet.y, packet.z)),
                Some((packet.yaw, packet.pitch)),
                packet.on_ground,
            );
        }
        "move_player_rot" => {
            let packet = sb::MovePlayerRot::read(reader)?;
            apply_move(
                server,
                player,
                None,
                Some((packet.yaw, packet.pitch)),
                packet.on_ground,
            );
        }
        "move_player_status_only" => {
            let packet = sb::MovePlayerStatusOnly::read(reader)?;
            apply_move(server, player, None, None, packet.on_ground);
        }
        "player_loaded" => {
            let _ = sb::PlayerLoaded::read(reader)?;
            player.lock().loaded = true;
            update_visibility(server, player);
            for other in server.players.all() {
                if other.uuid != player.uuid {
                    update_visibility(server, &other);
                }
            }
            let replay = server
                .mechs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .replay_packets();
            server.send_raw_to(player, &replay);
        }
        "interact" => {
            // Only the entity id is needed. The target offset uses the
            // variable-length LpVec3 layout and must not be parsed unless
            // required; reading it incorrectly would reject valid clicks.
            let entity_id = reader.read_varint()?;
            let command = {
                let mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                mechs.interaction(player.uuid, entity_id)
            };
            if let Some(command) = command {
                let label = {
                    let mut mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                    mechs.command(player.uuid, command)
                };
                if let Some(label) = label {
                    mech_reply(player, format!("Order accepted: {label}"));
                }
            }
        }
        "chunk_batch_received" => {
            let _ = sb::ChunkBatchReceived::read(reader)?;
        }
        "client_information" => {
            let info = sb::PlayClientInformation::read(reader)?;
            player.lock().view_distance =
                (info.0.view_distance as i32).clamp(2, server.config.view_distance.clamp(2, 12));
        }
        "chat" => {
            let chat = sb::ChatMessage::read(reader)?;
            if !chat.message.trim().is_empty() {
                server.broadcast(
                    &cb::SystemChat {
                        content: Text::new(format!("<{}> {}", player.name(), chat.message)),
                        overlay: false,
                    },
                    None,
                );
            }
        }
        "chat_command" => {
            let command = sb::ChatCommand::read(reader)?;
            handle_command(server, player, &command.command);
        }
        "chat_command_signed" => {
            let command = sb::SignedChatCommand::read(reader)?;
            handle_command(server, player, &command.command);
        }
        _ => {}
    }
    Ok(())
}

fn handle_command(server: &Arc<Server>, player: &Arc<Player>, command: &str) {
    let trimmed = command.trim();
    if let Some(rest) = trimmed.strip_prefix("mech") {
        if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            return mech_command(server, player, &parts);
        }
    }
    match trimmed {
        "spawn" | "tp" => {
            let (x, y, z) = server.world.spawn;
            teleport(server, player, x, y, z, 0.0, 0.0);
        }
        "flappy" | "play" => {
            game::start_for_player(server, player);
        }
        "help" => player.send(&cb::SystemChat {
            content: Text::new(
                "/spawn — platform; /flappy — map game; /mech spawn|despawn|status|reset|help",
            ),
            overlay: false,
        }),
        _ => player.send(&cb::SystemChat {
            content: Text::new("Unknown command. Try /help"),
            overlay: false,
        }),
    }
}

fn mech_reply(player: &Player, text: impl Into<String>) {
    player.send(&cb::SystemChat {
        content: Text::new(text),
        overlay: false,
    });
}

fn mech_command(server: &Arc<Server>, player: &Arc<Player>, parts: &[&str]) {
    let sub = parts.first().copied().unwrap_or("help");
    match sub {
        "spawn" => {
            let (x, z, yaw) = {
                let s = player.lock();
                (s.x, s.z, s.yaw)
            };
            let panel_yaw = yaw;
            let yaw = f64::from(yaw).to_radians();
            let position = [x - yaw.sin() * 12.0, 65.0, z + yaw.cos() * 12.0];
            let panel = [x - yaw.sin() * 4.0, 66.0, z + yaw.cos() * 4.0];
            let result = {
                let mut mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                mechs.spawn(
                    &server.data,
                    player.uuid,
                    position,
                    panel,
                    panel_yaw,
                    &mut || server.players.allocate_entity_id(),
                )
            };
            match result {
                Ok(spawn) => {
                    server.send_raw_all(&spawn.packets);
                    mech_reply(
                        player,
                        format!(
                            "Mech {} #{} deployed ({} entities incl. panel). Use /mech status.",
                            spawn.name,
                            spawn.mech_id,
                            spawn.packets.len() / 2
                        ),
                    );
                }
                Err(err) => mech_reply(player, format!("Cannot deploy mech: {err}")),
            }
        }
        "despawn" => {
            let result = {
                let mut mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                mechs.despawn(&server.data, player.uuid)
            };
            match result {
                Ok(packets) => {
                    server.send_raw_all(&packets);
                    mech_reply(player, "Mech removed.");
                }
                Err(err) => mech_reply(player, format!("Cannot remove mech: {err}")),
            }
        }
        "status" => {
            let status = {
                let mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                mechs.status(player.uuid)
            };
            match status {
                Ok(s) => mech_reply(
                    player,
                    format!(
                        "{} #{} alive={} voxels={} displays={} reactor={} weapon={} legs={}/{} torso={}",
                        s.name,
                        s.mech_id,
                        s.alive,
                        s.voxels,
                        s.display_entities,
                        s.reactor,
                        s.weapon,
                        s.left_leg,
                        s.right_leg,
                        s.torso
                    ),
                ),
                Err(err) => mech_reply(player, format!("No mech: {err}")),
            }
        }
        "reset" => {
            {
                let mut mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                if let Ok(packets) = mechs.despawn(&server.data, player.uuid) {
                    server.send_raw_all(&packets);
                }
            }
            mech_command(server, player, &["spawn"]);
        }
        _ => {
            let Some(command) = mech_wars::MechCommand::from_text(parts) else {
                mech_reply(
                    player,
                    "/mech spawn|despawn|status|reset | forward|back|stop | turn left|right|stop | torso left|right|stop|reset | fire | auto on|off | estop",
                );
                return;
            };
            let label = {
                let mut mechs = server.mechs.lock().unwrap_or_else(|e| e.into_inner());
                mechs.command(player.uuid, command)
            };
            match label {
                Some(label) => mech_reply(player, format!("Order accepted: {label}")),
                None => mech_reply(player, "You do not have a mech."),
            }
        }
    }
}

fn apply_move(
    server: &Arc<Server>,
    player: &Arc<Player>,
    pos: Option<(f64, f64, f64)>,
    rot: Option<(f32, f32)>,
    on_ground: bool,
) {
    let (from, old_chunk, new_chunk, view) = {
        let mut state = player.lock();
        let from = (state.x, state.y, state.z);
        let old_chunk = state.chunk();
        if let Some((x, y, z)) = pos {
            // Sanity bounds: reject obviously impossible coordinates instead
            // of letting them poison replication.
            if x.is_finite()
                && y.is_finite()
                && z.is_finite()
                && x.abs() < 3.0e7
                && z.abs() < 3.0e7
                && (-512.0..1024.0).contains(&y)
            {
                state.x = x;
                state.y = y;
                state.z = z;
            }
        }
        if let Some((yaw, pitch)) = rot {
            if yaw.is_finite() && pitch.is_finite() {
                state.yaw = yaw;
                state.pitch = pitch;
            }
        }
        state.on_ground = on_ground;
        state.last_activity = Instant::now();
        let new_chunk = state.chunk();
        (from, old_chunk, new_chunk, state.view_distance)
    };

    if old_chunk != new_chunk {
        player.send(&cb::SetCenterChunk {
            chunk_x: new_chunk.x,
            chunk_z: new_chunk.z,
        });
        stream_chunks(server, player, new_chunk, view);
    }
    broadcast_movement(server, player, from, pos.is_some(), rot.is_some());
}

fn broadcast_movement(
    server: &Arc<Server>,
    player: &Arc<Player>,
    from: (f64, f64, f64),
    moved: bool,
    rotated: bool,
) {
    if !moved && !rotated {
        return;
    }
    let (to, yaw, pitch, on_ground) = {
        let state = player.lock();
        (
            (state.x, state.y, state.z),
            state.yaw,
            state.pitch,
            state.on_ground,
        )
    };
    let watchers: Vec<Arc<Player>> = server
        .players
        .all()
        .into_iter()
        .filter(|other| {
            other.uuid != player.uuid && other.lock().visible_players.contains(&player.uuid)
        })
        .collect();
    if watchers.is_empty() {
        return;
    }
    let big_move = (to.0 - from.0).abs() >= 7.9
        || (to.1 - from.1).abs() >= 7.9
        || (to.2 - from.2).abs() >= 7.9;

    for other in watchers {
        if big_move {
            other.send(&cb::EntityPositionSync {
                entity_id: player.entity_id,
                x: to.0,
                y: to.1,
                z: to.2,
                yaw,
                pitch,
                on_ground,
            });
            continue;
        }
        match (moved, rotated) {
            (true, true) => other.send(&cb::MoveEntityPosRot {
                entity_id: player.entity_id,
                delta_x: cb::movement_delta(from.0, to.0),
                delta_y: cb::movement_delta(from.1, to.1),
                delta_z: cb::movement_delta(from.2, to.2),
                yaw,
                pitch,
                on_ground,
            }),
            (true, false) => other.send(&cb::MoveEntityPos {
                entity_id: player.entity_id,
                delta_x: cb::movement_delta(from.0, to.0),
                delta_y: cb::movement_delta(from.1, to.1),
                delta_z: cb::movement_delta(from.2, to.2),
                on_ground,
            }),
            (false, true) => other.send(&cb::MoveEntityRot {
                entity_id: player.entity_id,
                yaw,
                pitch,
                on_ground,
            }),
            (false, false) => {}
        }
    }
    if rotated {
        for other in server.players.all() {
            if other.uuid != player.uuid && other.lock().visible_players.contains(&player.uuid) {
                other.send(&cb::RotateHead {
                    entity_id: player.entity_id,
                    head_yaw: yaw,
                });
            }
        }
    }
}

fn update_visibility(server: &Arc<Server>, player: &Arc<Player>) {
    let (loaded, visible, center, radius) = {
        let state = player.lock();
        (
            state.loaded,
            state.visible_players.clone(),
            state.chunk(),
            state
                .view_distance
                .clamp(2, server.config.view_distance.clamp(2, 12)),
        )
    };
    if !loaded {
        return;
    }
    let mut should_see = HashSet::new();
    for other in server.players.all() {
        if other.uuid == player.uuid {
            continue;
        }
        let other_state = other.lock();
        if other_state.loaded && center.distance_to(other_state.chunk()) <= radius {
            should_see.insert(other.uuid);
        }
    }
    let appeared: Vec<Uuid> = should_see.difference(&visible).copied().collect();
    let vanished: Vec<Uuid> = visible.difference(&should_see).copied().collect();

    for uuid in &appeared {
        if let Some(other) = server.players.get(*uuid) {
            player.send(&spawn_entity(server, &other));
            let yaw = other.lock().yaw;
            player.send(&cb::RotateHead {
                entity_id: other.entity_id,
                head_yaw: yaw,
            });
        }
    }
    if !vanished.is_empty() {
        let ids: Vec<i32> = vanished
            .iter()
            .filter_map(|u| server.players.get(*u).map(|p| p.entity_id))
            .collect();
        if !ids.is_empty() {
            player.send(&cb::RemoveEntities { entity_ids: ids });
        }
    }

    let mut state = player.lock();
    for uuid in appeared {
        state.visible_players.insert(uuid);
    }
    for uuid in vanished {
        state.visible_players.remove(&uuid);
    }
}

fn spawn_entity(server: &Server, other: &Arc<Player>) -> cb::SpawnEntity {
    let state = other.lock();
    cb::SpawnEntity {
        entity_id: other.entity_id,
        uuid: other.uuid,
        entity_type: server.data.player_entity_type_id(),
        x: state.x,
        y: state.y,
        z: state.z,
        velocity: (0.0, 0.0, 0.0),
        pitch: state.pitch,
        yaw: state.yaw,
        head_yaw: state.yaw,
        data: 0,
    }
}
