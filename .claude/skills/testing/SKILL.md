---
name: testing
description: Guiding principles for writing tests
---
# Testing

Guiding principles:

- EVERY functional change should have a corresponding test
- Test which are related to on-disk persistence, should use `tempfile` crate to create an isolated temporary directory which will cleanup itself after test is finished.
- No test should execute for longer than a minute.
- Integration tests live in `./tests` folder of the repository.