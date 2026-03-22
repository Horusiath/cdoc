use crate::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Globally unique peer identifier. [PID] is stored locally and reused across the calls.
/// PID cannot be `0`.
#[repr(transparent)]
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Ord,
    PartialOrd,
    Hash,
    FromBytes,
    IntoBytes,
    Immutable,
    KnownLayout,
)]
pub struct PID(pub(crate) crate::U32);

impl PID {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let pid = *crate::U32::ref_from_bytes(bytes).ok()?;
        if pid == U32::new(0) {
            None
        } else {
            Some(PID(pid))
        }
    }

    pub fn new<C: Into<crate::U32>>(pid: C) -> Option<Self> {
        let pid = pid.into();
        if pid == U32::new(0) {
            None
        } else {
            Some(PID(pid))
        }
    }

    pub fn random() -> Self {
        PID(crate::U32::new(fastrand::u32(1..)))
    }
}
