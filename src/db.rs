use crate::ReadWriteTransaction;
use crate::transaction::ReadOnlyTransaction;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Db {}

impl Db {
    pub fn open(options: DbOptions) -> crate::Result<Self> {
        todo!()
    }

    pub fn begin_readonly(&self) -> crate::Result<ReadOnlyTransaction> {
        todo!()
    }

    pub fn begin(&self) -> crate::Result<ReadWriteTransaction> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct DbOptions {
    /// Directory path where main residual data is stored.
    base_path: PathBuf,
    /// Directory path where write-ahead log files are stored.
    wal_path: PathBuf,
}

impl DbOptions {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        // base_path and wal_path by default point to the same space, as they have different
        // subdirectory structure and don't interfere with each other
        DbOptions {
            base_path: path.as_ref().to_path_buf(),
            wal_path: path.as_ref().to_path_buf(),
        }
    }
}
