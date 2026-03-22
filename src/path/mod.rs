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
