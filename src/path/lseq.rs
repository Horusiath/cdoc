use std::io::Write;
use crate::pid::PID;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Segment {
    pid: PID,
    seq: u32,
}

impl Segment {
    /// Parses next segment from the sequence.
    pub fn parse(bytes: &[u8]) -> (Option<Self>, &[u8]) {
        todo!()
    }
}


pub const LOW_WATERMARK: u8 = 1;
pub const HIGH_WATERMARK: u8 = 255;

pub fn create_seq<W: Write>(w: &mut W, lo: &[u8], hi: &[u8]) -> std::io::Result<()> {
    todo!()
}