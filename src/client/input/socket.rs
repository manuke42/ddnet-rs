use std::{
    fs,
    io::{self, Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use crossbeam::channel::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use input_binds::binds::{BindKey, MouseExtra};
use log::{debug, error, info, warn};
use native::native::{DeviceId, KeyCode, MouseButton, PhysicalKey};
use serde::Deserialize;
use serde_json;

use super::input_handling::{InputAxisMoveEv, InputControlCommand, InputEv, InputKeyEv};
use base::join_thread::JoinThread;

#[derive(Debug)]
pub struct InputSocketServer {
    path: PathBuf,
    shutdown: Arc<AtomicBool>,
    response_tx: Sender<String>,
    response_requests_rx: Receiver<()>,
    _thread: JoinThread<()>,
}

impl InputSocketServer {
    pub fn start<P: AsRef<Path>>(path: P, sender: Sender<InputEv>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if path.as_os_str().is_empty() {
            return Err(anyhow!("input socket path must not be empty"));
        }

        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create parent directory for input socket at {}",
                        parent.display()
                    )
                })?;
            }
        }

        if let Err(err) = fs::remove_file(&path) {
            if err.kind() != io::ErrorKind::NotFound {
                warn!(
                    "failed to remove stale input socket at {}: {err}",
                    path.display()
                );
            }
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();
        let thread_path = path.clone();
        let thread_sender = sender.clone();
        let (response_tx, response_rx) = crossbeam::channel::unbounded();
        let (response_request_tx, response_request_rx) = crossbeam::channel::unbounded();
        let handle = std::thread::Builder::new()
            .name("unix-input-socket".to_string())
            .spawn(move || {
                run_socket_loop(
                    thread_path,
                    thread_sender,
                    response_rx,
                    response_request_tx,
                    thread_shutdown,
                )
            })
            .context("failed to spawn unix input socket thread")?;

        Ok(Self {
            path,
            shutdown,
            response_tx,
            response_requests_rx: response_request_rx,
            _thread: JoinThread::new(handle),
        })
    }

    pub fn response_sender(&self) -> Sender<String> {
        self.response_tx.clone()
    }

    pub fn take_pending_responses(&self) -> usize {
        let mut count = 0;
        while self.response_requests_rx.try_recv().is_ok() {
            count += 1;
        }
        count
    }
}

impl Drop for InputSocketServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = UnixStream::connect(&self.path);
        if let Err(err) = fs::remove_file(&self.path) {
            if err.kind() != io::ErrorKind::NotFound {
                warn!(
                    "failed to remove input socket at {} during shutdown: {err}",
                    self.path.display()
                );
            }
        }
    }
}

fn run_socket_loop(
    path: PathBuf,
    sender: Sender<InputEv>,
    response_rx: Receiver<String>,
    response_request_tx: Sender<()>,
    shutdown: Arc<AtomicBool>,
) {
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        Err(err) => {
            error!("failed to bind input socket at {}: {err}", path.display());
            return;
        }
    };

    info!("input socket listening at {}", path.display());

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                debug!("input socket client connected");
                drain_responses(&response_rx);
                if let Err(err) = handle_connection(
                    stream,
                    sender.clone(),
                    &response_rx,
                    &response_request_tx,
                    &shutdown,
                ) {
                    warn!("error while handling input socket client: {err}");
                }
                debug!("input socket client disconnected");
            }
            Err(err) => {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                warn!("input socket accept error: {err}");
            }
        }
    }

    info!("input socket at {} shutting down", path.display());
}

fn handle_connection(
    mut stream: UnixStream,
    sender: Sender<InputEv>,
    response_rx: &Receiver<String>,
    response_request_tx: &Sender<()>,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .context("failed to configure input socket read timeout")?;

    let mut buffer = Vec::new();
    let mut temp = [0u8; 2048];

    while !shutdown.load(Ordering::Relaxed) {
        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(count) => {
                buffer.extend_from_slice(&temp[..count]);
                consume_buffer(
                    &mut buffer,
                    &sender,
                    response_rx,
                    response_request_tx,
                    shutdown,
                    &mut stream,
                )?;
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(err) if err.kind() == io::ErrorKind::TimedOut => {
                continue;
            }
            Err(err) => return Err(err).context("error while reading from input socket"),
        }
    }

    Ok(())
}

