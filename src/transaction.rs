use crate::Mutation;
use crate::db::{DbInner, ReadableState, WriterState};
use crate::hlc::Timestamp;
use crate::sst::memtable::MemTable;
use parking_lot::ArcMutexGuard;
use std::collections::BTreeMap;
use std::sync::Arc;

/// A read-only snapshot of the database.
///
/// Holds an `Arc` to the [`ReadableState`] captured at creation time, so writes
/// that happen after this transaction was opened are invisible.
pub struct ReadOnlyTransaction {
    snapshot: Arc<ReadableState>,
}

impl ReadOnlyTransaction {
    pub(crate) fn new(snapshot: Arc<ReadableState>) -> Self {
        ReadOnlyTransaction { snapshot }
    }

    /// Looks up a single key. MemTable is checked first, then SSTables
    /// from newest to oldest.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        if let Some(v) = self.snapshot.active.get(key) {
            return Some(v.to_vec());
        }
        for sst in &self.snapshot.sstables {
            if let Some(v) = sst.get(key) {
                return Some(v);
            }
        }
        None
    }

    /// Returns all key-value pairs whose key starts with `prefix`.
    /// Results are deduplicated — the newest version of each key wins.
    pub fn scan(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        merge_scan(prefix, &self.snapshot.active, &self.snapshot.sstables)
    }
}

/// A read-write transaction backed by the single-writer lock.
///
/// Writes go into a forked MemTable and the WAL. On [`commit`](Self::commit),
/// the forked MemTable replaces the active one atomically. On
/// [`abort`](Self::abort) (or [`Drop`]), uncommitted WAL records are truncated.
pub struct ReadWriteTransaction {
    db: Arc<DbInner>,
    /// Forked copy of the MemTable — all writes land here first.
    fork: MemTable,
    /// Guard holding the single-writer mutex.
    guard: Option<ArcMutexGuard<parking_lot::RawMutex, WriterState>>,
    /// Snapshot of the readable state at transaction start.
    snapshot: Arc<ReadableState>,
    /// Set to `true` once `commit()` or `abort()` has been called.
    finished: bool,
}

impl ReadWriteTransaction {
    pub(crate) fn new(db: Arc<DbInner>) -> Self {
        let guard = parking_lot::Mutex::lock_arc(&db.writer);
        let snapshot = db.readable.load_full();
        let fork = snapshot.active.clone();
        ReadWriteTransaction {
            db,
            fork,
            guard: Some(guard),
            snapshot,
            finished: false,
        }
    }

    /// Execute given mutation descriptor over the existing database.
    pub fn execute(&mut self, mutation: Mutation) -> crate::Result<()> {
        mutation.for_each(|key, value| self.insert(&key, &value))
    }

    /// Inserts a key-value pair into this transaction.
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> crate::Result<()> {
        if let Some(ref mut g) = self.guard {
            g.wal.write_record(key, value, None)?;
        }
        self.fork.insert(key, value.to_vec());
        Ok(())
    }

    /// Looks up a single key, seeing this transaction's uncommitted writes.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        if let Some(v) = self.fork.get(key) {
            return Some(v.to_vec());
        }
        for sst in &self.snapshot.sstables {
            if let Some(v) = sst.get(key) {
                return Some(v);
            }
        }
        None
    }

    /// Prefix scan, including uncommitted writes from this transaction.
    pub fn scan(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        merge_scan(prefix, &self.fork, &self.snapshot.sstables)
    }

    /// Commits the transaction: WAL commit record, atomic MemTable swap,
    /// and (optionally) SSTable flush when the threshold is exceeded.
    pub fn commit(mut self) -> crate::Result<()> {
        self.finished = true;
        let ts = Timestamp::now();

        {
            let guard = self.guard.as_mut().expect("guard taken before commit");
            guard.wal.write_commit(ts)?;
        }

        // Build the new readable state with our forked MemTable.
        let new_readable = Arc::new(ReadableState {
            active: self.fork.clone(),
            sstables: self.snapshot.sstables.clone(),
        });
        self.db.readable.store(new_readable);

        // Flush to SSTable if the MemTable exceeds the threshold.
        if self.fork.estimated_size() >= self.db.memtable_size {
            let reader = crate::db::flush_memtable(&self.fork, &self.db.sst_dir)?;
            let mut new_ssts = vec![Arc::new(reader)];
            new_ssts.extend(self.snapshot.sstables.iter().cloned());

            let flushed = Arc::new(ReadableState {
                active: MemTable::new(),
                sstables: new_ssts,
            });
            self.db.readable.store(flushed);

            if let Some(ref mut g) = self.guard {
                g.wal.reset()?;
            }
        }

        Ok(())
    }

    /// Explicitly aborts the transaction, discarding all writes.
    pub fn abort(mut self) -> crate::Result<()> {
        self.finished = true;
        if let Some(ref mut g) = self.guard {
            g.wal.truncate_to_last_commit()?;
        }
        Ok(())
    }
}

impl Drop for ReadWriteTransaction {
    fn drop(&mut self) {
        if !self.finished {
            // Best-effort abort on drop — ignore errors.
            if let Some(ref mut g) = self.guard {
                let _ = g.wal.truncate_to_last_commit();
            }
        }
    }
}

/// Merges prefix-scan results from a MemTable and a list of SSTables.
/// MemTable entries win over SSTable entries for the same key.
/// SSTables are assumed newest-first, so earlier entries win.
fn merge_scan(
    prefix: &[u8],
    memtable: &MemTable,
    sstables: &[Arc<crate::sst::SSTableReader>],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();

    // SSTables from oldest to newest so newer entries overwrite older ones.
    for sst in sstables.iter().rev() {
        for (k, v) in sst.scan_prefix(prefix) {
            merged.insert(k, v);
        }
    }

    // MemTable entries win over all SSTables.
    for (k, v) in memtable.scan(prefix) {
        merged.insert(k.to_vec(), v.to_vec());
    }

    merged.into_iter().collect()
}
