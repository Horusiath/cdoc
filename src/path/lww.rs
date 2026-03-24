use crate::hlc::Timestamp;
use crate::pid::PID;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Last Write Wins register header. Used to compare values that are subject to merge.
#[repr(C, packed)]
#[derive(
    Debug, Clone, Ord, PartialOrd, Eq, PartialEq, FromBytes, IntoBytes, Immutable, KnownLayout,
)]
pub struct LWWHeader {
    timestamp: Timestamp,
    pid: PID,
}

impl LWWHeader {
    pub const SIZE: usize = size_of::<LWWHeader>();

    #[inline]
    pub fn new(timestamp: Timestamp, pid: PID) -> Self {
        Self { timestamp, pid }
    }

    /// Returns a hybrid logical timestamp of the current register.
    #[inline]
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Returns a PID of a last peer, which updated current register.
    #[inline]
    pub fn pid(&self) -> PID {
        self.pid
    }

    /// Merges two registers together. Each register prefix must be a valid [LWWHeader], followed
    /// by the register value.
    ///
    /// Returns a value which is logically higher - comparison is made by timestamp, then (if both
    /// timestamps are equal) by pid value.
    pub fn merge<'a>(left: &'a [u8], right: &'a [u8]) -> crate::Result<&'a [u8]> {
        let (left_header, _) = Self::ref_from_prefix(left)?;
        let (right_header, _) = Self::ref_from_prefix(right)?;
        if left_header > right_header {
            Ok(left)
        } else {
            Ok(right)
        }
    }
}

#[cfg(test)]
mod tests {}
