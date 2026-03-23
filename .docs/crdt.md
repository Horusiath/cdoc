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

Naive deletions are hard to implement in CRDT environment, as we need to be able to inform about possible deletions
in the future delta requests. For that we use a special tombstone marker that will put a deleted path (together with
the deletion timestamp) in a separate tombstone keyspace.

Those tombstones are then compared against values replicated through WAL and erased if not necessary.

## Building delta updates

CDoc has a unique (for CRDT) approach to replication. Usually for delta-state CRDT, one side offers a vector version
used to compute delta of necessary changes than the other side can provide. However, vector versions are inherently
unbounded and over time, for long living documents, can run into multiple kB.

CDoc synchronization works by providing a cutoff filter, which can be:

- Single HLC timestamp.
- Combination of HLC timestamp and PID.
- Document path prefix.

Since CDoc uses hybrid logical clocks instead of monotonically incrementing sequencers, we can provide a single HLC
timestamp to simply say what was the last known synchronization time and continue from there, regardless which peer
do we talk to. Since this can come with some redundancy, we can optionally also provide specific PID to inform that
we're interested only with updates coming from specific peer.

Additionally, CDoc supports partial replication in form of path prefixes. When provided, it will inform other peer,
that we're interested only in updates coming from specific subtree of the document matching the given path. This can
also be used to build shared (synchronized) and private (local-only) spaces in the document.
