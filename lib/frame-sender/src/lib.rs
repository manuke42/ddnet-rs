#[cfg(unix)]
pub mod encoder;
pub mod traits;
pub mod types;

pub mod stub;

#[cfg(unix)]
pub type AvEncoder = encoder::FfmpegEncoder;
#[cfg(not(unix))]
pub type AvEncoder = stub::StubEncoder;
