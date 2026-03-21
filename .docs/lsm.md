# Log Structured Merge Trees

CDoc internals are realized as a Log Structured Merge Tree persistent key-value store with key-prefix compaction.

We use a standard implementation of Log Structure Merge Tree, with a single writer and any number of concurrent readers.
Readers don't block writer and vice versa. While a single writer is possible, we want to provide a limited scope for
MVCC support in form of [multi-WAL](./wal.md).

## Why custom LSM?

The question as for why we decided to build our own LSM instead of using existing one (i.e. RocksDB) lies in
characteristics of the work that we want to perform:

- Most of our values will work around timestamp-based Last Write Wins (LWW) registers. Transactions are also bound
  to them on a core level.
- Timestamps are also crucial for building delta updates. Therefore, having SSTable blocks representation in min-max
  timestamp indexing can speed up the process of builing such deltas.
- For the same reason, using time-window based compaction strategy may seem to be viable option.
- One of the desired features is support for timestamp-based snapshots. Persistence subsystem must understand
  snapshot concept natively in order to keep alive past versions of records throughout the compaction process.
- We propose a novel approach towards replicating incremental changes through multiple peer-scoped Write-Ahead Logs,
  which doesn't really have any equivalent in this field.
- Our proposed modification to idea of Prefixed Entry Object Notation can be applied to SSTable blocks, WAL and
  network sync protocol altogether. It's highly desirable to have access to native building blocks of LSM that we
  want to use.

## Memtables

## SSTables

### Key-prefix compression

Key-prefix compression is essential for CDoc. Basically, every SST block entry starts with `u16` value describing
how many bytes current key shares with the previous one. For first entry in the block this value is always `0`.
Additionally, to prevent backtracking over the entire block, every 16 keys, we additionally reset the prefixes.

CDoc path keys are limited to 32KiB in length, so their size will never reach beyond `i16::MAX` value.

### BLOB splits

We utilize PEON's concept of chunking big data into multiple entries as a way to handle large binary values. Since
path is capable of informing if it's containing a chunked value, we can use these to make entries always fit the
block size.

## Compaction