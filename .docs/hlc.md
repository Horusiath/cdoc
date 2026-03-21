# Hybrid Logical Clocks

Hybrid Logical Clocks offer a desirable middle ground between wall clock timestamps (that we can use to refer to i.e.
in reports) and incremental sequencers that are used for things like ordering.

The major problem with regular system clocks is that they can be subject of skews and leap seconds, which means that
they don't always progress incrementally through time. Hybrid logical clocks (HLC) address that concern by splitting
the timestamp - represented as `u64` - into two parts:

- A regular UNIX milliseconds timestamp upper 48 bits remain without change. This way we can operate on real time,
  while still having a sub-second precision.
- The lowest 16 bits are masked and incremented in sequential, monotonic manner within the time period bounded by
  the upper 48bits.

Additionally, to we use a global atomic counter to keep track of the latest timestamp value and to make sure that it
won't drop down - which can happen during leap seconds. Ultimately the HLC timestamp is a maximum between system
timestamp (masked) and latest counter value, incremented by 1.