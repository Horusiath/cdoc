# CDoc

CDoc (alias: *sea dog* 🦭) is a persistent, embedded document store, which enables data modifications in spirit of
Conflict-free Replicated Data Types.

## Core concepts

- Objects are stored as paths (see: [Object → Path decomposition](./.docs/object-decomposition.md)): this enables fast
  filtering by path and storing objects of arbitrary size (even bigger than an available RAM).
    - Paths are stored using key-prefix compression to reduce occupied space.
- Index-based sequences (list, collaborative text) uses Linear Sequence ([LSeq](./.docs/lseq.md)) algorithm.
    - Additionally, we use non-interleaving variant of LSeq that makes it more applicable for collaborative text.
- Paths are NOT dependent on each other: it simplifies replication and erases dependence on historical values, meaning
  that the overhead doesn't grow in near-linear fashion to the history of changes.
- Document is persistent by design, build on top of custom LSM store with support for multiple active Write-Ahead Logs.
- Modified prefixed-entry object notation can be used both for Write-Ahead Log, SSTables block storage and as network
  synchronization mechanism.

## References

This project was not invented overnight. It's an evolution of many ideas researched over the years:

- [Larger than memory CRDTs](https://www.bartoszsypytkowski.com/crdt-optimizations#larger-than-memory-crdts) (2021)
  and [RiakDB BigSets](https://syncfree.proj.lip6.fr/attachments/article/59/bigsets-white-paper.pdf) (2016) for general
  document structure.
- [Prefixed Entry Object Notation](https://www.bartoszsypytkowski.com/peon/) (2025) for more detailed approach on the
  document decomposition and possible foundation of storage and network format.
- [Conflict-free Database over Virtual File System](https://www.bartoszsypytkowski.com/conflict-free-database-over-virtual-file-system/) (
  2025) as a basis of multi-WAL implementation.
- [Non-interleaving LSeq](https://www.bartoszsypytkowski.com/non-interleaving-lseq/) (2024) as a viable alternative for
  text CRDT.
- [Hybrid Logical Clocks](https://www.bartoszsypytkowski.com/hybrid-logical-clocks/) (2020) as a reasonable approach for
  Last Write Wins registers using unreliable clocks.
- [MinMax Indexing](https://www.postgresql.org/message-id/20130614222805.GZ5491@eldon.alvh.no-ip.org) (2013) a.k.a. BRIN
  for efficient lookup of LWW registers across SSTable blocks.

## AI Notice

This project is written with a help of AI-generated code. All `.docs` are made by humans, so is the research and ideas,
every code is reviewed by human before being committed to the repository. AI is used mostly for repetitive tasks and all
AI-generated output is committed separately with `ai-gen:` prefixed commit messages.