use crate::hlc::Timestamp;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Append-only Write-Ahead Log.
///
/// Records come in two flavours (distinguished by the sign bit of `key_len`):
/// - **Update**: `[key_len i16 LE][value_len u16 LE][key][value][crc u32 LE]`
/// - **Commit**: `[key_len i16 LE (sign set)][value_len u16 LE][timestamp u64 BE][key][value][crc u32 LE]`
///
/// CRC is rolling within a transaction (starts at 0, accumulates key+value bytes).
pub(crate) struct WriteAheadLog {
    file: File,
    /// File position of the byte right after the last commit record.
    last_commit_pos: u64,
    /// Rolling CRC accumulated across the current (uncommitted) transaction.
    rolling_crc: u32,
}

impl WriteAheadLog {
    /// Creates a new, empty WAL file at `path`.
    pub fn create(path: &Path) -> crate::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .read(true)
            .open(path)?;
        Ok(WriteAheadLog {
            file,
            last_commit_pos: 0,
            rolling_crc: 0,
        })
    }

    /// Replays a WAL file, returning only entries from committed transactions.
    pub fn replay(path: &Path) -> crate::Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut all_committed = Vec::new();
        let mut pending = Vec::new();
        let mut rolling_crc: u32 = 0;

        loop {
            let pos = file.stream_position()?;
            if pos >= file_len {
                break;
            }

            // Read key_len (i16 LE)
            let mut buf2 = [0u8; 2];
            if file.read_exact(&mut buf2).is_err() {
                break;
            }
            let raw_key_len = i16::from_le_bytes(buf2);
            let is_commit = raw_key_len < 0;
            let key_len = if is_commit {
                (raw_key_len & 0x7FFF) as usize
            } else {
                raw_key_len as usize
            };

            // Read value_len (u16 LE)
            if file.read_exact(&mut buf2).is_err() {
                break;
            }
            let value_len = u16::from_le_bytes(buf2) as usize;

            // If commit record, read timestamp (u64 BE) — we don't use it during replay
            if is_commit {
                let mut ts_buf = [0u8; 8];
                if file.read_exact(&mut ts_buf).is_err() {
                    break;
                }
            }

            // Read key
            let mut key = vec![0u8; key_len];
            if file.read_exact(&mut key).is_err() {
                break;
            }

            // Read value
            let mut value = vec![0u8; value_len];
            if file.read_exact(&mut value).is_err() {
                break;
            }

            // Read CRC (u32 LE)
            let mut crc_buf = [0u8; 4];
            if file.read_exact(&mut crc_buf).is_err() {
                break;
            }
            let stored_crc = u32::from_le_bytes(crc_buf);

            // Verify rolling CRC
            let mut hasher = crc32fast::Hasher::new_with_initial(rolling_crc);
            hasher.update(&key);
            hasher.update(&value);
            let computed_crc = hasher.finalize();

            if computed_crc != stored_crc {
                // Corruption — stop replay, discard pending
                break;
            }

            rolling_crc = computed_crc;

            // Only add entries with actual data (commit-only records have no key/value).
            if key_len > 0 || value_len > 0 {
                pending.push((key, value));
            }

            if is_commit {
                all_committed.append(&mut pending);
                rolling_crc = 0;
            }
        }

        Ok(all_committed)
    }

    /// Appends an update record for the current transaction.
    pub fn write_update(&mut self, key: &[u8], value: &[u8]) -> crate::Result<()> {
        let key_len = key.len() as i16; // sign bit clear → update
        self.file.write_all(&key_len.to_le_bytes())?;
        self.file.write_all(&(value.len() as u16).to_le_bytes())?;
        self.file.write_all(key)?;
        self.file.write_all(value)?;

        let mut hasher = crc32fast::Hasher::new_with_initial(self.rolling_crc);
        hasher.update(key);
        hasher.update(value);
        self.rolling_crc = hasher.finalize();

        self.file.write_all(&self.rolling_crc.to_le_bytes())?;
        self.file.flush()?;
        Ok(())
    }

    /// Appends a commit record, marking the end of the current transaction.
    pub fn write_commit(&mut self, ts: Timestamp) -> crate::Result<()> {
        // Commit record: key_len=0 with sign bit set → i16 = -32768 (0x8000)
        // sign bit set means commit record, remaining bits = key length (0 here)
        let raw: i16 = i16::MIN; // 0x8000 — commit marker with 0-length key
        self.file.write_all(&raw.to_le_bytes())?;
        self.file.write_all(&0u16.to_le_bytes())?; // value_len = 0

        // timestamp (u64 BE)
        let ts_bytes: [u8; 8] = zerocopy::IntoBytes::as_bytes(&ts)
            .try_into()
            .map_err(|_| crate::Error::Corruption("timestamp serialization failed".into()))?;
        self.file.write_all(&ts_bytes)?;

        // empty key + empty value → CRC over nothing new
        let hasher = crc32fast::Hasher::new_with_initial(self.rolling_crc);
        // no key/value bytes to update
        let crc = hasher.finalize();
        self.file.write_all(&crc.to_le_bytes())?;
        self.file.flush()?;

        self.last_commit_pos = self.file.stream_position()?;
        self.rolling_crc = 0;
        Ok(())
    }

    /// Truncates the WAL back to the position right after the last commit,
    /// effectively discarding any uncommitted writes.
    pub fn truncate_to_last_commit(&mut self) -> crate::Result<()> {
        self.file.set_len(self.last_commit_pos)?;
        self.file.seek(SeekFrom::Start(self.last_commit_pos))?;
        self.rolling_crc = 0;
        Ok(())
    }

    /// Resets the WAL to empty (used after MemTable flush).
    pub fn reset(&mut self) -> crate::Result<()> {
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.last_commit_pos = 0;
        self.rolling_crc = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_replay_single_transaction() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_update(b"key1", b"val1")?;
            wal.write_update(b"key2", b"val2")?;
            wal.write_commit(Timestamp::now())?;
        }

        let entries = WriteAheadLog::replay(&path)?;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], (b"key1".to_vec(), b"val1".to_vec()));
        assert_eq!(entries[1], (b"key2".to_vec(), b"val2".to_vec()));
        Ok(())
    }

    #[test]
    fn uncommitted_entries_discarded_on_replay() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_update(b"key1", b"val1")?;
            wal.write_commit(Timestamp::now())?;
            // second transaction never committed
            wal.write_update(b"key2", b"val2")?;
        }

        let entries = WriteAheadLog::replay(&path)?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, b"key1");
        Ok(())
    }

    #[test]
    fn truncate_to_last_commit_discards_pending() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_update(b"k1", b"v1")?;
            wal.write_commit(Timestamp::now())?;
            wal.write_update(b"k2", b"v2")?;
            wal.truncate_to_last_commit()?;
        }

        let entries = WriteAheadLog::replay(&path)?;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0, b"k1");
        Ok(())
    }

    #[test]
    fn reset_clears_everything() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_update(b"k1", b"v1")?;
            wal.write_commit(Timestamp::now())?;
            wal.reset()?;
        }

        let entries = WriteAheadLog::replay(&path)?;
        assert!(entries.is_empty());
        Ok(())
    }
}
