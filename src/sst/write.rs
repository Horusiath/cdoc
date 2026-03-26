use super::{BLOCK_SIZE, BLOOM_HASH_COUNT, PREFIX_RESET_INTERVAL, SSTableFooter, bloom_hash};
use crate::hlc::Timestamp;
use crate::{LE32, LE64};
use std::io::Write;

/// Builds an SSTable from pre-sorted key-value entries.
pub(crate) struct SSTableWriter<W: Write> {
    writer: W,
    /// Accumulator for the current block being written.
    block_buf: Vec<u8>,
    /// Previous key in the current block (for prefix compression).
    prev_key: Vec<u8>,
    /// Number of entries written into the current block.
    entries_in_block: usize,
    /// Total bytes written to the underlying writer so far.
    bytes_written: u64,
    /// Block index entries: (first_key, block_offset).
    index: Vec<BlockIndexEntry>,
    /// All keys seen — kept for building the bloom filter at the end.
    all_keys: Vec<Vec<u8>>,
    /// Running CRC over the whole file.
    file_hasher: crc32fast::Hasher,
    /// Global min timestamp.
    min_ts: Timestamp,
    /// Global max timestamp.
    max_ts: Timestamp,
}

struct BlockIndexEntry {
    first_key: Vec<u8>,
    offset: u64,
    min_ts: Timestamp,
    max_ts: Timestamp,
}

impl<W: Write> SSTableWriter<W> {
    /// Creates a new writer. `min_ts` / `max_ts` bracket the entries being flushed.
    pub fn new(writer: W, min_ts: Timestamp, max_ts: Timestamp) -> Self {
        SSTableWriter {
            writer,
            block_buf: Vec::with_capacity(BLOCK_SIZE + 1024),
            prev_key: Vec::new(),
            entries_in_block: 0,
            bytes_written: 0,
            index: Vec::new(),
            all_keys: Vec::new(),
            file_hasher: crc32fast::Hasher::new(),
            min_ts,
            max_ts,
        }
    }

    /// Adds a key-value entry. **Entries must be added in sorted key order.**
    pub fn add(&mut self, key: &[u8], value: &[u8]) -> crate::Result<()> {
        // Start a new block when the current one exceeds the target size.
        if !self.block_buf.is_empty() && self.block_buf.len() >= BLOCK_SIZE {
            self.finish_block()?;
        }

        // First entry in a new block — record it in the index.
        if self.entries_in_block == 0 {
            self.index.push(BlockIndexEntry {
                first_key: key.to_vec(),
                offset: self.bytes_written,
                min_ts: self.min_ts,
                max_ts: self.max_ts,
            });
            self.prev_key.clear();
        }

        // Every PREFIX_RESET_INTERVAL entries, reset prefix compression.
        let shared = if self.entries_in_block.is_multiple_of(PREFIX_RESET_INTERVAL) {
            self.prev_key.clear();
            0u16
        } else {
            common_prefix_len(&self.prev_key, key) as u16
        };

        let suffix = &key[shared as usize..];
        let suffix_len = suffix.len() as u16;
        let value_len = value.len() as u16;

        self.block_buf.extend_from_slice(&shared.to_le_bytes());
        self.block_buf.extend_from_slice(&suffix_len.to_le_bytes());
        self.block_buf.extend_from_slice(&value_len.to_le_bytes());
        self.block_buf.extend_from_slice(suffix);
        self.block_buf.extend_from_slice(value);

        self.prev_key.clear();
        self.prev_key.extend_from_slice(key);
        self.entries_in_block += 1;
        self.all_keys.push(key.to_vec());

        Ok(())
    }

    /// Flushes the current block to the writer, appending its CRC32.
    fn finish_block(&mut self) -> crate::Result<()> {
        if self.block_buf.is_empty() {
            return Ok(());
        }

        let block_crc = crc32fast::hash(&self.block_buf);
        self.block_buf.extend_from_slice(&block_crc.to_le_bytes());

        self.emit(&self.block_buf.clone())?;
        self.block_buf.clear();
        self.prev_key.clear();
        self.entries_in_block = 0;

        Ok(())
    }

    /// Writes bytes to the underlying writer, updating the running file CRC.
    fn emit(&mut self, data: &[u8]) -> crate::Result<()> {
        self.writer.write_all(data)?;
        self.file_hasher.update(data);
        self.bytes_written += data.len() as u64;
        Ok(())
    }

    /// Finishes writing: flushes the last block, writes block index, bloom
    /// filter, and footer. Returns the underlying writer.
    pub fn finish(mut self) -> crate::Result<W> {
        // Flush any remaining block data.
        self.finish_block()?;

        let index_offset = self.bytes_written;

        // ── Block index ──
        // Collect index entries into a buffer first to avoid borrow conflict.
        let mut index_buf = Vec::new();
        for entry in &self.index {
            let key_len = entry.first_key.len() as u16;
            index_buf.extend_from_slice(&key_len.to_le_bytes());
            index_buf.extend_from_slice(&entry.first_key);
            index_buf.extend_from_slice(zerocopy::IntoBytes::as_bytes(&entry.min_ts));
            index_buf.extend_from_slice(zerocopy::IntoBytes::as_bytes(&entry.max_ts));
            index_buf.extend_from_slice(&entry.offset.to_le_bytes());
        }
        self.emit(&index_buf)?;

        let bloom_offset = self.bytes_written;

        // ── Bloom filter ──
        let num_keys = self.all_keys.len();
        let filter_bits = if num_keys == 0 { 8 } else { num_keys * 10 };
        let filter_bytes = filter_bits.div_ceil(8);
        let mut bloom = vec![0u8; filter_bytes];

        for key in &self.all_keys {
            for i in 0..BLOOM_HASH_COUNT {
                let h = bloom_hash(key, i);
                let bit_pos = (h as usize) % (filter_bytes * 8);
                bloom[bit_pos / 8] |= 1 << (bit_pos % 8);
            }
        }

        // Write hash count then filter bytes.
        self.emit(&BLOOM_HASH_COUNT.to_le_bytes())?;
        self.emit(&bloom)?;

        // ── Footer ──
        let file_crc = self.file_hasher.clone().finalize();

        let footer = SSTableFooter {
            index_offset: LE64::new(index_offset),
            bloom_offset: LE64::new(bloom_offset),
            min_timestamp: self.min_ts,
            max_timestamp: self.max_ts,
            entry_count: LE64::new(num_keys as u64),
            version: LE32::new(1),
            checksum: LE32::new(file_crc),
        };

        let footer_bytes = zerocopy::IntoBytes::as_bytes(&footer);
        // Footer bytes are NOT included in the running CRC (the CRC field is part of footer).
        self.writer.write_all(footer_bytes)?;
        self.writer.flush()?;

        Ok(self.writer)
    }
}

/// Length of the common prefix between two byte slices.
fn common_prefix_len(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}
