use crate::PID;
use crate::path::lseq::FractionalIndex;
use crate::path::{
    DELIMITER, Field, TERMINATOR_CHUNKED, TERMINATOR_COUNTER, TERMINATOR_LWW, Terminator,
};
use crate::path::{PathError, PathSegment};
use crate::varint::VarInt;

pub struct PathReader<'a> {
    buf: &'a [u8],
}

impl<'a> PathReader<'a> {
    pub fn new(buf: &'a [u8]) -> PathReader<'a> {
        PathReader { buf }
    }

    fn read_field(&mut self) -> crate::Result<Field<'a>, PathError> {
        let mut i = 0;
        for byte in self.buf {
            if *byte == DELIMITER {
                break;
            }
            i += 1;
        }
        let (l, r) = self.buf.split_at(i);
        self.buf = r;
        Ok(unsafe { Field::new_unchecked(std::str::from_utf8_unchecked(l)) })
    }

    fn read_chunked(&mut self) -> crate::Result<Terminator, PathError> {
        match u64::read_from(self.buf) {
            None => Err(PathError::VarInt),
            Some((end_index, read)) => {
                self.buf = &self.buf[read..];
                Ok(Terminator::Chunked(end_index))
            }
        }
    }

    fn read_counter(&mut self) -> crate::Result<Terminator, PathError> {
        use zerocopy::FromBytes;
        match PID::ref_from_bytes(self.buf) {
            Err(_) => Err(PathError::VarInt),
            Ok(&pid) => {
                self.buf = &[]; // end of the path
                Ok(Terminator::Counter(pid))
            }
        }
    }

    fn read_fractional_index(&mut self) -> crate::Result<FractionalIndex<'a>, PathError> {
        match FractionalIndex::from_bytes(&self.buf) {
            None => Err(PathError::InvalidIndex),
            Some((index, read)) => {
                self.buf = &self.buf[read..];
                Ok(index)
            }
        }
    }
}

impl<'a> Iterator for PathReader<'a> {
    type Item = crate::Result<PathSegment<'a>, PathError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buf.len() < 2 {
            return None;
        }

        let delim = self.buf[0];
        if delim == DELIMITER {
            self.buf = &self.buf[1..];
            let delim = self.buf[0];
            match delim {
                TERMINATOR_CHUNKED => {
                    self.buf = &self.buf[1..];
                    Some(self.read_chunked().map(PathSegment::Tail))
                }
                TERMINATOR_LWW => {
                    self.buf = &[]; // end of the path
                    Some(Ok(PathSegment::Tail(Terminator::LWW)))
                }
                TERMINATOR_COUNTER => {
                    self.buf = &self.buf[1..];
                    Some(self.read_counter().map(PathSegment::Tail))
                }
                1..17 => Some(self.read_fractional_index().map(PathSegment::Index)),
                17..32 => Some(Err(PathError::Delimiter(delim))),
                _ => Some(self.read_field().map(PathSegment::Field)),
            }
        } else {
            Some(Err(PathError::Delimiter(delim)))
        }
    }
}
