//! Loopback-only binary control and state stream for local AI actor processes.

use std::{
    collections::{HashMap, VecDeque},
    os::unix::fs::PermissionsExt,
    path::Path,
    str::FromStr,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use base_io::runtime::{IoRuntime, IoRuntimeTask};
use game_base::player_input::PlayerInput;
use game_interface::types::{
    game::GameTickType,
    id_gen::IdGeneratorIdType,
    id_types::PlayerId,
    input::{CharacterInputMethodFlags, cursor::CharacterInputCursor},
    weapons::WeaponType,
};
use math::math::vector::dvec2;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::broadcast,
};

pub const AI_SOCKET_PATH: &str = "/tmp/ddnet-ai.sock";
pub const AI_SOCKET_PATH_ENV: &str = "DDNET_AI_SOCKET_PATH";

const MAGIC: &[u8; 4] = b"DAI1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 6;
const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;
const HELLO: u8 = 1;
const STEP: u8 = 2;
const INPUT: u8 = 3;
const RUN_MODE: u8 = 4;
const RESET: u8 = 5;
const RESPAWN: u8 = 6;
const SPAWN: u8 = 7;
const DESPAWN: u8 = 8;
const RESET_ACTOR: u8 = 9;
const INFO: u8 = 10;
const ACK: u8 = 128;
const MAP: u8 = 129;
const STATE: u8 = 130;
const ERROR: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlTickMode {
    Manual,
    Realtime,
    Unbounded,
}

#[derive(Debug, Clone)]
pub struct AiMapTile {
    pub x: u16,
    pub y: u16,
    pub layer: u8,
    pub tile: u8,
    pub value: u16,
    pub aux: u16,
}

#[derive(Debug, Clone)]
pub struct AiMap {
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<AiMapTile>,
}

