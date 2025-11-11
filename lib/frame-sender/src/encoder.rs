use std::{
    convert::TryFrom,
    fmt::Debug,
    io::{BufWriter, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use anyhow::Context;
use base::join_thread::JoinThread;
use graphics_backend::backend::GraphicsBackend;
use graphics_backend_traits::{
    frame_fetcher_plugin::{
        BackendFrameFetcher, BackendPresentedImageDataRgba, FetchCanvasError, FetchCanvasIndex,
        OffscreenCanvasId,
    },
    traits::GraphicsBackendInterface,
};
use hiarc::{Hiarc, hiarc_safer_arc_mutex};
use log::{error, info, warn};
use pool::mt_datatypes::PoolUnclearedVec;
use sound::backend_types::SoundBackendInterface;
use sound::frame_fetcher_plugin::{
    self, BackendAudioFrame, FetchSoundManagerError, FetchSoundManagerIndex, OffairSoundManagerId,
};
use sound_backend::sound_backend::SoundBackend;
use std::os::unix::net::UnixStream;

use crate::{traits::AudioVideoEncoder, types::EncoderSettings};

const PROTOCOL_VERSION: u16 = 1;
const MESSAGE_KIND_FRAME: u16 = 1;
const HEADER_MAGIC: &[u8; 4] = b"DDNF";
const FRAME_FORMAT: &[u8; 4] = b"RGBA";
const HEADER_SIZE: usize = 32;

#[derive(Debug)]
struct FramePacket {
    index: u64,
    width: u32,
    height: u32,
    pixels: PoolUnclearedVec<u8>,
}

#[hiarc_safer_arc_mutex]
#[derive(Debug, Hiarc)]
pub struct AudioVideoEncoderImpl {
    #[hiarc_skip_unsafe]
    video_sender: mpsc::SyncSender<FramePacket>,
    cur_video_frame: u64,
    _video_frame_buffer_id: OffscreenCanvasId,

    cur_audio_frame: u64,
    _audio_frame_buffer_id: OffairSoundManagerId,

    video_frames_in_queue: Arc<AtomicU64>,
    max_video_frames_in_queue: u64,

    _backend_data: PhantomData<BackendPresentedImageDataRgba>,
    _sender_thread: JoinThread<()>,
}

#[hiarc_safer_arc_mutex]
impl AudioVideoEncoderImpl {
    pub fn new(
        video_frame_buffer_id: OffscreenCanvasId,
        audio_frame_buffer_id: OffairSoundManagerId,
        file_path: &Path,
        encoder_settings: EncoderSettings,
    ) -> anyhow::Result<Self> {
        let max_video_frames_in_queue = encoder_settings.max_threads.max(1);
        let channel_capacity = usize::try_from(max_video_frames_in_queue)
            .unwrap_or(usize::MAX / 2)
            .saturating_mul(2)
            .max(1);
        let (video_sender, video_receiver) = mpsc::sync_channel::<FramePacket>(channel_capacity);

        let video_frames_in_queue = Arc::new(AtomicU64::new(0));
        let socket_path = file_path.to_path_buf();
        let thread_frames_in_queue = video_frames_in_queue.clone();
        let thread_settings = encoder_settings.clone();

        let sender_thread = std::thread::Builder::new()
            .name("frame-socket-sender".to_string())
            .spawn(move || {
                run_sender_loop(
                    socket_path,
                    thread_settings,
                    video_receiver,
                    thread_frames_in_queue,
                );
            })
            .context("failed to spawn frame sender thread")?;

        Ok(Self {
            video_sender,
            cur_video_frame: 0,
            _video_frame_buffer_id: video_frame_buffer_id,
            cur_audio_frame: 0,
            _audio_frame_buffer_id: audio_frame_buffer_id,
            video_frames_in_queue,
            max_video_frames_in_queue,

            _backend_data: PhantomData,
            _sender_thread: JoinThread::new(sender_thread),
        })
    }
    pub fn overloaded(&self) -> bool {
        self.video_frames_in_queue.load(Ordering::Relaxed) >= self.max_video_frames_in_queue
    }
}

fn run_sender_loop(
    socket_path: PathBuf,
    settings: EncoderSettings,
    receiver: mpsc::Receiver<FramePacket>,
    frames_in_queue: Arc<AtomicU64>,
) {
    let stream = match UnixStream::connect(&socket_path) {
        Ok(stream) => stream,
        Err(err) => {
            error!(
                "failed to connect to frame receiver at {:?}: {}",
                socket_path, err
            );
            drain_receiver(&receiver, &frames_in_queue);
            return;
        }
    };

    let mut writer = BufWriter::with_capacity(4 * 1024 * 1024, stream);

    if let Err(err) = send_stream_init(&mut writer, &settings) {
        error!(
            "failed to send frame stream init to {:?}: {}",
            socket_path, err
        );
        drain_receiver(&receiver, &frames_in_queue);
        return;
    }

    info!("frame sender connected to {:?}", socket_path);

    while let Ok(packet) = receiver.recv() {
        let send_result = send_frame_packet(&mut writer, &packet);
        decrement_counter(&frames_in_queue);

        if let Err(err) = send_result {
            error!(
                "frame sender connection at {:?} encountered an error: {}",
                socket_path, err
            );
            drain_receiver(&receiver, &frames_in_queue);
            break;
        }
    }
}

fn send_stream_init(
    writer: &mut BufWriter<UnixStream>,
    settings: &EncoderSettings,
) -> std::io::Result<()> {
    let handshake = format!(
        "{{\"width\":{},\"height\":{},\"fps\":{},\"format\":\"RGBA\",\"bytes_per_pixel\":4,\"sample_rate\":{}}}\n",
        settings.width, settings.height, settings.fps, settings.sample_rate
    );
    writer.write_all(handshake.as_bytes())?;
    writer.flush()
}

fn send_frame_packet(
    writer: &mut BufWriter<UnixStream>,
    packet: &FramePacket,
) -> std::io::Result<()> {
    let expected_len = packet.width.saturating_mul(packet.height).saturating_mul(4) as usize;
    if expected_len != packet.pixels.len() {
        warn!(
            "frame {} has unexpected buffer size: expected {}, got {}",
            packet.index,
            expected_len,
            packet.pixels.len()
        );
    }

    let payload_len = u32::try_from(packet.pixels.len()).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame payload exceeds protocol limit",
        )
    })?;

    let mut header = [0u8; HEADER_SIZE];
    header[0..4].copy_from_slice(HEADER_MAGIC);
    header[4..6].copy_from_slice(&PROTOCOL_VERSION.to_be_bytes());
    header[6..8].copy_from_slice(&MESSAGE_KIND_FRAME.to_be_bytes());
    header[8..12].copy_from_slice(&packet.width.to_be_bytes());
    header[12..16].copy_from_slice(&packet.height.to_be_bytes());
    header[16..24].copy_from_slice(&packet.index.to_be_bytes());
    header[24..28].copy_from_slice(&payload_len.to_be_bytes());
    header[28..32].copy_from_slice(FRAME_FORMAT);

    writer.write_all(&header)?;
    writer.write_all(&packet.pixels[..])?;
    writer.flush()
}

