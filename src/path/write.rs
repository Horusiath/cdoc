use crate::path::lseq::FractionalIndex;
use crate::path::{
    DELIMITER, Field, PathError, TERMINATOR_CHUNKED, TERMINATOR_COUNTER, TERMINATOR_LWW,
};
use crate::varint::VarInt;
use std::io::Write;
use zerocopy::IntoBytes;

/// Struct used for encoding path format to any kind of input.
/// Tracks total bytes written to enforce the `i16::MAX` length limit.
#[derive(Debug)]
pub struct PathWriter<W> {
    writer: W,
    written: usize,
}

impl<W: Write> PathWriter<W> {
    pub fn new(writer: W) -> PathWriter<W> {
        PathWriter { writer, written: 0 }
    }

    fn ensure_capacity(&self, additional: usize) -> crate::Result<()> {
        if self.written + additional > i16::MAX as usize {
            Err(PathError::TooLong)?
        }
        Ok(())
    }

    /// Appends new field to the end of the path. This must be a human-readable string.
    /// ASCII characters from `0-31` range are not allowed.
    pub fn push_field(&mut self, field: &str) -> crate::Result<()> {
        let f = Field::new(field)?;
        let len = 1 + f.as_bytes().len();
        self.ensure_capacity(len)?;
        self.writer.write_all(&[DELIMITER])?;
        self.writer.write_all(f.as_bytes())?;
        self.written += len;
        Ok(())
    }

    /// Appends a new fractional index to the end of the path.
    pub fn push_index(&mut self, index: FractionalIndex<'_>) -> crate::Result<()> {
        let len = index.bytes().len();
        self.ensure_capacity(1 + len)?;
        self.writer.write_all(&[DELIMITER])?;
        self.writer.write_all(index.bytes())?;
        self.written += len;
        Ok(())
    }

    /// Finalizes the path marking it as a chunked content ending at index.
    pub fn lww_chunked(mut self, end_index: u64) -> crate::Result<W> {
        let be = end_index.to_be_bytes();
        let data_len = size_of::<u64>() - be.iter().take_while(|&&b| b == 0).count();
        self.ensure_capacity(2 + data_len)?;
        self.writer.write_all(&[DELIMITER, TERMINATOR_CHUNKED])?;
        end_index.write(&mut self.writer)?;
        Ok(self.writer)
    }

    /// Finalizes the path as a Last-Write Wins register.
    pub fn lww(mut self) -> crate::Result<W> {
        self.writer.write_all(&[DELIMITER, TERMINATOR_LWW])?;
        Ok(self.writer)
    }

    /// Finalizes the path as a Last-Write Wins register.
    pub fn counter(mut self, pid: crate::PID) -> crate::Result<W> {
        self.writer.write_all(&[DELIMITER, TERMINATOR_COUNTER])?;
        self.writer.write_all(pid.as_bytes())?;
        Ok(self.writer)
    }
}
