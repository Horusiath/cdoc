# Conflict-free Replicated Data Types

Conflict-free Replicated Data Types (CRDT) are data structures designed for offline-first synchronization: they 
enable systems to concurrently write data to them while out-of-sync for prolonged periods of time. They ensure that 
once synchronization happen, their divergent states will eventually converge to the same consistent state.

In CDoc, the data structure is size-unbound document tree, which can represent **string**-key fields or index-like 
sequences. We operate over several different types of data: 
- Atomic values in the leafs of the document tree are operating as Last Write Wins (LWW) registers using [Hybrid 
  Logical Clocks](./hlc.md) as their source of synchronization.
- Indexed sequences like arrays or collaborative text use [non-interleaving linear sequences](./lseq.md) (LSeq).

## Tombstoning

## Building delta updates