#[derive(Debug, Clone, Copy)]
pub struct AiDynamicObject {
    pub object_id: u64,
    pub kind: u8,
    pub subtype: u8,
    pub properties: u16,
    pub state: u32,
    pub stage_id: u32,
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    pub size: [f32; 2],
    pub scalars: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct AiTickState {
    pub tick: GameTickType,
    pub ticks_per_second: u32,
    pub objects: Vec<AiDynamicObject>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayerControlMessage {
    pub player_id: PlayerId,
    pub for_monotonic_tick: Option<GameTickType>,
    pub input: PlayerInput,
}

struct TickGate {
    inner: Mutex<TickGateState>,
    cvar: Condvar,
}

struct TickGateState {
    permits: usize,
    closed: bool,
    mode: ControlTickMode,
}

impl TickGate {
    fn new() -> Self {
        Self {
            inner: Mutex::new(TickGateState {
                permits: 0,
                closed: false,
                mode: ControlTickMode::Realtime,
            }),
            cvar: Condvar::new(),
        }
    }

    fn wait_for_tick(&self) -> bool {
        let mut guard = self.inner.lock().unwrap();
        while guard.permits == 0 && !guard.closed && matches!(guard.mode, ControlTickMode::Manual) {
            guard = self.cvar.wait(guard).unwrap();
        }
        if guard.closed {
            return false;
        }
        if matches!(guard.mode, ControlTickMode::Manual) {
            guard.permits = guard.permits.saturating_sub(1);
        }
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

    fn mode(&self) -> ControlTickMode {
        self.inner.lock().unwrap().mode
    }

    fn set_mode(&self, mode: ControlTickMode) {
        let mut guard = self.inner.lock().unwrap();
        guard.mode = mode;
        if !matches!(mode, ControlTickMode::Manual) {
            guard.permits = 0;
        }
        self.cvar.notify_all();
    }
}

struct ControlInner {
    gate: TickGate,
    queue: Mutex<VecDeque<PlayerControlMessage>>,
    player_inputs: Mutex<HashMap<PlayerId, PlayerInput>>,
    frames: broadcast::Sender<Vec<u8>>,
    last_map: Mutex<Option<Vec<u8>>>,
    info: Mutex<Vec<u8>>,
    last_state: Mutex<Option<Vec<u8>>>,
    map_epoch: Mutex<u64>,
    current_tick: AtomicU64,
    reset_requested: Mutex<bool>,
    respawn_requests: Mutex<Vec<PlayerId>>,
    spawn_requests: Mutex<usize>,
    actor_resets: Mutex<Vec<(PlayerId, Option<[f32; 2]>)>>,
    despawn_requests: Mutex<Vec<PlayerId>>,
}

impl ControlInner {
    fn new() -> Arc<Self> {
        let (frames, _) = broadcast::channel(32);
        Arc::new(Self {
            gate: TickGate::new(),
            queue: Mutex::new(VecDeque::new()),
            player_inputs: Mutex::new(HashMap::new()),
            frames,
            last_map: Mutex::new(None),
            info: Mutex::new(Vec::new()),
            last_state: Mutex::new(None),
            map_epoch: Mutex::new(0),
            current_tick: AtomicU64::new(0),
            reset_requested: Mutex::new(false),
            respawn_requests: Mutex::new(Vec::new()),
            spawn_requests: Mutex::new(0),
            actor_resets: Mutex::new(Vec::new()),
            despawn_requests: Mutex::new(Vec::new()),
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
        (
            Arc::new(Self {
                inner: inner.clone(),
            }),
            ControlHandle { inner },
        )
    }

    pub fn wait_for_tick(&self) -> bool {
        self.inner.gate.wait_for_tick()
    }

    pub fn tick_mode(&self) -> ControlTickMode {
        self.inner.gate.mode()
    }

    pub fn take_inputs(&self) -> Vec<PlayerControlMessage> {
        self.inner.queue.lock().unwrap().drain(..).collect()
    }

    pub fn take_reset_request(&self) -> bool {
        let mut requested = self.inner.reset_requested.lock().unwrap();
        std::mem::take(&mut *requested)
    }

    pub fn take_respawn_requests(&self) -> Vec<PlayerId> {
        std::mem::take(&mut *self.inner.respawn_requests.lock().unwrap())
    }

    pub fn take_actor_resets(&self) -> Vec<(PlayerId, Option<[f32; 2]>)> {
        std::mem::take(&mut *self.inner.actor_resets.lock().unwrap())
    }

    pub fn take_spawn_requests(&self) -> usize {
        std::mem::take(&mut *self.inner.spawn_requests.lock().unwrap())
    }

    pub fn take_despawn_requests(&self) -> Vec<PlayerId> {
        std::mem::take(&mut *self.inner.despawn_requests.lock().unwrap())
    }

    pub fn set_ai_map(&self, map: &AiMap, name: &str, map_hash: &str) {
        let metadata = serde_json::json!({"bridge_version": 2, "map_name": name, "map_hash": map_hash,
            "isolated_stages": true, "maintenance_resets": true});
        *self.inner.info.lock().unwrap() = encode_message(INFO, metadata.to_string().as_bytes());
        let mut epoch = self.inner.map_epoch.lock().unwrap();
        *epoch = epoch.saturating_add(1);
        let frame = encode_map(*epoch, map);
        *self.inner.last_map.lock().unwrap() = Some(frame.clone());
        let _ = self.inner.frames.send(frame);
    }

    pub fn set_ai_tick(&self, tick: GameTickType) {
        self.inner.current_tick.store(tick, Ordering::Relaxed);
    }

    pub fn publish_ai_state(&self, state: &AiTickState) {
        self.set_ai_tick(state.tick);
        if self.inner.frames.receiver_count() == 0 {
            return;
        }
        let map_epoch = *self.inner.map_epoch.lock().unwrap();
        let frame = encode_state(map_epoch, state);
        *self.inner.last_state.lock().unwrap() = Some(frame.clone());
        let _ = self.inner.frames.send(frame);
    }

    pub fn has_ai_receivers(&self) -> bool {
        self.inner.frames.receiver_count() > 0
    }

    pub fn close(&self) {
        self.inner.gate.close();
    }
}

impl ControlHandle {
    fn current_tick(&self) -> u64 {
        self.inner.current_tick.load(Ordering::Relaxed)
    }

    fn tick_mode(&self) -> ControlTickMode {
        self.inner.gate.mode()
    }

    fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.inner.frames.subscribe()
    }

    fn last_map(&self) -> Option<Vec<u8>> {
        self.inner.last_map.lock().unwrap().clone()
    }

    fn last_state(&self) -> Option<Vec<u8>> {
        self.inner.last_state.lock().unwrap().clone()
    }

    fn allow_ticks(&self, count: usize) {
        self.inner.gate.allow(count);
    }

    fn set_tick_mode(&self, mode: ControlTickMode) {
        self.inner.gate.set_mode(mode);
    }

    fn request_reset(&self) {
        *self.inner.reset_requested.lock().unwrap() = true;
    }

    fn request_respawn(&self, raw_player_id: u64) -> Result<()> {
        let raw_id = IdGeneratorIdType::from_str(&raw_player_id.to_string())
            .map_err(|_| anyhow!("invalid player_id"))?;
        self.inner
            .respawn_requests
            .lock()
            .unwrap()
            .push(PlayerId::from(raw_id));
        Ok(())
    }

    fn request_spawn(&self) {
        let mut requests = self.inner.spawn_requests.lock().unwrap();
        *requests = requests.saturating_add(1);
    }

    fn request_despawn(&self, raw_player_id: u64) -> Result<()> {
        let raw_id = IdGeneratorIdType::from_str(&raw_player_id.to_string())
            .map_err(|_| anyhow!("invalid player_id"))?;
        self.inner
            .despawn_requests
            .lock()
            .unwrap()
            .push(PlayerId::from(raw_id));
        Ok(())
    }

    fn queue_input(&self, input: AiInput) -> Result<()> {
        let raw_id = IdGeneratorIdType::from_str(&input.player_id.to_string())
            .map_err(|_| anyhow!("invalid player_id"))?;
        let player_id = PlayerId::from(raw_id);
        let mut inputs = self.inner.player_inputs.lock().unwrap();
        let stored = inputs.entry(player_id).or_default();
        let mut next = *stored;
        let cursor = CharacterInputCursor::from_vec2(&dvec2::new(input.cursor[0], input.cursor[1]));

        next.inp.state.dir.set(input.direction);
        let was_jump = *next.inp.state.jump;
        next.inp.state.jump.set(input.jump);
        if input.jump && !was_jump {
            next.inp.consumable.jump.add(1);
        }
        let was_fire = *next.inp.state.fire;
        next.inp.state.fire.set(input.fire);
        if input.fire && !was_fire {
            next.inp.consumable.fire.add(1, cursor);
        }
        let was_hook = *next.inp.state.hook;
        next.inp.state.hook.set(input.hook);
        if input.hook && !was_hook {
            next.inp.consumable.hook.add(1, cursor);
        }
        next.inp.cursor.set(cursor);
        if let Some(weapon) = input.weapon {
            next.inp.consumable.set_weapon_req(Some(weapon));
        }
        next.inp
            .state
            .input_method_flags
            .set(CharacterInputMethodFlags::DUMMY);
        next.inc_version();
        *stored = next;
        drop(inputs);

        self.inner
            .queue
            .lock()
            .unwrap()
            .push_back(PlayerControlMessage {
                player_id,
                for_monotonic_tick: input.for_tick,
                input: next,
            });
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct AiInput {
    player_id: u64,
    for_tick: Option<GameTickType>,
    direction: i32,
    jump: bool,
    fire: bool,
    hook: bool,
    weapon: Option<WeaponType>,
    cursor: [f64; 2],
}

pub fn spawn_ai_socket_server(io_rt: &IoRuntime, handle: ControlHandle) -> IoRuntimeTask<()> {
    let socket_path =
        std::env::var(AI_SOCKET_PATH_ENV).unwrap_or_else(|_| AI_SOCKET_PATH.to_owned());
    io_rt.spawn(async move {
        let path = Path::new(&socket_path);
        if let Err(error) = std::fs::remove_file(path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(anyhow!(
                "removing stale AI socket at {}: {error}",
                path.display()
            ));
        }
        let listener = UnixListener::bind(path)
            .with_context(|| format!("binding AI socket at {}", path.display()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        log::info!("AI binary socket listening at {}", path.display());
        loop {
            let (stream, _) = listener.accept().await?;
            let handle = handle.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(stream, handle).await {
                    log::debug!("AI socket connection closed: {error}");
                }
            });
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })
}

async fn handle_connection(stream: UnixStream, handle: ControlHandle) -> Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    if let Some(map) = handle.last_map() {
        write_frame(&mut writer, &map).await?;
    }
    if let Some(state) = handle.last_state() {
        write_frame(&mut writer, &state).await?;
    }
    let mut frames = handle.subscribe();
    loop {
        tokio::select! {
            frame = read_frame(&mut reader) => {
                let response = match frame.and_then(|frame| handle_command(&handle, &frame)) {
                    Ok(response) => response,
                    Err(error) => encode_message(ERROR, error.to_string().as_bytes()),
                };
                write_frame(&mut writer, &response).await?;
            }
            received = frames.recv() => match received {
                Ok(frame) => write_frame(&mut writer, &frame).await?,
                Err(broadcast::error::RecvError::Lagged(count)) => log::debug!("AI socket skipped {count} stale frames"),
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            },
        }
    }
}

async fn read_frame(reader: &mut tokio::net::unix::OwnedReadHalf) -> Result<Vec<u8>> {
    let length = reader.read_u32_le().await? as usize;
    if !(HEADER_LEN..=MAX_FRAME_LEN).contains(&length) {
        return Err(anyhow!("invalid AI frame length: {length}"));
    }
    let mut frame = vec![0; length];
    reader.read_exact(&mut frame).await?;
    Ok(frame)
}

async fn write_frame(writer: &mut tokio::net::unix::OwnedWriteHalf, frame: &[u8]) -> Result<()> {
    writer
        .write_u32_le(u32::try_from(frame.len()).context("AI frame is too large")?)
        .await?;
    writer.write_all(frame).await?;
    Ok(())
}

fn handle_command(handle: &ControlHandle, frame: &[u8]) -> Result<Vec<u8>> {
    let (kind, body) = split_frame(frame)?;
    let tick = match kind {
        INFO => {
            expect_length(body, 0)?;
            return Ok(handle.inner.info.lock().unwrap().clone());
        }
        HELLO => {
            expect_length(body, 8)?;
            if matches!(handle.tick_mode(), ControlTickMode::Manual) {
                let tick = handle.current_tick().saturating_add(1);
                handle.allow_ticks(1);
                tick
            } else {
                handle.current_tick()
            }
        }
        INPUT => {
            expect_length(body, 29)?;
            handle.queue_input(decode_input(body)?)?;
            handle.current_tick()
        }
        STEP => {
            expect_length(body, 4)?;
            let count = u32::from_le_bytes(body.try_into().unwrap()).max(1) as usize;
            handle.set_tick_mode(ControlTickMode::Manual);
            let tick = handle.current_tick().saturating_add(count as u64);
            handle.allow_ticks(count);
            tick
        }
        RUN_MODE => {
            expect_length(body, 1)?;
            handle.set_tick_mode(match body[0] {
                0 => ControlTickMode::Manual,
                1 => ControlTickMode::Realtime,
                2 => ControlTickMode::Unbounded,
                _ => return Err(anyhow!("invalid AI tick mode")),
            });
            handle.current_tick()
        }
        RESET => {
            expect_length(body, 0)?;
            handle.request_reset();
            handle.set_tick_mode(ControlTickMode::Manual);
            handle.allow_ticks(1);
            handle.current_tick()
        }
        RESPAWN => {
            expect_length(body, 8)?;
            handle.request_respawn(u64::from_le_bytes(body.try_into().unwrap()))?;
            handle.set_tick_mode(ControlTickMode::Manual);
            // Vanilla respawn waits for five game ticks after a kill request.
            const RESPAWN_TICKS: usize = 6;
            let tick = handle.current_tick().saturating_add(RESPAWN_TICKS as u64);
            handle.allow_ticks(RESPAWN_TICKS);
            tick
        }
        RESET_ACTOR => {
            expect_length(body, 17)?;
            let raw = u64::from_le_bytes(body[..8].try_into().unwrap());
            let id = IdGeneratorIdType::from_str(&raw.to_string())
                .map_err(|_| anyhow!("invalid player_id"))?;
            let pos = [
                f32::from_le_bytes(body[9..13].try_into().unwrap()),
                f32::from_le_bytes(body[13..17].try_into().unwrap()),
            ];
            if body[8] > 1 || (body[8] == 1 && pos.iter().any(|x| !x.is_finite() || *x < 0.0)) {
                return Err(anyhow!("invalid scenario position"));
            }
            handle
                .inner
                .player_inputs
                .lock()
                .unwrap()
                .remove(&PlayerId::from(id));
            handle
                .inner
                .queue
                .lock()
                .unwrap()
                .retain(|input| input.player_id != PlayerId::from(id));
            handle
                .inner
                .actor_resets
                .lock()
                .unwrap()
                .push((PlayerId::from(id), (body[8] == 1).then_some(pos)));
            handle.set_tick_mode(ControlTickMode::Manual);
            let tick = handle.current_tick().saturating_add(1);
            handle.allow_ticks(1);
            tick
        }
        SPAWN => {
            expect_length(body, 0)?;
            handle.request_spawn();
            handle.set_tick_mode(ControlTickMode::Manual);
            let tick = handle.current_tick().saturating_add(1);
            handle.allow_ticks(1);
            tick
        }
        DESPAWN => {
            expect_length(body, 8)?;
            handle.request_despawn(u64::from_le_bytes(body.try_into().unwrap()))?;
            handle.set_tick_mode(ControlTickMode::Manual);
            let tick = handle.current_tick().saturating_add(1);
            handle.allow_ticks(1);
            tick
        }
        _ => return Err(anyhow!("unsupported AI message type: {kind}")),
    };
    Ok(encode_ack(kind, tick))
}

fn decode_input(body: &[u8]) -> Result<AiInput> {
    let direction = body[16] as i8 as i32;
    if !(-1..=1).contains(&direction) {
        return Err(anyhow!("invalid movement direction"));
    }
    let weapon = match body[18] {
        0 => None,
        1 => Some(WeaponType::Hammer),
        2 => Some(WeaponType::Gun),
        3 => Some(WeaponType::Shotgun),
        4 => Some(WeaponType::Grenade),
        5 => Some(WeaponType::Laser),
        _ => return Err(anyhow!("invalid weapon index")),
    };
    let for_tick = u64::from_le_bytes(body[8..16].try_into().unwrap());
    Ok(AiInput {
        player_id: u64::from_le_bytes(body[0..8].try_into().unwrap()),
        for_tick: (for_tick != 0).then_some(for_tick),
        direction,
        jump: body[17] & 1 != 0,
        fire: body[17] & 2 != 0,
        hook: body[17] & 4 != 0,
        weapon,
        cursor: [
            f32::from_le_bytes(body[21..25].try_into().unwrap()) as f64,
            f32::from_le_bytes(body[25..29].try_into().unwrap()) as f64,
        ],
    })
}

fn split_frame(frame: &[u8]) -> Result<(u8, &[u8])> {
    if frame.len() < HEADER_LEN || &frame[0..4] != MAGIC || frame[4] != VERSION {
        return Err(anyhow!("invalid AI protocol frame"));
    }
    Ok((frame[5], &frame[HEADER_LEN..]))
}

fn expect_length(body: &[u8], expected: usize) -> Result<()> {
    (body.len() == expected).then_some(()).ok_or_else(|| {
        anyhow!(
            "invalid AI message length: expected {expected}, got {}",
            body.len()
        )
    })
}

fn encode_message(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER_LEN + body.len());
    frame.extend_from_slice(MAGIC);
    frame.push(VERSION);
    frame.push(kind);
    frame.extend_from_slice(body);
    frame
}

fn encode_ack(kind: u8, tick: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(9);
    body.push(kind);
    body.extend_from_slice(&tick.to_le_bytes());
    encode_message(ACK, &body)
}

fn encode_map(epoch: u64, map: &AiMap) -> Vec<u8> {
    let mut body = Vec::with_capacity(16 + map.tiles.len() * 10);
    body.extend_from_slice(&epoch.to_le_bytes());
    body.extend_from_slice(&map.width.to_le_bytes());
    body.extend_from_slice(&map.height.to_le_bytes());
    body.extend_from_slice(&(map.tiles.len() as u32).to_le_bytes());
    for tile in &map.tiles {
        body.extend_from_slice(&tile.x.to_le_bytes());
        body.extend_from_slice(&tile.y.to_le_bytes());
        body.push(tile.layer);
        body.push(tile.tile);
        body.extend_from_slice(&tile.value.to_le_bytes());
        body.extend_from_slice(&tile.aux.to_le_bytes());
    }
    encode_message(MAP, &body)
}

fn encode_state(map_epoch: u64, state: &AiTickState) -> Vec<u8> {
    let mut body = Vec::with_capacity(24 + state.objects.len() * 60);
    body.extend_from_slice(&state.tick.to_le_bytes());
    body.extend_from_slice(&map_epoch.to_le_bytes());
    body.extend_from_slice(&state.ticks_per_second.to_le_bytes());
    body.extend_from_slice(&(state.objects.len() as u32).to_le_bytes());
    for object in &state.objects {
        body.extend_from_slice(&object.object_id.to_le_bytes());
        body.push(object.kind);
        body.push(object.subtype);
        body.extend_from_slice(&object.properties.to_le_bytes());
        body.extend_from_slice(&object.state.to_le_bytes());
        body.extend_from_slice(&object.stage_id.to_le_bytes());
        for scalar in object
            .position
            .into_iter()
            .chain(object.velocity)
            .chain(object.size)
            .chain(object.scalars)
        {
            body.extend_from_slice(&scalar.to_le_bytes());
        }
    }
    encode_message(STATE, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };

    #[test]
    fn actor_reset_queues_one_maintenance_tick_and_clears_inputs() {
        let (bridge, handle) = ControlBridge::create();
        handle.set_tick_mode(ControlTickMode::Manual);
        let mut body = 12_u64.to_le_bytes().to_vec();
        body.push(1);
        body.extend_from_slice(&5.5_f32.to_le_bytes());
        body.extend_from_slice(&7.5_f32.to_le_bytes());
        let response = handle_command(&handle, &encode_message(RESET_ACTOR, &body)).unwrap();
        let (kind, ack) = split_frame(&response).unwrap();
        assert_eq!(kind, ACK);
        assert_eq!(ack[0], RESET_ACTOR);
        assert_eq!(u64::from_le_bytes(ack[1..].try_into().unwrap()), 1);
        let requests = bridge.take_actor_resets();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].1, Some([5.5, 7.5]));
        assert_eq!(handle.inner.gate.inner.lock().unwrap().permits, 1);
    }

    #[test]
    fn actor_reset_rejects_nonfinite_positions_without_releasing_ticks() {
        let (_, handle) = ControlBridge::create();
        let mut body = 12_u64.to_le_bytes().to_vec();
        body.push(1);
        body.extend_from_slice(&f32::NAN.to_le_bytes());
        body.extend_from_slice(&7.5_f32.to_le_bytes());
        assert!(handle_command(&handle, &encode_message(RESET_ACTOR, &body)).is_err());
        assert_eq!(handle.inner.gate.inner.lock().unwrap().permits, 0);
    }

    #[test]
    fn leaving_manual_mode_wakes_a_waiting_tick_gate() {
        let gate = Arc::new(TickGate::new());
        gate.set_mode(ControlTickMode::Manual);
        let waiter = gate.clone();
        let (sender, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            sender.send(waiter.wait_for_tick()).unwrap();
        });

        std::thread::sleep(Duration::from_millis(10));
        gate.set_mode(ControlTickMode::Realtime);
        let result = receiver.recv_timeout(Duration::from_secs(1)).ok();
        gate.close();
        thread.join().unwrap();

        assert_eq!(result, Some(true));
    }

    #[test]
    fn hook_click_uses_the_requested_cursor() {
        let (bridge, handle) = ControlBridge::create();
        handle
            .queue_input(AiInput {
                player_id: 1,
                for_tick: None,
                direction: 0,
                jump: false,
                fire: false,
                hook: true,
                weapon: None,
                cursor: [0.0, 10.0],
            })
            .unwrap();

        let input = bridge.take_inputs().pop().unwrap().input;
        let (_, cursor) = input.inp.consumable.diff(&Default::default()).hook.unwrap();
        let cursor = cursor.to_vec2();

        assert!(cursor.x.abs() < 0.001);
        assert!((cursor.y - 10.0).abs() < 0.001);
    }
}
