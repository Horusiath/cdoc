# Transactions

CDoc supports read-only (RO) and read-write (RW) transactions. The rule is that RO transactions don't block RW ones
and vice versa.

RW transactions operate on local buffer of uncommited changes, that's a fork of `MemTable` internal tree. At the
current moment we don't consider MVCC model in a traditional sense, however we can consider using it in the future.

When committing, each transaction is assigned a HLC timestamp. All of the updates made by this transaction are using
this timestamp for potential conflict resolution.