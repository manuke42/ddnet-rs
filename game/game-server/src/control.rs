use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    str::FromStr,
    sync::{Arc, Condvar, Mutex},
};

use anyhow::{Context, Result, anyhow};
use base_io::runtime::{IoRuntime, IoRuntimeTask};
use futures::{SinkExt, StreamExt};
use game_base::player_input::PlayerInput;
use game_interface::types::{
    game::GameTickType,
    id_gen::IdGeneratorIdType,
    id_types::PlayerId,
    input::{CharacterInputMethodFlags, cursor::CharacterInputCursor},
    render::character::PlayerIngameMode,
};
use math::math::vector::dvec2;
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{Message, Utf8Bytes},
};

const BROADCAST_CHANNEL_CAPACITY: usize = 32;
const CONTROL_PORT: u16 = 5000;

#[derive(Debug, Clone, Copy)]
pub struct PlayerControlMessage {
    pub player_id: PlayerId,
    pub for_monotonic_tick: Option<GameTickType>,
    pub input: PlayerInput,
}

#[derive(Debug, Serialize)]
pub struct ControlTickReport {
    pub tick: GameTickType,
    pub map: String,
    pub player_count: usize,
    pub players: Vec<PlayerSnapshot>,
    pub stages: Vec<StageSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct PlayerSnapshot {
    pub player_id: u64,
    pub name: Option<String>,
    pub account: Option<String>,
    pub stage_id: Option<String>,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub speed: f32,
    pub move_dir: i32,
    pub current_weapon: String,
    pub has_air_jump: bool,
    pub phased: bool,
    pub hook_target: Option<u64>,
    pub ingame_mode: Option<String>,
    pub browser_score: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StageSnapshot {
    pub stage_id: String,
    pub game_ticks_passed: GameTickType,
    pub character_count: usize,
    pub projectile_count: usize,
    pub pickup_count: usize,
    pub laser_count: usize,
    pub flag_count: usize,
}

struct TickGate {
    inner: Mutex<TickGateState>,
    cvar: Condvar,
}

struct TickGateState {
    permits: usize,
    closed: bool,
}

impl TickGate {
    fn new() -> Self {
        Self {
            inner: Mutex::new(TickGateState {
                permits: 0,
                closed: false,
            }),
            cvar: Condvar::new(),
        }
    }

    fn wait_for_tick(&self) -> bool {
        let mut guard = self.inner.lock().unwrap();
        while guard.permits == 0 && !guard.closed {
            guard = self.cvar.wait(guard).unwrap();
        }
        if guard.closed {
            return false;
        }
        guard.permits = guard.permits.saturating_sub(1);
        true
    }

    fn allow(&self, count: usize) {
        if count == 0 {
            return;
        }
        let mut guard = self.inner.lock().unwrap();
        guard.permits = guard.permits.saturating_add(count);
        self.cvar.notify_all();
    }

    fn close(&self) {
        let mut guard = self.inner.lock().unwrap();
        guard.closed = true;
        self.cvar.notify_all();
    }
}

struct ControlInner {
    gate: TickGate,
    queue: Mutex<VecDeque<PlayerControlMessage>>,
    player_inputs: Mutex<HashMap<PlayerId, PlayerInput>>,
    state_tx: broadcast::Sender<String>,
    last_state: Mutex<Option<String>>,
}

impl ControlInner {
    fn new() -> Arc<Self> {
        let (state_tx, _rx) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Arc::new(Self {
            gate: TickGate::new(),
            queue: Mutex::new(VecDeque::new()),
            player_inputs: Mutex::new(HashMap::new()),
            state_tx,
            last_state: Mutex::new(None),
        })
    }
}

pub struct ControlBridge {
    inner: Arc<ControlInner>,
}

#[derive(Clone)]
pub struct ControlHandle {
    inner: Arc<ControlInner>,
}

impl ControlBridge {
    pub fn create() -> (Arc<Self>, ControlHandle) {
        let inner = ControlInner::new();
        let bridge = Arc::new(Self {
            inner: inner.clone(),
        });
        let handle = ControlHandle { inner };
        (bridge, handle)
    }

    pub fn wait_for_tick(&self) -> bool {
        self.inner.gate.wait_for_tick()
    }

    pub fn allow_ticks(&self, count: usize) {
        self.inner.gate.allow(count);
    }

    pub fn take_inputs(&self) -> Vec<PlayerControlMessage> {
        let mut guard = self.inner.queue.lock().unwrap();
        guard.drain(..).collect()
    }

    pub fn publish_state<T: Serialize>(&self, state: &T) {
        match serde_json::to_string(state) {
            Ok(json) => {
                {
                    let mut last = self.inner.last_state.lock().unwrap();
                    *last = Some(json.clone());
                }
                if let Err(err) = self.inner.state_tx.send(json) {
                    log::trace!("no websocket receivers for tick state: {err}");
                }
            }
            Err(err) => log::error!("failed to serialize control tick state: {err}"),
        }
    }

    pub fn close(&self) {
        self.inner.gate.close();
    }
}

impl ControlHandle {
    pub fn allow_ticks(&self, count: usize) {
        self.inner.gate.allow(count);
    }

    pub fn queue_input(&self, msg: InputCommand) -> Result<()> {
        let raw_id = IdGeneratorIdType::from_str(&msg.player_id.to_string())
            .map_err(|_| anyhow!("invalid player_id"))?;
        let player_id = PlayerId::from(raw_id);

        let mut players = self.inner.player_inputs.lock().unwrap();
        let stored = players
            .entry(player_id)
            .or_insert_with(PlayerInput::default);
        let mut input = *stored;

        if let Some(dir) = msg.dir {
            let dir_clamped = dir.clamp(-1, 1);
            input.inp.state.dir.set(dir_clamped);
        }
        if let Some(jump) = msg.jump {
            let was = *input.inp.state.jump;
            input.inp.state.jump.set(jump);
            if jump && !was {
                input.inp.consumable.jump.add(1);
            }
        }
        if let Some(fire) = msg.fire {
            let was = *input.inp.state.fire;
            input.inp.state.fire.set(fire);
            if fire && !was {
                input
                    .inp
                    .consumable
                    .fire
                    .add(1, CharacterInputCursor::default());
            }
        }
        if let Some(hook) = msg.hook {
            let was = *input.inp.state.hook;
            input.inp.state.hook.set(hook);
            if hook && !was {
                input
                    .inp
                    .consumable
                    .hook
                    .add(1, CharacterInputCursor::default());
            }
        }
        if let Some(cursor) = msg.cursor {
            let cursor_vec = dvec2::new(cursor[0], cursor[1]);
            input
                .inp
                .cursor
                .set(CharacterInputCursor::from_vec2(&cursor_vec));
        }
        input
            .inp
            .state
            .input_method_flags
            .set(CharacterInputMethodFlags::DUMMY);

        input.inc_version();
        *stored = input;
        drop(players);

        let mut queue = self.inner.queue.lock().unwrap();
        queue.push_back(PlayerControlMessage {
            player_id,
            for_monotonic_tick: msg.for_tick,
            input,
        });
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.inner.state_tx.subscribe()
    }

    pub fn last_state(&self) -> Option<String> {
        self.inner.last_state.lock().unwrap().clone()
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ControlCommand {
    Step {
        count: Option<usize>,
    },
    Input {
        player_id: u64,
        #[serde(default)]
        for_tick: Option<GameTickType>,
        #[serde(default)]
        dir: Option<i32>,
        #[serde(default)]
        jump: Option<bool>,
        #[serde(default)]
        fire: Option<bool>,
        #[serde(default)]
        hook: Option<bool>,
        #[serde(default)]
        cursor: Option<[f64; 2]>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct InputCommand {
    pub player_id: u64,
    pub for_tick: Option<GameTickType>,
    pub dir: Option<i32>,
    pub jump: Option<bool>,
    pub fire: Option<bool>,
    pub hook: Option<bool>,
    pub cursor: Option<[f64; 2]>,
}

impl From<ControlCommand> for Option<InputCommand> {
    fn from(cmd: ControlCommand) -> Self {
        match cmd {
            ControlCommand::Input {
                player_id,
                for_tick,
                dir,
                jump,
                fire,
                hook,
                cursor,
            } => Some(InputCommand {
                player_id,
                for_tick,
                dir,
                jump,
                fire,
                hook,
                cursor,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct AckResponse<'a> {
    r#type: &'a str,
    ok: bool,
    message: Option<String>,
}

pub fn spawn_control_server(io_rt: &IoRuntime, handle: ControlHandle) -> IoRuntimeTask<()> {
    io_rt.spawn(async move {
        let listener = TcpListener::bind(("0.0.0.0", CONTROL_PORT))
            .await
            .context("binding control websocket listener")?;
        log::info!(
            "control websocket listening on ws://0.0.0.0:{}",
            CONTROL_PORT
        );

        loop {
            let (stream, addr) = listener.accept().await?;
            let conn_handle = handle.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_connection(stream, addr, conn_handle).await {
                    log::warn!("control websocket {} closed with error: {err}", addr);
                }
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    addr: SocketAddr,
    handle: ControlHandle,
) -> Result<()> {
    let websocket = accept_async(stream)
        .await
        .with_context(|| format!("accepting websocket connection from {addr}"))?;
    let (mut ws_tx, mut ws_rx) = websocket.split();

    if let Some(snapshot) = handle.last_state() {
        ws_tx
            .send(Message::Text(Utf8Bytes::from(snapshot)))
            .await
            .context("sending initial tick snapshot")?;
    }

    let mut state_rx = handle.subscribe();
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<Message>();

    let mut tx_clone = ws_tx;
    let writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(msg) = response_rx.recv() => {
                    if tx_clone.send(msg).await.is_err() {
                        break;
                    }
                }
                Ok(state) = state_rx.recv() => {
                    if tx_clone
                        .send(Message::Text(Utf8Bytes::from(state)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    while let Some(msg) = ws_rx.next().await {
        match msg? {
            Message::Text(text) => {
                handle_command(&handle, &response_tx, &text)?;
            }
            Message::Binary(bin) => {
                if let Ok(text) = std::str::from_utf8(&bin) {
                    handle_command(&handle, &response_tx, text)?;
                } else {
                    let _ = response_tx.send(Message::Text(Utf8Bytes::from(
                        serde_json::to_string(&AckResponse {
                            r#type: "error",
                            ok: false,
                            message: Some("binary payload must be utf-8".to_string()),
                        })
                        .unwrap(),
                    )));
                }
            }
            Message::Ping(ping) => {
                let _ = response_tx.send(Message::Pong(ping));
            }
            Message::Pong(_) => {}
            Message::Close(_) => break,
            other => log::debug!("ignoring websocket frame from {addr}: {other:?}"),
        }
    }

    drop(response_tx);
    let _ = writer.await;
    Ok(())
}

fn handle_command(
    handle: &ControlHandle,
    response_tx: &mpsc::UnboundedSender<Message>,
    payload: &str,
) -> Result<()> {
    match serde_json::from_str::<ControlCommand>(payload) {
        Ok(ControlCommand::Step { count }) => {
            let permits = count.unwrap_or(1).max(1);
            handle.allow_ticks(permits);
            //log::info!(target: "server", "allowed {permits} tick(s) via control websocket");
            let _ = response_tx.send(Message::Text(Utf8Bytes::from(
                serde_json::to_string(&AckResponse {
                    r#type: "step_ack",
                    ok: true,
                    message: Some(format!("released {permits} tick(s)")),
                })
                .unwrap(),
            )));
        }
        Ok(cmd) => {
            if let Some(input) = Option::<InputCommand>::from(cmd) {
                match handle.queue_input(input) {
                    Ok(()) => {
                        log::info!(target: "server", "queued input for player_id {} via control websocket", input.player_id);
                        let _ = response_tx.send(Message::Text(Utf8Bytes::from(
                            serde_json::to_string(&AckResponse {
                                r#type: "input_ack",
                                ok: true,
                                message: None,
                            })
                            .unwrap(),
                        )));
                    }
                    Err(err) => {
                        let _ = response_tx.send(Message::Text(Utf8Bytes::from(
                            serde_json::to_string(&AckResponse {
                                r#type: "error",
                                ok: false,
                                message: Some(err.to_string()),
                            })
                            .unwrap(),
                        )));
                    }
                }
            }
        }
        Err(err) => {
            let _ = response_tx.send(Message::Text(Utf8Bytes::from(
                serde_json::to_string(&AckResponse {
                    r#type: "error",
                    ok: false,
                    message: Some(format!("failed to parse command: {err}")),
                })
                .unwrap(),
            )));
        }
    }
    Ok(())
}

pub fn format_ingame_mode(mode: &PlayerIngameMode) -> String {
    match mode {
        PlayerIngameMode::Spectator => "spectator".to_string(),
        PlayerIngameMode::InGame { in_custom_stage } => {
            if *in_custom_stage {
                "playing_custom_stage".to_string()
            } else {
                "playing".to_string()
            }
        }
    }
}
