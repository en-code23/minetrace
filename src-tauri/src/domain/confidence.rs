use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Verified,
    High,
    Partial,
    Unknown,
}

impl Confidence {
    pub fn from_score(score: u8) -> Self {
        match score {
            80..=u8::MAX => Self::Verified,
            55..=79 => Self::High,
            30..=54 => Self::Partial,
            _ => Self::Unknown,
        }
    }
}
