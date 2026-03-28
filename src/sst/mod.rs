pub(crate) mod compaction;
pub mod memtable;
pub mod read;
pub mod write;

pub use read::SSTableReader;
pub use write::SSTableWriter;

use crate::hlc::Timestamp;
use crate::{LE32, LE64};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Target block size before starting a new block.
const BLOCK_SIZE: usize = 64 * 1024;

/// Number of entries between prefix resets within a block.
const PREFIX_RESET_INTERVAL: usize = 16;

/// Number of hash functions for the bloom filter.
const BLOOM_HASH_COUNT: u32 = 7;

/// Fixed-size footer at the end of every SSTable file.
/// Can be zero-copy mapped directly from the file.
#[repr(C)]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct SSTableFooter {
    /// File offset where the block index starts.
    pub index_offset: LE64,
    /// File offset where the bloom filter starts.
    pub bloom_offset: LE64,
    /// The lowest HLC timestamp found in the file.
    pub min_timestamp: Timestamp,
    /// The highest HLC timestamp found in the file.
    pub max_timestamp: Timestamp,
    /// Total number of key-value entries in the file.
    pub entry_count: LE64,
    /// SSTable format version.
    pub version: LE32,
    /// CRC32 checksum of the entire file (excluding this field itself).
    pub checksum: LE32,
}

const FOOTER_SIZE: usize = size_of::<SSTableFooter>();

impl SSTableFooter {}

/// Simple double-hashing bloom filter hash.
fn bloom_hash(key: &[u8], i: u32) -> u64 {
    let h1 = {
        let mut h = crc32fast::Hasher::new();
        h.update(key);
        h.finalize() as u64
    };
    let h2 = {
        let mut h = crc32fast::Hasher::new_with_initial(0x9e3779b9);
        h.update(key);
        h.finalize() as u64
    };
    h1.wrapping_add((i as u64).wrapping_mul(h2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn roundtrip_write_and_read() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.sst");

        let ts = Timestamp::now();
        {
            let file = File::create(&path)?;
            let mut w = SSTableWriter::new(file, ts, ts);
            for i in 0u32..100 {
                let key = format!("key_{:05}", i);
                let val = format!("value_{}", i);
                w.add(key.as_bytes(), val.as_bytes())?;
            }
            w.finish()?;
        }

        let reader = SSTableReader::open(&path)?;

        // Point lookup.
        let val = reader.get(b"key_00042");
        assert_eq!(val.as_deref(), Some(b"value_42".as_slice()));

        // Missing key.
        assert!(reader.get(b"nonexistent").is_none());

        Ok(())
    }

    #[test]
    fn prefix_scan() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.sst");

        let ts = Timestamp::now();
        {
            let file = File::create(&path)?;
            let mut w = SSTableWriter::new(file, ts, ts);
            // Sorted keys.
            w.add(b"aaa/1", b"v1")?;
            w.add(b"aaa/2", b"v2")?;
            w.add(b"aab/1", b"v3")?;
            w.add(b"bbb/1", b"v4")?;
            w.finish()?;
        }

        let reader = SSTableReader::open(&path)?;
        let results = reader.scan_prefix(b"aaa/");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, b"aaa/1");
        assert_eq!(results[1].0, b"aaa/2");

        Ok(())
    }

    #[test]
    fn bloom_filter_rejects_missing_keys() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.sst");

        let ts = Timestamp::now();
        {
            let file = File::create(&path)?;
            let mut w = SSTableWriter::new(file, ts, ts);
            w.add(b"existing_key", b"val")?;
            w.finish()?;
        }

        let reader = SSTableReader::open(&path)?;
        // Bloom filter should (almost certainly) reject a clearly absent key.
        // We can't guarantee 0 false positives, but a single absent key
        // should very rarely pass.
        assert!(reader.get(b"definitely_not_here_xyz").is_none());

        Ok(())
    }
}
