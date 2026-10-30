//! One client connection: framing, state machine and dispatch.

use crate::config_phase;
use crate::login;
use crate::play;
use crate::server::Server;
use crate::status;
use arcade_players::{Outbound, Player};
use arcade_protocol::packets::config as config_packets;
use arcade_protocol::packets::config::ClientInformation;
use arcade_protocol::packets::handshake::{Handshake, Intent};
use arcade_protocol::packets::login as login_packets;
use arcade_protocol::packets::play::clientbound as cb;
use arcade_protocol::{
    ClientboundPacket, Codec, GameProfile, PacketReader, ProtocolError, State, Text,
};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

const MAX_READ: usize = 16 * 1024;

pub struct Connection {
    pub server: Arc<Server>,
    pub addr: SocketAddr,
    pub ip: String,
    reader: OwnedReadHalf,
    writer: OwnedWriteHalf,
    pub codec: Codec,
    pub state: State,
    pub protocol_version: i32,
    pub handshake: Option<Handshake>,
    pub verify_token: [u8; 4],
    pub claimed_name: String,
    pub profile: Option<GameProfile>,
    pub client_info: Option<ClientInformation>,
    pub player: Option<Arc<Player>>,
    outbound_rx: Option<mpsc::Receiver<Outbound>>,
    last_packet: Instant,
    config_sent: bool,
}

impl Connection {
    pub fn new(server: Arc<Server>, socket: TcpStream, addr: SocketAddr) -> Self {
        let (reader, writer) = socket.into_split();
        let token = uuid::Uuid::new_v4().into_bytes();
        Self {
            server,
            addr,
            ip: addr.ip().to_string(),
            reader,
            writer,
            codec: Codec::new(),
            state: State::Handshake,
            protocol_version: 0,
            handshake: None,
            verify_token: [token[0], token[1], token[2], token[3]],
            claimed_name: String::new(),
            profile: None,
            client_info: None,
            player: None,
            outbound_rx: None,
            last_packet: Instant::now(),
            config_sent: false,
        }
    }