fn drain_receiver(receiver: &mpsc::Receiver<FramePacket>, frames_in_queue: &AtomicU64) {
    while let Ok(packet) = receiver.try_recv() {
        let _ = packet;
        decrement_counter(frames_in_queue);
    }
}

fn decrement_counter(counter: &AtomicU64) {
    let mut current = counter.load(Ordering::Relaxed);
    while current != 0 {
        match counter.compare_exchange_weak(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

#[hiarc_safer_arc_mutex]
impl BackendFrameFetcher for AudioVideoEncoderImpl {
    #[hiarc_trait_is_immutable_self]
    fn next_frame(&mut self, frame_data: BackendPresentedImageDataRgba) {
        let packet = FramePacket {
            index: self.cur_video_frame,
            width: frame_data.width,
            height: frame_data.height,
            pixels: frame_data.dest_data_buffer,
        };

        match self.video_sender.send(packet) {
            Ok(()) => {
                self.cur_video_frame += 1;
                self.video_frames_in_queue.fetch_add(1, Ordering::Relaxed);
            }
            Err(err) => {
                self.video_frames_in_queue.store(0, Ordering::Relaxed);
                warn!("dropping video frame {}: {}", self.cur_video_frame, err);
            }
        }
    }

    fn current_fetch_index(&self) -> FetchCanvasIndex {
        FetchCanvasIndex::Onscreen
    }

    fn fetch_err(&self, err: FetchCanvasError) {
        match err {
            FetchCanvasError::CanvasNotFound => {}
            FetchCanvasError::DriverErr(err) => {
                panic!("err in video frame sending: {err}");
            }
        }
    }
}

#[hiarc_safer_arc_mutex]
impl frame_fetcher_plugin::BackendFrameFetcher for AudioVideoEncoderImpl {
    #[hiarc_trait_is_immutable_self]
    fn next_frame(&mut self, _frame_data: BackendAudioFrame) {
        self.cur_audio_frame += 1;
    }

    fn current_fetch_index(&self) -> FetchSoundManagerIndex {
        FetchSoundManagerIndex::Onair
    }

    fn fetch_err(&self, err: FetchSoundManagerError) {
        match err {
            FetchSoundManagerError::SoundManagerNotFound => {}
            FetchSoundManagerError::DriverErr(err) => {
                panic!("err in audio frame sending: {err}");
            }
        }
    }
}

pub struct FfmpegEncoder {
    backend: Rc<GraphicsBackend>,
    sound_backend: Rc<SoundBackend>,
    encoder: Arc<AudioVideoEncoderImpl>,
}

impl AudioVideoEncoder for FfmpegEncoder {
    fn new(
        video_frame_buffer_id: OffscreenCanvasId,
        audio_frame_buffer_id: OffairSoundManagerId,
        file_path: &Path,
        backend: &Rc<GraphicsBackend>,
        sound_backend: &Rc<SoundBackend>,
        encoder_settings: EncoderSettings,
    ) -> anyhow::Result<Self> {
        let encoder = Arc::new(AudioVideoEncoderImpl::new(
            video_frame_buffer_id,
            audio_frame_buffer_id,
            file_path,
            encoder_settings,
        )?);

        backend.attach_frame_fetcher("av-encoder".into(), encoder.clone())?;
        sound_backend.attach_frame_fetcher("av-encoder".into(), encoder.clone())?;

        Ok(Self {
            backend: backend.clone(),
            sound_backend: sound_backend.clone(),
            encoder,
        })
    }

    fn overloaded(&self) -> bool {
        self.encoder.overloaded()
    }
}

impl Drop for FfmpegEncoder {
    fn drop(&mut self) {
        let _ = self.backend.detach_frame_fetcher("av-encoder".into());
        let _ = self.sound_backend.detach_frame_fetcher("av-encoder".into());
    }
}
