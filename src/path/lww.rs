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
mod tests {
    use super::*;
    use zerocopy::IntoBytes;

    fn register(ts: u64, pid: u32, value: &[u8]) -> Vec<u8> {
        let header = LWWHeader::new(Timestamp::new(ts), PID::new(pid).unwrap());
        let mut buf = Vec::with_capacity(LWWHeader::SIZE + value.len());
        buf.extend_from_slice(header.as_bytes());
        buf.extend_from_slice(value);
        buf
    }

    #[test]
    fn merge_returns_register_with_higher_timestamp() {
        let left = register(10, 1, b"old");
        let right = register(20, 1, b"new");
        let result = LWWHeader::merge(&left, &right).unwrap();
        assert!(std::ptr::eq(result, right.as_slice()));
    }

    #[test]
    fn merge_returns_register_with_higher_timestamp_reversed() {
        let left = register(20, 1, b"new");
        let right = register(10, 1, b"old");
        let result = LWWHeader::merge(&left, &right).unwrap();
        assert!(std::ptr::eq(result, left.as_slice()));
    }

    #[test]
    fn merge_uses_pid_as_tiebreaker_when_timestamps_equal() {
        let left = register(10, 1, b"lo-pid");
        let right = register(10, 2, b"hi-pid");
        let result = LWWHeader::merge(&left, &right).unwrap();
        assert!(std::ptr::eq(result, right.as_slice()));
    }

    #[test]
    fn merge_uses_pid_as_tiebreaker_reversed() {
        let left = register(10, 2, b"hi-pid");
        let right = register(10, 1, b"lo-pid");
        let result = LWWHeader::merge(&left, &right).unwrap();
        assert!(std::ptr::eq(result, left.as_slice()));
    }
}
