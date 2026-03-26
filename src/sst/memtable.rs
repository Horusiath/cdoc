use std::io::Write;

/// Memtable implementation.
#[derive(Clone)]
pub(crate) struct MemTable {
    inner: rart::VersionedAdaptiveRadixTree<rart::VectorKey, Vec<u8>>,
}

impl MemTable {
    pub fn new() -> Self {
        MemTable {
            inner: rart::VersionedAdaptiveRadixTree::new(),
        }
    }

    /// Snapshots current [MemTable]. Used by [crate::ReadWriteTransaction].
    /// [crate::ReadOnlyTransaction] are using clone variant instead.
    pub fn snapshot(&self) -> Self {
        //TODO: when transaction commits - since at the moment we only allow one active
        // read-write transaction at the time - we should be able to do atomic compare-and-swap
        // of the memtable used by that transaction with the one used by the database.
        MemTable {
            inner: self.inner.snapshot(),
        }
    }

    /// Flushes the content of a current [MemTable] into given writer. This is used to produce an
    /// [SSTable] file.
    pub fn flush<W: Write>(&self, w: &mut W) -> crate::Result<()> {
        todo!()
    }
}
