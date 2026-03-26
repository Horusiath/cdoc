use crate::sst::memtable::MemTable;
use crate::sst::{SSTableReader, SSTableWriter};
use crate::transaction::{ReadOnlyTransaction, ReadWriteTransaction};
use crate::wal::WriteAheadLog;
use arc_swap::ArcSwap;
use parking_lot::Mutex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default MemTable flush threshold (4 MiB).
const DEFAULT_MEMTABLE_SIZE: usize = 4 * 1024 * 1024;

/// Shared state visible to all readers (swapped atomically).
pub(crate) struct ReadableState {
    /// Active MemTable containing the most recent unflushed writes.
    pub active: MemTable,
    /// Sorted SSTables from newest (index 0) to oldest.
    pub sstables: Vec<Arc<SSTableReader>>,
}

/// State protected by the single-writer mutex.
pub(crate) struct WriterState {
    pub wal: WriteAheadLog,
}

/// Internal database state shared via `Arc`.
pub(crate) struct DbInner {
    pub readable: ArcSwap<ReadableState>,
    pub writer: Arc<Mutex<WriterState>>,
    pub sst_dir: PathBuf,
    pub memtable_size: usize,
}

/// Handle to an LSM-tree backed key-value database.
///
/// Cheap to clone (inner state is behind `Arc`).
#[derive(Clone)]
pub struct Db {
    pub(crate) inner: Arc<DbInner>,
}

impl Db {
    /// Opens (or creates) a database at the paths specified by `options`.
    pub fn open(options: DbOptions) -> crate::Result<Self> {
        let sst_dir = options.base_path.join("sst");
        let wal_dir = options.wal_path.join("wal");
        fs::create_dir_all(&sst_dir)?;
        fs::create_dir_all(&wal_dir)?;

        // Clean up incomplete SSTable writes from prior crashes.
        for entry in fs::read_dir(&sst_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                fs::remove_file(&path)?;
            }
        }

        // Load existing SSTables sorted by name descending (newest first).
        let mut sst_paths: Vec<PathBuf> = Vec::new();
        for entry in fs::read_dir(&sst_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("sst") {
                sst_paths.push(path);
            }
        }
        sst_paths.sort();
        sst_paths.reverse();

        let mut sstables = Vec::with_capacity(sst_paths.len());
        for path in &sst_paths {
            sstables.push(Arc::new(SSTableReader::open(path)?));
        }

        // Replay WAL into a fresh MemTable.
        let wal_path = wal_dir.join("current.wal");
        let mut memtable = MemTable::new();
        if wal_path.exists() {
            let entries = WriteAheadLog::replay(&wal_path)?;
            for (key, value) in entries {
                memtable.insert(&key, value);
            }
        }

        // Create (or truncate) the active WAL file.
        let wal = WriteAheadLog::create(&wal_path)?;

        let readable = ReadableState {
            active: memtable,
            sstables,
        };

        let writer = WriterState { wal };

        let inner = DbInner {
            readable: ArcSwap::new(Arc::new(readable)),
            writer: Arc::new(Mutex::new(writer)),
            sst_dir,
            memtable_size: options.memtable_size,
        };

        Ok(Db {
            inner: Arc::new(inner),
        })
    }

    /// Starts a read-only transaction (snapshot).
    pub fn begin_readonly(&self) -> ReadOnlyTransaction {
        let snapshot = self.inner.readable.load_full();
        ReadOnlyTransaction::new(snapshot)
    }

    /// Starts a read-write transaction (acquires the single-writer lock).
    pub fn begin(&self) -> ReadWriteTransaction {
        ReadWriteTransaction::new(self.inner.clone())
    }
}

/// Flushes a MemTable to a new SSTable file, returning the opened reader.
pub(crate) fn flush_memtable(memtable: &MemTable, sst_dir: &Path) -> crate::Result<SSTableReader> {
    use crate::hlc::Timestamp;

    let ts = Timestamp::now();
    let tmp_path = sst_dir.join(format!("{}.sst.tmp", ts));
    let final_path = sst_dir.join(format!("{}.sst", ts));

    let file = fs::File::create(&tmp_path)?;
    let mut writer = SSTableWriter::new(file, ts, ts);
    for (key, value) in memtable.iter() {
        writer.add(key, value)?;
    }
    writer.finish()?;

    // Atomic rename so readers never see a partial file.
    fs::rename(&tmp_path, &final_path)?;

    SSTableReader::open(&final_path)
}

/// Configuration for opening a [`Db`].
#[derive(Debug, Clone)]
pub struct DbOptions {
    /// Directory path where main residual data is stored.
    base_path: PathBuf,
    /// Directory path where write-ahead log files are stored.
    wal_path: PathBuf,
    /// MemTable flush threshold in bytes.
    memtable_size: usize,
}

impl DbOptions {
    /// Creates options with default settings. Both SSTable and WAL data will
    /// live under `path` (in different subdirectories).
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        DbOptions {
            base_path: path.as_ref().to_path_buf(),
            wal_path: path.as_ref().to_path_buf(),
            memtable_size: DEFAULT_MEMTABLE_SIZE,
        }
    }

    /// Sets the MemTable flush threshold in bytes.
    pub fn with_memtable_size(mut self, size: usize) -> Self {
        self.memtable_size = size;
        self
    }
}
