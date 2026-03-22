use crate::pid::PID;
use crate::varint::VarInt;
use std::io::Write;

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FractionalIndex<'a> {
    buf: &'a [u8],
}

impl<'a> FractionalIndex<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    #[inline]
    pub fn bytes(&self) -> &'a [u8] {
        self.buf
    }

    pub fn segments(&self) -> Segments<'a> {
        Segments { bytes: self.buf }
    }

    pub fn from_bytes(bytes: &'a [u8]) -> Option<(Self, usize)> {
        let mut i = 0;
        let mut b = bytes;
        while !b.is_empty() {
            // first try to read PID
            match crate::U32::read_from(b) {
                None => break,
                Some((_, n)) => {
                    i += n;
                    b = &b[n..];
                }
            }
            // try to read seq num second
            let (_, n) = u32::read_from(b)?;
            i += n;
            b = &b[n..];
        }
        Some((Self::new(&bytes[..i]), i))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Segment {
    pid: PID,
    seq: u32,
}

impl Segment {
    const MAX: Self = Segment::new(PID(crate::U32::MAX_VALUE), u32::MAX);

    pub const fn new(pid: PID, seq: u32) -> Self {
        Self { pid, seq }
    }

    /// Parses next segment from the sequence.
    pub fn parse(mut bytes: &[u8]) -> Option<(Self, &[u8])> {
        let (pid, n) = crate::U32::read_from(bytes)?;
        let pid = PID::new(pid)?;
        bytes = &bytes[n..];
        let (seq, n) = u32::read_from(bytes)?;
        bytes = &bytes[n..];
        Some((Segment { pid, seq }, bytes))
    }

    pub fn write<W: Write>(&self, w: &mut W) -> std::io::Result<()> {
        self.pid.0.write(w)?;
        self.seq.write(w)?;
        Ok(())
    }
}

pub struct Segments<'a> {
    bytes: &'a [u8],
}

impl<'a> Segments<'a> {
    pub fn new(bytes: &'a [u8]) -> Segments<'a> {
        Segments { bytes }
    }
}

impl<'a> Iterator for Segments<'a> {
    type Item = Segment;

    fn next(&mut self) -> Option<Self::Item> {
        let (segment, bytes) = Segment::parse(self.bytes)?;
        self.bytes = bytes;
        Some(segment)
    }
}

pub fn write_fractional_index<W: Write>(
    w: &mut W,
    lo: &[u8],
    hi: &[u8],
    pid: PID,
) -> std::io::Result<()> {
    let mut lo = Segments::new(lo);
    let mut hi = Segments::new(hi);

    let mut lower = lo.next();
    let mut higher = hi.next();
    let mut diffed = false;

    while let Some(l) = &lower
        && let Some(h) = &higher
    {
        let n = Segment::new(l.pid, l.seq + 1);
        if h > &n {
            if n.pid != pid {
                // segment peers differ, copy left and descent to next loop iteration
                l.write(w)?;
            } else {
                n.write(w)?;
                diffed = true;
                break;
            }
        } else {
            l.write(w)?;
        }

        lower = lo.next();
        higher = hi.next();
    }

    let min = Segment::new(pid, 0);
    while !diffed {
        let l = lower.take().unwrap_or(min);
        let h = higher.take().unwrap_or(Segment::MAX);
        let n = Segment::new(pid, l.seq + 1);
        if h > n {
            n.write(w)?;
            diffed = true;
        } else {
            l.write(w)?;
        }

        lower = lo.next();
        higher = hi.next();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(n: u32) -> PID {
        PID::new(n).unwrap()
    }

    fn seg(pid_val: u32, seq: u32) -> Segment {
        Segment::new(pid(pid_val), seq)
    }

    fn write_segments(segments: &[Segment]) -> Vec<u8> {
        let mut buf = Vec::new();
        for s in segments {
            s.write(&mut buf).unwrap();
        }
        buf
    }

    fn generate_between(lo: &[u8], hi: &[u8], p: PID) -> Vec<u8> {
        let mut result = Vec::new();
        write_fractional_index(&mut result, lo, hi, p).unwrap();
        result
    }

    fn assert_parses(bytes: &[u8]) {
        let (parsed, len) = FractionalIndex::from_bytes(bytes).unwrap();
        assert_eq!(len, bytes.len());
        assert_eq!(parsed.bytes(), bytes);
    }

    #[test]
    fn empty_boundaries() {
        let p = pid(1);
        let result = generate_between(&[], &[], p);

        assert!(!result.is_empty());
        assert_parses(&result);
    }

    #[test]
    fn lo_empty_hi_nonempty() {
        let p = pid(1);
        let hi = write_segments(&[seg(2, 5)]);

        let result = generate_between(&[], &hi, p);

        assert_parses(&result);
        assert!(result.as_slice() < hi.as_slice());
    }

    #[test]
    fn lo_nonempty_hi_empty() {
        let p = pid(1);
        let lo = write_segments(&[seg(2, 5)]);

        let result = generate_between(&lo, &[], p);

        assert_parses(&result);
        assert!(result.as_slice() > lo.as_slice());
    }

    #[test]
    fn same_pid_increments_seq_at_same_level() {
        let p = pid(1);
        let lo = write_segments(&[seg(1, 3)]);
        let hi = write_segments(&[seg(2, 10)]);

        let result = generate_between(&lo, &hi, p);

        assert_parses(&result);
        assert!(result.as_slice() > lo.as_slice());
        assert!(result.as_slice() < hi.as_slice());

        // should stay at the same level, not descend into a new segment
        let segments: Vec<_> = FractionalIndex::new(&result).segments().collect();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0], seg(1, 4));
    }

    #[test]
    fn lo_fewer_segments_than_hi() {
        let p = pid(1);
        let lo = write_segments(&[seg(2, 3)]);
        let hi = write_segments(&[seg(2, 3), seg(3, 5)]);

        let result = generate_between(&lo, &hi, p);

        assert_parses(&result);
        assert!(result.as_slice() > lo.as_slice());
        assert!(result.as_slice() < hi.as_slice());
    }

    #[test]
    fn hi_fewer_segments_than_lo() {
        let p = pid(1);
        let lo = write_segments(&[seg(2, 3), seg(3, 5)]);
        let hi = write_segments(&[seg(2, 5)]);

        let result = generate_between(&lo, &hi, p);

        assert_parses(&result);
        assert!(result.as_slice() > lo.as_slice());
        assert!(result.as_slice() < hi.as_slice());
    }
}