fn consume_buffer(
    buffer: &mut Vec<u8>,
    sender: &Sender<InputEv>,
    response_rx: &Receiver<String>,
    response_request_tx: &Sender<()>,
    shutdown: &Arc<AtomicBool>,
    stream: &mut UnixStream,
) -> Result<()> {
    loop {
        let Some(pos) = buffer.iter().position(|b| *b == b'\n') else {
            break;
        };
        let line = buffer.drain(..=pos).collect::<Vec<_>>();
        let payload = &line[..line.len().saturating_sub(1)];
        if payload.is_empty() {
            continue;
        }
        let text = match std::str::from_utf8(payload) {
            Ok(text) => text,
            Err(err) => {
                warn!("input socket: ignoring non-UTF8 payload ({err})");
                continue;
            }
        };
        let processed = process_payload(text, sender)?;
        if processed {
            if let Err(err) = response_request_tx.send(()) {
                return Err(anyhow!(
                    "failed to queue input socket response request: {err}"
                ));
            }
            match wait_for_response(response_rx, shutdown)? {
                Some(response) => send_response(stream, response)?,
                None => break,
            }
        }
    }
    Ok(())
}

fn process_payload(text: &str, sender: &Sender<InputEv>) -> Result<bool> {
    let msg: SocketEvent = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(err) => {
            warn!("input socket: ignoring malformed payload `{text}` ({err})");
            return Ok(false);
        }
    };
    dispatch_event(msg, sender)
}

fn dispatch_event(event: SocketEvent, sender: &Sender<InputEv>) -> Result<bool> {
    let device = DeviceId::dummy();
    let maybe_event = match event {
        SocketEvent::Key { code, state } => Some(InputEv::Key(InputKeyEv {
            key: BindKey::Key(PhysicalKey::Code(code)),
            is_down: state.is_pressed(),
            device,
        })),
        SocketEvent::MouseButton { button, state } => Some(InputEv::Key(InputKeyEv {
            key: BindKey::Mouse(button),
            is_down: state.is_pressed(),
            device,
        })),
        SocketEvent::MouseMove { dx, dy } => Some(InputEv::Move(InputAxisMoveEv {
            device,
            xrel: dx,
            yrel: dy,
        })),
        SocketEvent::Scroll { delta } => {
            if delta == 0.0 {
                None
            } else {
                let key = if delta < 0.0 {
                    BindKey::Extra(MouseExtra::WheelDown)
                } else {
                    BindKey::Extra(MouseExtra::WheelUp)
                };
                Some(InputEv::Key(InputKeyEv {
                    key,
                    is_down: false,
                    device,
                }))
            }
        }
        SocketEvent::InputEnd { ticks } => {
            Some(InputEv::Control(InputControlCommand::FlushInputs { ticks }))
        }
    };

    if let Some(event) = maybe_event {
        sender
            .send(event)
            .context("failed to forward input socket event")?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn wait_for_response(
    response_rx: &Receiver<String>,
    shutdown: &Arc<AtomicBool>,
) -> Result<Option<String>> {
    loop {
        if shutdown.load(Ordering::Relaxed) {
            return Ok(None);
        }
        match response_rx.recv_timeout(Duration::from_millis(50)) {
            Ok(response) => return Ok(Some(response)),
            Err(RecvTimeoutError::Timeout) => {
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow!("input socket response channel disconnected"));
            }
        }
    }
}

fn send_response(stream: &mut UnixStream, response: String) -> Result<()> {
    let mut data = response.into_bytes();
    data.push(b'\n');
    stream
        .write_all(&data)
        .context("failed to write response to input socket client")?;
    stream
        .flush()
        .context("failed to flush input socket response to client")
}

fn drain_responses(response_rx: &Receiver<String>) {
    loop {
        match response_rx.try_recv() {
            Ok(_) => {}
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SocketEvent {
    Key {
        code: KeyCode,
        state: SocketButtonState,
    },
    MouseButton {
        button: MouseButton,
        state: SocketButtonState,
    },
    MouseMove {
        dx: f64,
        dy: f64,
    },
    Scroll {
        delta: f64,
    },
    InputEnd {
        #[serde(default)]
        ticks: Option<u32>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SocketButtonState {
    Pressed,
    #[serde(alias = "down")]
    Down,
    Released,
    #[serde(alias = "up")]
    Up,
}

impl SocketButtonState {
    fn is_pressed(&self) -> bool {
        matches!(self, SocketButtonState::Pressed | SocketButtonState::Down)
    }
}