    fn display_name(&self) -> String {
        self.profile
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| self.addr.to_string())
    }

    pub async fn run(&mut self) {
        let result = self.run_inner().await;
        if let Some(player) = self.player.take() {
            play::on_quit(&self.server, &player).await;
        }
        match result {
            Ok(()) => {}
            Err(err) => match err.downcast_ref::<std::io::Error>() {
                Some(io)
                    if matches!(
                        io.kind(),
                        std::io::ErrorKind::ConnectionReset
                            | std::io::ErrorKind::ConnectionAborted
                            | std::io::ErrorKind::BrokenPipe
                            | std::io::ErrorKind::UnexpectedEof
                    ) => {}
                _ => {
                    if self.state == State::Play {
                        tracing::info!("{} disconnected: {err}", self.display_name());
                    } else {
                        tracing::debug!("{} ({:?}) dropped: {err}", self.addr, self.state);
                    }
                }
            },
        }
    }

    async fn run_inner(&mut self) -> anyhow::Result<()> {
        let idle_timeout = Duration::from_secs(self.server.config.idle_timeout_seconds.max(5));
        let keepalive_period = Duration::from_secs(self.server.config.keepalive_seconds.max(5));
        let mut keepalive = tokio::time::interval(keepalive_period);
        keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut buf = vec![0u8; MAX_READ];

        loop {
            let idle = tokio::time::sleep_until(tokio::time::Instant::from_std(
                self.last_packet + idle_timeout,
            ));
            tokio::select! {
                read = self.reader.read(&mut buf) => {
                    let n = read?;
                    if n == 0 {
                        return Ok(());
                    }
                    self.server.metrics.bytes_in.fetch_add(n as u64, Ordering::Relaxed);
                    self.codec.push_received(&buf[..n]);
                    loop {
                        let packet = match self.codec.next_packet() {
                            Ok(Some(p)) => p,
                            Ok(None) => break,
                            Err(err) => {
                                self.kick(Text::new("Malformed packet")).await;
                                return Err(err.into());
                            }
                        };
                        self.last_packet = Instant::now();
                        self.server.metrics.packets_in.fetch_add(1, Ordering::Relaxed);
                        if let Err(err) = self.handle_packet(&packet).await {
                            match err.downcast_ref::<ProtocolError>() {
                                Some(ProtocolError::UnknownPacket { .. }) => {
                                    // Clients send plenty of packets this
                                    // playground does not care about.
                                }
                                _ => {
                                    self.kick(Text::new("Invalid packet")).await;
                                    return Err(err);
                                }
                            }
                        }
                        if self.state == State::Play
                            && self
                                .player
                                .as_ref()
                                .map(|p| p.is_slow())
                                .unwrap_or(false)
                        {
                            self.kick(Text::new("Outbound queue overflow")).await;
                            return Ok(());
                        }
                    }
                }
                message = recv_outbound(&mut self.outbound_rx) => {
                    match message {
                        Some(Outbound::Packet(bytes)) => {
                            let frame = self.codec.encode(&bytes);
                            self.server.metrics.bytes_out.fetch_add(frame.len() as u64, Ordering::Relaxed);
                            self.server.metrics.packets_out.fetch_add(1, Ordering::Relaxed);
                            self.writer.write_all(&frame).await?;
                        }
                        Some(Outbound::Disconnect(reason)) => {
                            self.kick(reason).await;
                            return Ok(());
                        }
                        None => return Ok(()),
                    }
                }
                _ = keepalive.tick(), if self.state == State::Play => {
                    if let Some(player) = &self.player {
                        play::keepalive(&self.server, player).await;
                        if play::keepalive_expired(player) {
                            self.kick(Text::new("Timed out")).await;
                            return Ok(());
                        }
                    }
                }
                _ = idle => {
                    self.kick(Text::new("Timed out")).await;
                    return Ok(());
                }
            }
        }
    }

    /// Direct send, used before the play state owns the outbound queue.
    pub async fn send<P: ClientboundPacket>(&mut self, packet: &P) -> anyhow::Result<()> {
        let payload = self.server.data.packet_ids.encode(packet)?;
        let frame = self.codec.encode(&payload);
        self.server
            .metrics
            .packets_out
            .fetch_add(1, Ordering::Relaxed);
        self.server
            .metrics
            .bytes_out
            .fetch_add(frame.len() as u64, Ordering::Relaxed);
        self.writer.write_all(&frame).await?;
        Ok(())
    }

    /// Writes an already-encoded `id + body` payload. Used for the large,
    /// immutable registry packets.
    pub async fn send_encoded(&mut self, payload: &[u8]) -> anyhow::Result<()> {
        let frame = self.codec.encode(payload);
        self.server
            .metrics
            .packets_out
            .fetch_add(1, Ordering::Relaxed);
        self.server
            .metrics
            .bytes_out
            .fetch_add(frame.len() as u64, Ordering::Relaxed);
        self.writer.write_all(&frame).await?;
        Ok(())
    }

    pub async fn kick(&mut self, reason: Text) {
        let result = match self.state {
            State::Login => self.send(&login_packets::LoginDisconnect { reason }).await,
            State::Config => {
                self.send(&config_packets::ConfigDisconnect { reason })
                    .await
            }
            State::Play => self.send(&cb::Disconnect { reason }).await,
            _ => Ok(()),
        };
        if let Err(err) = result {
            tracing::debug!("could not send kick to {}: {err}", self.addr);
        }
        let _ = self.writer.flush().await;
        let _ = self.writer.shutdown().await;
    }

    async fn handle_packet(&mut self, payload: &[u8]) -> anyhow::Result<()> {
        match self.state {
            State::Handshake => {
                if payload.first() == Some(&0xFE) {
                    return Ok(());
                }
                let mut reader = PacketReader::new(payload);
                let id = reader.read_varint()?;
                if id != 0 {
                    anyhow::bail!("expected handshake, got packet {id}");
                }
                let handshake = arcade_protocol::packets::read_exact::<Handshake>(&mut reader)?;
                self.protocol_version = handshake.protocol_version;
                self.state = match handshake.intent {
                    Intent::Status => State::Status,
                    Intent::Login | Intent::Transfer => State::Login,
                };
                self.handshake = Some(handshake);
                Ok(())
            }
            State::Status => {
                let (name, mut reader) = {
                    let ids = &self.server.data.packet_ids;
                    let (name, reader) = ids.decode(State::Status, payload)?;
                    (name.to_owned(), reader)
                };
                status::handle(self, &name, &mut reader).await
            }
            State::Login => {
                let (name, mut reader) = {
                    let ids = &self.server.data.packet_ids;
                    let (name, reader) = ids.decode(State::Login, payload)?;
                    (name.to_owned(), reader)
                };
                login::handle(self, &name, &mut reader).await
            }
            State::Config => {
                let (name, mut reader) = {
                    let ids = &self.server.data.packet_ids;
                    let (name, reader) = ids.decode(State::Config, payload)?;
                    (name.to_owned(), reader)
                };
                config_phase::handle(self, &name, &mut reader).await
            }
            State::Play => {
                let (name, mut reader) = {
                    let ids = &self.server.data.packet_ids;
                    let (name, reader) = ids.decode(State::Play, payload)?;
                    (name.to_owned(), reader)
                };
                let player = self.player.clone().expect("player exists in play state");
                play::handle(&self.server, &player, &name, &mut reader, payload).await
            }
        }
    }

    pub fn enter_play(&mut self, player: Arc<Player>, rx: mpsc::Receiver<Outbound>) {
        self.state = State::Play;
        self.player = Some(player);
        self.outbound_rx = Some(rx);
    }

    pub fn mark_config_sent(&mut self) {
        self.config_sent = true;
    }

    pub fn config_sent(&self) -> bool {
        self.config_sent
    }
}

async fn recv_outbound(rx: &mut Option<mpsc::Receiver<Outbound>>) -> Option<Outbound> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}
