use crate::BE32;
use std::fmt::{Display, Formatter};
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
pub struct PID(pub(crate) crate::BE32);

impl PID {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let pid = *crate::BE32::ref_from_bytes(bytes).ok()?;
        if pid == BE32::new(0) {
            None
        } else {
            Some(PID(pid))
        }
    }

    pub fn new<C: Into<crate::BE32>>(pid: C) -> Option<Self> {
        let pid = pid.into();
        if pid == BE32::new(0) {
            None
        } else {
            Some(PID(pid))
        }
    }

    pub fn random() -> Self {
        PID(crate::BE32::new(fastrand::u32(1..)))
    }
}

impl Display for PID {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:x}", self.0)
    }
}
