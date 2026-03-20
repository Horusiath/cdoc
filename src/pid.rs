use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Globally unique peer identifier. [PID] is stored locally and reused across the calls.
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct PID(crate::U32);

impl PID {
    pub fn random() -> Self {
        PID(crate::U32::new(fastrand::u32(1..)))
    }
}