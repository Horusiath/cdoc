use crate::path::lseq::FractionalIndex;
use std::ops::Deref;

pub mod lseq;
mod read;
mod write;

const DELIMITER: u8 = 0;
const CHUNKED: u8 = 0b11111;

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("unsupported delimiter: {0}")]
    Delimiter(u8),
    #[error("couldn't parse varint")]
    VarInt,
    #[error("field string contains non human readable characters")]
    InvalidField,
    #[error("byte string contains invalid fractional index")]
    InvalidIndex,
    #[error("path exceeds maximum length of {} bytes", i16::MAX)]
    TooLong,
}

/// Individual path segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment<'a> {
    /// String field.
    Field(Field<'a>),
    /// Fractional index.
    Index(FractionalIndex<'a>),
    /// Special tail case (always at the end of the path).
    Tail(PathTail),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathTail {
    Chunked(u64),
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Field<'a>(&'a str);

impl<'a> Field<'a> {
    pub fn new(s: &'a str) -> crate::Result<Self, PathError> {
        if Self::is_valid(s) {
            Ok(Self(s))
        } else {
            Err(PathError::InvalidField)
        }
    }

    #[inline]
    pub unsafe fn new_unchecked(s: &'a str) -> Self {
        Self(s)
    }

    #[inline]
    pub fn is_valid(s: &str) -> bool {
        for byte in s.as_bytes() {
            if *byte < 32 {
                return false;
            }
        }
        true
    }
}

impl<'a> Deref for Field<'a> {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> TryFrom<&'a str> for Field<'a> {
    type Error = PathError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'a> From<Field<'a>> for &'a str {
    #[inline]
    fn from(value: Field<'a>) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::lseq::{FractionalIndex, Segment};
    use super::read::PathReader;
    use super::write::PathWriter;
    use super::*;
    use crate::pid::PID;
    use crate::varint::VarInt;
    use proptest::prelude::*;

    fn valid_field() -> impl Strategy<Value = String> {
        proptest::string::string_regex("[a-zA-Z][a-zA-Z0-9_]{0,63}").unwrap()
    }

    fn valid_index() -> impl Strategy<Value = Vec<u8>> {
        proptest::collection::vec((1u32..=100, 0u32..=100), 1..=3).prop_map(|segs| {
            let mut buf = Vec::new();
            for (pid_val, seq) in segs {
                let seg = Segment::new(PID::new(pid_val).unwrap(), seq);
                seg.write(&mut buf).unwrap();
            }
            buf
        })
    }

    #[derive(Debug, Clone)]
    enum TestSegment {
        Field(String),
        Index(Vec<u8>),
    }

    fn path_segment() -> impl Strategy<Value = TestSegment> {
        prop_oneof![
            valid_field().prop_map(TestSegment::Field),
            valid_index().prop_map(TestSegment::Index),
        ]
    }

    proptest! {
        #[test]
        fn roundtrip_path(segments in proptest::collection::vec(path_segment(), 1..=8)) {
            let mut writer = PathWriter::new(Vec::new());
            for seg in &segments {
                match seg {
                    TestSegment::Field(f) => writer.push_field(f).unwrap(),
                    TestSegment::Index(idx) => {
                        writer.push_index(FractionalIndex::new(idx)).unwrap();
                    }
                }
            }
            let buf = writer.finish();

            let reader = PathReader::new(&buf);
            let parsed: Vec<_> = reader.collect::<Result<Vec<_>, _>>().unwrap();

            prop_assert_eq!(parsed.len(), segments.len());
            for (p, o) in parsed.iter().zip(segments.iter()) {
                match (p, o) {
                    (PathSegment::Field(f), TestSegment::Field(s)) => {
                        prop_assert_eq!(&**f, s.as_str());
                    }
                    (PathSegment::Index(idx), TestSegment::Index(bytes)) => {
                        prop_assert_eq!(idx.bytes(), bytes.as_slice());
                    }
                    _ => prop_assert!(false, "segment type mismatch"),
                }
            }
        }
    }

    #[test]
    fn roundtrip_chunked_tail() {
        let mut writer = PathWriter::new(Vec::new());
        writer.push_field("content").unwrap();
        let buf = writer.chunked(42).unwrap();

        let reader = PathReader::new(&buf);
        let segments: Vec<_> = reader.collect::<Result<Vec<_>, _>>().unwrap();

        assert_eq!(segments.len(), 2);
        assert_eq!(
            segments[0],
            PathSegment::Field(Field::new("content").unwrap())
        );
        assert_eq!(segments[1], PathSegment::Tail(PathTail::Chunked(42)));
    }

    #[test]
    fn path_length_limit_on_field() {
        let mut writer = PathWriter::new(Vec::new());
        let long_field = "a".repeat(i16::MAX as usize);
        assert!(writer.push_field(&long_field).is_err());
    }

    #[test]
    fn path_length_limit_on_accumulated_fields() {
        let mut writer = PathWriter::new(Vec::new());
        let field = "a".repeat(10_000);
        writer.push_field(&field).unwrap();
        writer.push_field(&field).unwrap();
        writer.push_field(&field).unwrap();
        // 3 fields × (1 + 10_000) = 30_003, next would push past i16::MAX
        assert!(writer.push_field(&field).is_err());
    }

    #[test]
    fn path_length_limit_on_chunked() {
        let mut writer = PathWriter::new(Vec::new());
        let field = "a".repeat(i16::MAX as usize - 2);
        writer.push_field(&field).unwrap();
        assert!(writer.chunked(u64::MAX).is_err());
    }
}
