use crate::path::PathError;

mod db;
mod hlc;
mod mutation;
#[allow(dead_code)]
pub mod path;
mod pid;
mod query;
pub(crate) mod sst;
mod transaction;
mod varint;
pub(crate) mod wal;

pub use ciborium as cbor;
pub use db::{Db, DbOptions};
pub use mutation::{Mutation, Op, Segment};
pub use path::lseq::FractionalIndex;
pub use pid::PID;
pub use query::{Filter, Query, Select};
pub use transaction::{ReadOnlyTransaction, ReadWriteTransaction};

pub type BE16 = zerocopy::big_endian::U16;
pub type BE32 = zerocopy::big_endian::U32;
pub type BE64 = zerocopy::big_endian::U64;
pub type BE128 = zerocopy::big_endian::U128;

pub type LE16 = zerocopy::little_endian::U16;
pub type LE32 = zerocopy::little_endian::U32;
pub type LE64 = zerocopy::little_endian::U64;
pub type LE128 = zerocopy::little_endian::U128;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("invalid path: {0}")]
    Path(#[from] PathError),
    #[error("failed to deserialize zerocopy struct")]
    ZeroCopy,
    #[error("failed to serialize CBOR value: {0}")]
    Cbor(#[from] crate::cbor::ser::Error<std::io::Error>),
    #[error(transparent)]
    Query(#[from] crate::query::QueryError),
    #[error("data corruption: {0}")]
    Corruption(String),
}

impl<T> From<zerocopy::CastError<&[u8], T>> for Error {
    fn from(_: zerocopy::CastError<&[u8], T>) -> Self {
        Error::ZeroCopy
    }
}
