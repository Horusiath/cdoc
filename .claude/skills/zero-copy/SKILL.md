---
name: testing
description: Patterns for implementing zero-copy structs
---

# Zero-copy struct design

Use `zerocopy` crate for types that we're going to serialize/deserialize directly to disk with no memory copy overhead.

## Example

```rust
#[repr(C)]
#[derive(FromBytes, KnownLayout, Immutable, IntoBytes)]
struct MyStruct {
    // other fields
}

impl MyStruct {
    pub fn parse(bytes: &[u8]) -> crate::Result<&Self> {
        OK(Self::ref_from_bytes(bytes)?)
    }
}
```

- We prefer Big Endian encoding when the lexical order is desired: as a shortcut use `crate::U16`, `crate::U32`, 
  `crate::U64` and `crate::U128`.
- Use `zerocopy::FromBytes` when performing simple conversions. 
- Use `zerocopy::TryFromBytes` to prevent illegals states at the type level.