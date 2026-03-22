use crate::path::lseq::FractionalIndex;
use crate::path::{CHUNKED, DELIMITER, Field, PathTail};
use crate::path::{PathError, PathSegment};
use crate::varint::VarInt;

pub struct PathReader<'a> {
    buf: &'a [u8],
}

impl<'a> PathReader<'a> {
    pub fn new(buf: &'a [u8]) -> PathReader<'a> {
        PathReader { buf }
    }

    fn is_delimiter(c: u8) -> bool {
        c < 32
    }

    fn read_field(&mut self) -> crate::Result<Field<'a>, PathError> {
        let mut i = 0;
        for byte in self.buf {
            if Self::is_delimiter(*byte) {
                break;
            }
            i += 1;
        }
        let (l, r) = self.buf.split_at(i);
        self.buf = r;
        Ok(unsafe { Field::new_unchecked(std::str::from_utf8_unchecked(l)) })
    }

    fn read_chunked(&mut self) -> crate::Result<PathTail, PathError> {
        match u64::read_from(self.buf) {
            None => Err(PathError::VarInt),
            Some((end_index, read)) => {
                self.buf = &self.buf[read..];
                Ok(PathTail::Chunked(end_index))
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
        if self.buf.is_empty() {
            return None;
        }

        let delim = self.buf[0];
        if Self::is_delimiter(delim) {
            match delim {
                DELIMITER => {
                    self.buf = &self.buf[1..];
                    Some(self.read_field().map(PathSegment::Field))
                }
                CHUNKED => {
                    self.buf = &self.buf[1..];
                    Some(self.read_chunked().map(PathSegment::Tail))
                }
                1..17 => Some(self.read_fractional_index().map(PathSegment::Index)),
                delim => Some(Err(PathError::Delimiter(delim))),
            }
        } else {
            Some(Err(PathError::Delimiter(delim)))
        }
    }
}
