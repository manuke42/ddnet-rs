use std::{
    fs,
    io::{self, Read},
    os::unix::net::{UnixListener, UnixStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use crossbeam::channel::Sender;
use input_binds::binds::{BindKey, MouseExtra};
use log::{debug, error, info, warn};
use native::native::{DeviceId, KeyCode, MouseButton, PhysicalKey};
use serde::Deserialize;
use serde_json;

use super::input_handling::{InputAxisMoveEv, InputEv, InputKeyEv};
use base::join_thread::JoinThread;

#[derive(Debug)]
pub struct InputSocketServer {
    path: PathBuf,
    shutdown: Arc<AtomicBool>,
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
                warn!("failed to remove stale input socket at {}: {err}", path.display());
            }
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = shutdown.clone();
        let thread_path = path.clone();
        let thread_sender = sender.clone();
        let handle = std::thread::Builder::new()
            .name("unix-input-socket".to_string())
            .spawn(move || run_socket_loop(thread_path, thread_sender, thread_shutdown))
            .context("failed to spawn unix input socket thread")?;

        Ok(Self {
            path,
            shutdown,
            _thread: JoinThread::new(handle),
        })
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

fn run_socket_loop(path: PathBuf, sender: Sender<InputEv>, shutdown: Arc<AtomicBool>) {
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
                if let Err(err) = handle_connection(stream, sender.clone(), &shutdown) {
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
                consume_buffer(&mut buffer, &sender)?;
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

fn consume_buffer(buffer: &mut Vec<u8>, sender: &Sender<InputEv>) -> Result<()> {
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
        if let Err(err) = process_payload(text, sender) {
            return Err(err);
        }
    }
    Ok(())
}

fn process_payload(text: &str, sender: &Sender<InputEv>) -> Result<()> {
    let msg: SocketEvent = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(err) => {
            warn!("input socket: ignoring malformed payload `{text}` ({err})");
            return Ok(());
        }
    };
    dispatch_event(msg, sender)
}

fn dispatch_event(event: SocketEvent, sender: &Sender<InputEv>) -> Result<()> {
    let device = DeviceId::dummy();
    let event = match event {
        SocketEvent::Key { code, state } => InputEv::Key(InputKeyEv {
            key: BindKey::Key(PhysicalKey::Code(code)),
            is_down: state.is_pressed(),
            device,
        }),
        SocketEvent::MouseButton { button, state } => InputEv::Key(InputKeyEv {
            key: BindKey::Mouse(button),
            is_down: state.is_pressed(),
            device,
        }),
        SocketEvent::MouseMove { dx, dy } => InputEv::Move(InputAxisMoveEv {
            device,
            xrel: dx,
            yrel: dy,
        }),
        SocketEvent::Scroll { delta } => {
            if delta == 0.0 {
                return Ok(());
            }
            let key = if delta < 0.0 {
                BindKey::Extra(MouseExtra::WheelDown)
            } else {
                BindKey::Extra(MouseExtra::WheelUp)
            };
            InputEv::Key(InputKeyEv {
                key,
                is_down: false,
                device,
            })
        }
    };

    sender
        .send(event)
        .context("failed to forward input socket event")
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
