use crate::hlc::Timestamp;
use crate::{LE32, LE64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub struct SSTable {}

impl SSTable {}

/// Footer of the SSTable. This struct can be zero-copy mapped directly. For the remaining footer
/// data (block index and bloom filter), you need to refer to `index_offset` and `bloom_offset`.
#[repr(C)]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct SSTableFooter {
    /// File offset where the block index starts.
    pub index_offset: LE64,
    /// File offset where the bloom filter starts.
    pub bloom_offset: LE64,
    /// The lowest LWW timestamp found in the file.
    pub min_timestamp: Timestamp,
    /// The highest LWW timestamp found in the file.
    pub max_timestamp: Timestamp,
    /// Total number of entries in the file.
    pub entry_count: LE64,
    /// Version of the SSTable.
    pub version: LE32,
    /// Checksum of the entire file.
    pub checksum: LE32,
}

impl SSTableFooter {
    /// Returns size of the bloom filter in bytes. It depends on the number of entries.
    pub fn bloom_len(&self) -> usize {
        self.entry_count.get() as usize * 10 / 8
    }
}
