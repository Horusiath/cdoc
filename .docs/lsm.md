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

> Keep in mind that we're still evaluating alternatives for using `Memtables`. Especially combination of SQLite
> WAL+memory mapped WAL index file and/or WiskKey.

Memory tables are used as an in-memory buffer for the most recent data writes. They allow to keep the most recent
writes small and organized, so that later when we need to flush them to the disk (in form of SSTables), we can do it
quickly in a single pass.

In CDoc `MemTable` implementation will follow a Adaptive Radix Tree (ART) design: it's a reasonable choice since the
document tree decomposition relies heavily on common prefixes and reads often reach for the ranges of keys under the
same prefix. Inside of ART keys are already decomposed [paths](./object-decomposition.md) to an overall document
structure, while values are represented as byte strings. These byte string can contain path specific data like Last
Write Wins (LWW) register headers - such headers are encoded using zero-copy strategy.

`MemTable` size is configurable (through `DbOptions.memtable_size`) and it's `4MiB` by default.

## SSTables

SSTables persist ordered key-value entries (where keys are stored in key-prefix compressed form) in blocks, followed
by the footer. Each block defaults to `64KiB`. Each `EntryHeader` consists of:

- `U16` (little endian): number bytes in common between current and previous key.
- `U16` (little endian): number of bytes building a suffix of a current key.
- `U16` (little endian): number of bytes building a value. Values larger than `U16::MAX` are split.
- unique bytes of current entry's key
- unique bytes of current entry's value

Each block ends with a `U32` (little endian) value which is a CRC32 checksum of the current block. This way we can
confirm that individual blocks haven't been corrupted. In the future we think about expanding network protocol with
corruption-repair mechanism that will enable peers to ask other peers for data in blocks that have been corrupted
automatically.

Thanks of blob splits (chunking) we can always fit values in `64KiB` in total if necessary.

Each SSTable ends with footer. Footer contains following information:

- First key of each block, min/max timestamps (2 * 8 bytes) of values in that block, and offset of that block (8 bytes)
  in SST file.
- Bloom filter: variable size, counted as `filter_size = num_keys × 10 / 8` bytes.
- File offset where does the block index starts: 8 bytes (little endian).
- File offset where does the bloom filter starts: 8 bytes (little endian).
- Number of keys: 8 bytes (little endian).
- Version: 4 bytes (little endian).
- CRC32 checksum of the entire file: 4 bytes - it's necessary since the disk corruption could hit a footer.

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

CDoc uses Sized Time-Window Compaction Strategy. SSTable file itself is combination of `{min_timestamp}-
{max_timestamp}.sst` representing min/max timestamps of the values that can be found in this file.

Whenever a MemTable is flushed, we create a new SSTable file. Those files can be compacted manually on time range
basis i.e. last hour, last day or last week. They can also be compacted on size/count basis: we can track individual SST
file sizes and whenever their number and size run over the specified threshold, we merge them together using groups
of files with adjacent time ranges . In practice this is similar to leveled approach: in this scenario older SST
files and files representing wider time ranges can act as higher level than the more recent ones, eg. merge last
24 1hour-span files into a single 1day-span file, then 7day-span files into a single 1week-span one etc.