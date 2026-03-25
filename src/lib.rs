use crate::path::PathError;

mod db;
mod hlc;
mod mutation;
mod path;
mod pid;
mod query;
pub(crate) mod sst;
mod transaction;
mod varint;
pub(crate) mod wal;

pub use db::{Db, DbOptions};
pub use mutation::Mutation;
pub use path::lseq::FractionalIndex;
pub use pid::PID;
pub use transaction::{ReadOnlyTransaction, ReadWriteTransaction};

pub type U16 = zerocopy::big_endian::U16;
pub type U32 = zerocopy::big_endian::U32;
pub type U64 = zerocopy::big_endian::U64;
pub type U128 = zerocopy::big_endian::U128;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    Path(#[from] PathError),
    #[error("failed to deserialize zerocopy struct")]
    ZeroCopy,
}

impl<T> From<zerocopy::CastError<&[u8], T>> for Error {
    fn from(_: zerocopy::CastError<&[u8], T>) -> Self {
        Error::ZeroCopy
    }
}
