use std::fmt::{Display, Formatter};
use serde::{Deserialize, Serialize};

/// A hybrid logical timestamp. It's based on UNIX milliseconds timestamp, but the last 16bits are
/// assigned from monotonically increasing sequencer.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Timestamp(u64);

impl Timestamp {
    pub fn now() -> Self {
        todo!()
    }

    pub fn sync(timestamp: Self) -> Self {
        todo!()
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        todo!("write human-readable ISO 8601 formatted timestamp string")
    }
}