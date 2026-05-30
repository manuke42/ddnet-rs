use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Hiarc, Default, Copy, Clone, Serialize, Deserialize)]
pub enum LaserType {
    #[default]
    Rifle,
    Puller,
    Door,
    Freeze,
}
