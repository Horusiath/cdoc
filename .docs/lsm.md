# Log Structured Merge Trees

CDoc internals are realized as a Log Structured Merge Tree persistent key-value store with key-prefix compaction.

## Why custom LSM?

The question as for why we decided to build our own LSM instead of using existing one (ie. RocksDB) lies in 
characteristics of the work that we want to perform:
- Most of our values will work around timestamp-based Last Write Wins (LWW) registers. Transactions are also bound 
  to them on core level.
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