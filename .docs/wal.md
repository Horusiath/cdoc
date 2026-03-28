# Write-Ahead Log

Each transaction update is backed by the Write-Ahead Log for that transaction. CDoc enables having multiple WAL
files at the same time, but only one active WAL file per PID. All WAL files live under the same `DbOptions.wal_path`
directory. A write-ahead log file path is a `{wal_path}/{pid}/{timestamp}.wal` where the `{pid}` subdirectory
matches the peer ID of a current peer - owner of that WAL - while `{timestamp}` is time when the WAL file was created.

Currently internal WAL structure is log of records. There are two kinds of records: **update** and **commit** record.
The general record structure is:

- `I16` (little endian) describing key length. For **update records** the sign bit is always `0`. For **commit
  records** it's always `1`.
- `U16` (little endian) describing value length. For deleted entries this value is `0`.
- key bytes
- value bytes
- If it's a **commit record** the next 8 bytes (`U64` big endian) is a HLC timestamp of the committed transaction.
- `U32` (little endian) CRC32 checksum of the record. It's composed as a rolling update: a checksum of previous
  record in the transaction (0 if it's a first record of the transaction) updated with key and value bytes of a
  current record.

This way we can also support one-shot transactions (small transactions that only update a single entry), since the
same record can be an update record and a commit record at the same time.

When replaying WAL, we don't commit transactions until we reached a commit record. When aborting the transaction, we
truncate WAL file to the position of the last committed record.