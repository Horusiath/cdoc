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
        Some((Self::new(&b[..i]), i))
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
