use anyhow::{Context, Result, anyhow};
use clap::Parser;
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

mod network;
use network::NetworkClient;

#[derive(Parser, Debug)]
#[command(name = "terminal-client")]
#[command(about = "DDNet Terminal Client - Deterministic tick-controlled client", long_about = None)]
struct Args {
    /// Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:8303")]
    server: String,

    /// Path to Unix socket for input
    #[arg(short, long, default_value = "/tmp/ddnet-input.sock")]
    input_socket: PathBuf,

    /// Path to Unix socket for frame output
    #[arg(short, long, default_value = "/tmp/ddnet-frames.sock")]
    frame_socket: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputEvent {
    Key { code: String, state: String },
    MouseButton { button: String, state: String },
    MouseMove { dx: f64, dy: f64 },
    Scroll { delta: f64 },
    InputEnd,
}

#[derive(Debug, Serialize)]
struct PhaseResponse {
    phase: String,
    tick: u64,
    player_x: f64,
}

/// Terminal client main structure
pub struct TerminalClient {
    input_socket_path: PathBuf,
    frame_socket_path: PathBuf,
    network_client: NetworkClient,
}

impl TerminalClient {
    pub fn new(
        server_addr: String,
        input_socket_path: PathBuf,
        frame_socket_path: PathBuf,
    ) -> Result<Self> {
        let network_client = NetworkClient::new(server_addr)?;
        
        Ok(Self {
            input_socket_path,
            frame_socket_path,
            network_client,
        })
    }

    /// Run the terminal client main loop
    pub async fn run(&mut self) -> Result<()> {
        info!("Terminal client starting...");

        // Start network client connection
        self.network_client.connect().await?;
        
        // Wait for player to spawn (AwaitReady phase)
        info!("Waiting for player to spawn...");
        self.network_client.wait_for_ready().await?;
        info!("Player spawned, entering Input phase");

        // Main tick control loop
        loop {
            // PHASE 1: Input phase - wait for input via Unix socket
            let inputs = self.input_phase().await?;
            
            // PHASE 2: Step phase - send inputs to server and wait for it to process one tick
            self.step_phase(inputs).await?;
            
            // PHASE 3: Output phase - receive snapshot, render, and send frame
            self.output_phase().await?;
        }
    }

    /// Input phase: Wait for inputs from Unix socket until input_end received
    async fn input_phase(&mut self) -> Result<Vec<InputEvent>> {
        info!("Entering Input phase");
        
        // Create Unix socket listener
        let _ = std::fs::remove_file(&self.input_socket_path);
        let listener = UnixListener::bind(&self.input_socket_path)
            .context("Failed to bind input socket")?;
        info!("Listening for input on {}", self.input_socket_path.display());

        // Accept one connection
        let (stream, _) = listener.accept().await?;
        let mut reader = BufReader::new(stream);
        let mut inputs = Vec::new();
        let mut line = String::new();

        // Read inputs until input_end
        loop {
            line.clear();
            let n = reader.read_line(&mut line).await?;
            
            if n == 0 {
                return Err(anyhow!("Input socket closed before input_end"));
            }

            let event: InputEvent = serde_json::from_str(line.trim())
                .context("Failed to parse input event")?;
            
            match event {
                InputEvent::InputEnd => {
                    info!("Received input_end, collected {} inputs", inputs.len());
                    
                    // Send response back to socket
                    let response = PhaseResponse {
                        phase: "input".to_string(),
                        tick: self.network_client.current_tick(),
                        player_x: self.network_client.player_x(),
                    };
                    let response_json = serde_json::to_string(&response)? + "\n";
                    reader.get_mut().write_all(response_json.as_bytes()).await?;
                    
                    break;
                }
                _ => {
                    inputs.push(event);
                }
            }
        }

        Ok(inputs)
    }

    /// Step phase: Send inputs to server and wait for it to process one tick
    async fn step_phase(&mut self, inputs: Vec<InputEvent>) -> Result<()> {
        info!("Entering Step phase with {} inputs", inputs.len());
        
        // Convert inputs to game input format and send to server
        self.network_client.send_inputs(inputs).await?;
        
        // Server will now process exactly one tick
        info!("Inputs sent, waiting for server to process tick...");
        
        Ok(())
    }

    /// Output phase: Receive snapshot from server, render, and send frame to socket
    async fn output_phase(&mut self) -> Result<()> {
        info!("Entering Output phase");
        
        // Receive snapshot from server
        let snapshot = self.network_client.receive_snapshot().await?;
        info!("Received snapshot for tick {}", snapshot.tick);
        
        // Render the world (simplified for terminal client)
        let frame_data = self.render_snapshot(&snapshot)?;
        
        // Send frame to output socket
        self.send_frame(frame_data).await?;
        
        info!("Output phase complete, returning to Input phase");
        Ok(())
    }

    /// Simplified rendering for terminal client
    fn render_snapshot(&self, snapshot: &network::Snapshot) -> Result<Vec<u8>> {
        // For a terminal client, we create a minimal "frame" representation
        // In a full implementation, this would render the game world
        
        let frame_info = format!(
            "{{\"tick\":{},\"player_x\":{},\"player_y\":{}}}\n",
            snapshot.tick,
            snapshot.player_x,
            snapshot.player_y
        );
        
        Ok(frame_info.into_bytes())
    }

    /// Send rendered frame to frame output socket
    async fn send_frame(&self, frame_data: Vec<u8>) -> Result<()> {
        // Connect to frame socket and send data
        match tokio::net::UnixStream::connect(&self.frame_socket_path).await {
            Ok(mut stream) => {
                stream.write_all(&frame_data).await?;
                info!("Frame sent to {}", self.frame_socket_path.display());
                Ok(())
            }
            Err(e) => {
                warn!("Could not connect to frame socket: {}, frame not sent", e);
                Ok(()) // Don't fail if frame socket isn't available
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    let args = Args::parse();
    
    info!("DDNet Terminal Client");
    info!("Server: {}", args.server);
    info!("Input socket: {}", args.input_socket.display());
    info!("Frame socket: {}", args.frame_socket.display());
    
    let mut client = TerminalClient::new(
        args.server,
        args.input_socket,
        args.frame_socket,
    )?;
    
    match client.run().await {
        Ok(_) => {
            info!("Terminal client shut down gracefully");
            Ok(())
        }
        Err(e) => {
            error!("Terminal client error: {}", e);
            Err(e)
        }
    }
}
