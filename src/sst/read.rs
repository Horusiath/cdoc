use super::{BLOOM_HASH_COUNT, FOOTER_SIZE, PREFIX_RESET_INTERVAL, SSTableFooter, bloom_hash};
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use zerocopy::FromBytes;

/// Memory-mapped reader for an SSTable file.
pub(crate) struct SSTableReader {
    mmap: Mmap,
    footer: SSTableFooter,
    /// Parsed block index: (first_key, offset).
    block_index: Vec<(Vec<u8>, u64)>,
    /// Raw bloom filter bytes.
    bloom: Vec<u8>,
}

impl SSTableReader {
    /// Opens and memory-maps an SSTable file.
    pub fn open(path: &Path) -> crate::Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        if mmap.len() < FOOTER_SIZE {
            return Err(crate::Error::Corruption("SSTable file too small".into()));
        }

        let footer_start = mmap.len() - FOOTER_SIZE;
        let footer = SSTableFooter::read_from_bytes(&mmap[footer_start..])
            .map_err(|_| crate::Error::Corruption("failed to parse SSTable footer".into()))?
            .clone();

        // Parse block index.
        let idx_start = footer.index_offset.get() as usize;
        let idx_end = footer.bloom_offset.get() as usize;
        let mut block_index = Vec::new();
        let mut pos = idx_start;
        while pos < idx_end {
            if pos + 2 > idx_end {
                break;
            }
            let key_len = u16::from_le_bytes([mmap[pos], mmap[pos + 1]]) as usize;
            pos += 2;
            if pos + key_len + 16 + 8 > mmap.len() {
                break;
            }
            let first_key = mmap[pos..pos + key_len].to_vec();
            pos += key_len;
            pos += 16; // skip min_ts + max_ts
            let offset =
                u64::from_le_bytes(mmap[pos..pos + 8].try_into().map_err(|_| {
                    crate::Error::Corruption("block index offset parse failed".into())
                })?);
            pos += 8;
            block_index.push((first_key, offset));
        }

        // Parse bloom filter.
        let bloom_start = footer.bloom_offset.get() as usize;
        let bloom_section_start = bloom_start + 4; // skip hash_count u32
        let bloom_section_end = footer_start;
        let bloom = if bloom_section_end > bloom_section_start {
            mmap[bloom_section_start..bloom_section_end].to_vec()
        } else {
            Vec::new()
        };

        Ok(SSTableReader {
            mmap,
            footer,
            block_index,
            bloom,
        })
    }

    /// Checks the bloom filter. Returns `true` if the key *might* exist.
    fn bloom_may_contain(&self, key: &[u8]) -> bool {
        if self.bloom.is_empty() {
            return true; // no filter → assume present
        }
        let total_bits = self.bloom.len() * 8;
        for i in 0..BLOOM_HASH_COUNT {
            let h = bloom_hash(key, i);
            let bit_pos = (h as usize) % total_bits;
            if self.bloom[bit_pos / 8] & (1 << (bit_pos % 8)) == 0 {
                return false;
            }
        }
        true
    }

    /// Looks up a single key. Returns the value if found.
    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        if !self.bloom_may_contain(key) {
            return None;
        }

        // Binary search block index to find the right block.
        let block_idx = match self
            .block_index
            .binary_search_by(|(k, _)| k.as_slice().cmp(key))
        {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };

        let entries = self.decode_block(block_idx);
        for (k, v) in entries {
            if k == key {
                return Some(v);
            }
        }
        None
    }

    /// Scans all entries whose key starts with `prefix`.
    pub fn scan_prefix(&self, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        let mut results = Vec::new();

        // Find the first block that could contain keys with this prefix.
        let start_block = match self
            .block_index
            .binary_search_by(|(k, _)| k.as_slice().cmp(prefix))
        {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };

        for block_idx in start_block..self.block_index.len() {
            // If the block's first key is past our prefix, stop.
            if block_idx > start_block && !self.block_index[block_idx].0.starts_with(prefix) {
                // Check if the first key is beyond the prefix range.
                if self.block_index[block_idx].0.as_slice() > prefix
                    && !self.block_index[block_idx].0.starts_with(prefix)
                {
                    // Could still have trailing matches from the previous block decode,
                    // but we already decoded it. Check if first key is strictly greater
                    // than any possible prefixed key.
                    let past_prefix = prefix_successor(prefix);
                    if let Some(ref succ) = past_prefix
                        && self.block_index[block_idx].0.as_slice() >= succ.as_slice()
                    {
                        break;
                    }
                }
            }

            let entries = self.decode_block(block_idx);
            for (k, v) in entries {
                if k.starts_with(prefix) {
                    results.push((k, v));
                } else if k.as_slice() > prefix && !k.starts_with(prefix) {
                    // Past the prefix range, check if we can bail out.
                    if let Some(ref succ) = prefix_successor(prefix)
                        && k.as_slice() >= succ.as_slice()
                    {
                        return results;
                    }
                }
            }
        }

        results
    }

    /// Decodes all entries in the block at `block_idx`.
    fn decode_block(&self, block_idx: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
        let block_start = self.block_index[block_idx].1 as usize;
        let block_end = if block_idx + 1 < self.block_index.len() {
            self.block_index[block_idx + 1].1 as usize
        } else {
            self.footer.index_offset.get() as usize
        };

        if block_end <= block_start + 4 || block_end > self.mmap.len() {
            return Vec::new();
        }

        // Last 4 bytes of the block are CRC32.
        let data = &self.mmap[block_start..block_end - 4];

        let mut entries = Vec::new();
        let mut prev_key = Vec::new();
        let mut pos = 0;
        let mut entry_count = 0;

        while pos + 6 <= data.len() {
            let shared = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
            let suffix_len = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;
            let value_len = u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as usize;
            pos += 6;

            if pos + suffix_len + value_len > data.len() {
                break;
            }

            // Reset prefix compression every PREFIX_RESET_INTERVAL entries.
            if entry_count % PREFIX_RESET_INTERVAL == 0 {
                prev_key.clear();
            }

            let mut key = Vec::with_capacity(shared + suffix_len);
            key.extend_from_slice(&prev_key[..shared.min(prev_key.len())]);
            key.extend_from_slice(&data[pos..pos + suffix_len]);
            pos += suffix_len;

            let value = data[pos..pos + value_len].to_vec();
            pos += value_len;

            prev_key = key.clone();
            entries.push((key, value));
            entry_count += 1;
        }

        entries
    }
}

/// Computes the lexicographic successor of a prefix (for range termination).
fn prefix_successor(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut succ = prefix.to_vec();
    while let Some(&last) = succ.last() {
        if last < 0xFF {
            *succ.last_mut().unwrap() = last + 1;
            return Some(succ);
        }
        succ.pop();
    }
    None
}
