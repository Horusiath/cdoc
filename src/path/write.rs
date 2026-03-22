use crate::path::lseq::FractionalIndex;
use crate::path::{CHUNKED, DELIMITER, Field};
use crate::varint::VarInt;
use std::io::Write;

/// Struct used for encoding path format to any kind of input.
#[derive(Debug)]
pub struct PathWriter<W> {
    writer: W,
}

impl<W: Write> PathWriter<W> {
    pub fn new(writer: W) -> PathWriter<W> {
        PathWriter { writer }
    }

    /// Appends new field to the end of the path. This must be a human-readable string.
    /// ASCII characters from `0-31` range are not allowed.
    pub fn push_field(&mut self, field: &str) -> crate::Result<()> {
        let f = Field::new(field)?;

        self.writer.write_all(&[DELIMITER])?;
        self.writer.write_all(f.as_bytes())?;
        Ok(())
    }

    /// Appends a new fractional index to the end of the path.
    pub fn push_index(&mut self, index: FractionalIndex<'_>) -> crate::Result<()> {
        self.writer.write_all(index.bytes())?;
        Ok(())
    }

    /// Finalizes the path marking it as a chunked content ending at index.
    pub fn chunked(mut self, end_index: u64) -> crate::Result<W> {
        self.writer.write_all(&[CHUNKED])?;
        end_index.write(&mut self.writer)?;
        Ok(self.writer)
    }

    /// Finalizes the path.
    #[inline]
    pub fn finish(self) -> W {
        self.writer
    }
}
