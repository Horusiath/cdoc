use crate::hlc::Timestamp;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use zerocopy::IntoBytes;

/// Append-only Write-Ahead Log.
///
/// Records come in two flavours (distinguished by the sign bit of `key_len`):
/// - **Update**: `[key_len i16 LE][value_len u16 LE][key][value][crc u32 LE]`
/// - **Commit**: `[key_len i16 LE (sign set)][value_len u16 LE][timestamp u64 BE][key][value][crc u32 LE]`
///
/// CRC is rolling within a transaction (starts at 0, accumulates key+value bytes).
pub struct WriteAheadLog {
    file: File,
    /// File position of the byte right after the last commit record.
    last_commit_pos: u64,
    /// Rolling CRC accumulated across the current (uncommitted) transaction.
    rolling_crc: u32,
}

impl WriteAheadLog {
    const COMMIT_BIT: u16 = 1 << 15;
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
    pub fn replay(path: &Path) -> crate::Result<Vec<WalRecord>> {
        //TODO: async IO
        //TODO: change that into async stream

        let mut file = File::open(path)?;
        let file_len = file.metadata()?.len();
        let mut records = Vec::new();
        let mut last_commit_pos = 0;
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
            let key_len = u16::from_le_bytes(buf2);
            let is_commit = key_len & Self::COMMIT_BIT != 0;
            let key_len = (key_len & (!Self::COMMIT_BIT)) as usize;

            // Read value_len (u16 LE)
            if file.read_exact(&mut buf2).is_err() {
                break;
            }
            let value_len = u16::from_le_bytes(buf2) as usize;

            // buf len: key_len + value_len + timestamp_len (if commit) + crc32_len
            // create an uninitialized buffer
            let mut key = Vec::with_capacity(key_len);
            unsafe { key.set_len(key_len) };
            let mut value = Vec::with_capacity(value_len);
            unsafe { value.set_len(value_len) };

            // read the rest of the record
            if key_len != 0 {
                if file.read_exact(&mut key).is_err() {
                    break;
                }
                if file.read_exact(&mut value).is_err() {
                    break;
                }
            }
            let timestamp = if is_commit {
                let mut buf = [0u8; 8];
                if file.read_exact(&mut buf).is_err() {
                    break;
                }
                Some(Timestamp::new(u64::from_be_bytes(buf)))
            } else {
                None
            };

            let mut buf = [0u8; 4];
            if file.read_exact(&mut buf).is_err() {
                break;
            }
            let verify_crc = u32::from_le_bytes(buf);

            let mut hasher = crc32fast::Hasher::new_with_initial(rolling_crc);
            hasher.update(&key);
            hasher.update(&value);
            if let Some(timestamp) = &timestamp {
                hasher.update(timestamp.as_bytes());
            }
            let crc = hasher.finalize();

            if crc != verify_crc {
                // Corruption — stop replay, discard pending
                break;
            }

            rolling_crc = crc;

            // Only add entries with actual data (commit-only records have no key/value).
            records.push(WalRecord::new(key, value, timestamp));

            if is_commit {
                last_commit_pos = records.len();
                rolling_crc = 0;
            }
        }

        records.truncate(last_commit_pos);
        Ok(records)
    }

    pub fn write_commit(&mut self, ts: Timestamp) -> crate::Result<()> {
        self.write_record(&[], &[], Some(&ts))
    }

    /// Appends an update record for the current transaction.
    pub fn write_record(
        &mut self,
        key: &[u8],
        value: &[u8],
        commit: Option<&Timestamp>,
    ) -> crate::Result<()> {
        let mut key_len = key.len() as u16; // sign bit clear → update
        if commit.is_some() {
            key_len |= Self::COMMIT_BIT;
        }
        self.file.write_all(&key_len.to_le_bytes())?;
        self.file.write_all(&(value.len() as u16).to_le_bytes())?;

        let mut hasher = crc32fast::Hasher::new_with_initial(self.rolling_crc);

        self.file.write_all(key)?;
        hasher.update(key);
        self.file.write_all(value)?;
        hasher.update(value);

        if let Some(ts) = commit {
            self.file.write_all(ts.as_bytes())?;
            hasher.update(ts.as_bytes());
        }

        self.rolling_crc = hasher.finalize();

        self.file.write_all(&self.rolling_crc.to_le_bytes())?;
        self.file.flush()?;

        if commit.is_some() {
            self.last_commit_pos = self.file.stream_position()?;
            self.rolling_crc = 0;
        }

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalRecord {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    pub timestamp: Option<Timestamp>,
}

impl WalRecord {
    #[inline]
    pub fn new<B1, B2>(key: B1, value: B2, timestamp: Option<Timestamp>) -> Self
    where
        B1: Into<Vec<u8>>,
        B2: Into<Vec<u8>>,
    {
        WalRecord {
            key: key.into(),
            value: value.into(),
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_replay_single_transaction() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        let timestamp = Timestamp::now();
        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_record(b"key1", b"val1", None)?;
            wal.write_record(b"key2", b"val2", Some(&timestamp))?;
        }

        let records = WriteAheadLog::replay(&path)?;
        assert_eq!(
            records,
            vec![
                WalRecord::new(b"key1", b"val1", None),
                WalRecord::new(b"key2", b"val2", Some(timestamp)),
            ]
        );
        Ok(())
    }

    #[test]
    fn uncommitted_entries_discarded_on_replay() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        let timestamp = Timestamp::now();
        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_record(b"key1", b"val1", None)?;
            wal.write_record(&[], &[], Some(&timestamp))?;
            // second transaction never committed
            wal.write_record(b"key2", b"val2", None)?;
        }

        let records = WriteAheadLog::replay(&path)?;
        assert_eq!(
            records,
            vec![
                WalRecord::new(b"key1", b"val1", None),
                WalRecord::new(b"", b"", Some(timestamp)),
            ]
        );
        Ok(())
    }

    #[test]
    fn truncate_to_last_commit_discards_pending() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        let timestamp = Timestamp::now();
        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_record(b"k1", b"v1", None)?;
            wal.write_record(&[], &[], Some(&timestamp))?;
            wal.write_record(b"k2", b"v2", None)?;
            wal.truncate_to_last_commit()?;
        }

        let records = WriteAheadLog::replay(&path)?;
        assert_eq!(
            records,
            vec![
                WalRecord::new(b"k1", b"v1", None),
                WalRecord::new(b"", b"", Some(timestamp)),
            ]
        );
        Ok(())
    }

    #[test]
    fn reset_clears_everything() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.wal");

        {
            let mut wal = WriteAheadLog::create(&path)?;
            wal.write_record(b"k1", b"v1", None)?;
            wal.write_commit(Timestamp::now())?;
            wal.reset()?;
        }

        let records = WriteAheadLog::replay(&path)?;
        assert!(records.is_empty());
        Ok(())
    }
}
