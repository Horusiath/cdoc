use std::path::{Path, PathBuf};

pub struct Db {

}

impl Db {
    pub fn open(options: DbOptions) -> crate::Result<Self> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct DbOptions {
    base_path: PathBuf,
    wal_path: PathBuf,
}

impl DbOptions {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        DbOptions {
            base_path: path.as_ref().to_path_buf(),
            wal_path: path.as_ref().to_path_buf(),
        }
    }
}