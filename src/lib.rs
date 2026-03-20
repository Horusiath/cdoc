

pub(crate) mod wal;
pub(crate) mod sst;
mod db;
mod pid;
mod varint;
mod path;
mod transaction;
mod hlc;

pub type U16 = zerocopy::big_endian::U16;
pub type U32 = zerocopy::big_endian::U32;
pub type U64 = zerocopy::big_endian::U64;
pub type U128 = zerocopy::big_endian::U128;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("failed to deserialize zerocopy struct")]
    ZeroCopy,
}

impl<T> From<zerocopy::CastError<&[u8], T>> for Error {
    fn from(_: zerocopy::CastError<&[u8], T>) -> Self {
        Error::ZeroCopy
    }
}

pub type Result<T> = std::result::Result<T, Error>;