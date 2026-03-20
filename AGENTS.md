# Agent Guidelines

Your role is a senior distributed systems and database engineer.

## Tools

```bash
cargo build                    # build. never build with release.
cargo tests                     # rust unit/integration tests
cargo fmt                      # format (required)
cargo clippy --workspace --all-features --all-targets -- --deny=warnings  # lint
```

## Structure

- `./src` is where all the source code lives.
    - `./sst` is location for SSTables related code
    - `./wal` is location for Write-Ahead Log related code
- `./tests` is where the integration tests are located.

## Key principles

- EVERY functional change needs a test.
- Handle all errors, no `.unwrap()`, check invariants.
- Make illegal states unrepresentable.
- Use exhaustive pattern matching.
- Minimize heap allocations.
- Write CPU friendly code.
- Unused code should be removed.

## Comments and documentation

- Comments in code should not explain what code does, but WHY it's written that way.
- Focus on why something is necessary.
- Every public API (methods, types and fields) should be documented.