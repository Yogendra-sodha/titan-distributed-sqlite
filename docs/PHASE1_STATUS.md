# Phase 1 Status

## Completed
- Created Rust workspace skeleton and crate boundaries.
- Added `titan-node` bootstrap executable.
- Added initial module stubs for:
  - `sqlite-adapter`
  - `wal-replicator`
  - `raft-core`
  - `snapshotter`
  - `rpc-api`
  - `observability`
- Implemented `sqlite-adapter` with `rusqlite` and basic write/read APIs.
- Added unit test for create/write/read flow in `sqlite-adapter`.
- Implemented Phase-1 WAL batch parser harness in `wal-replicator`.
- Added parser unit tests and integration fixture (`tests/integration/wal_batch_fixture.txt`).

## Current Blocker
No hard blocker right now.

## Newly Completed
- Rust build/test toolchain validated under VS Build Tools environment.
- `cargo test` passes across workspace.
- Added binary SQLite WAL parser scaffold:
  - WAL header parsing
  - frame header parsing
  - basic size/salt validation
- Added `titan-node` CLI mode:
  - `parse-fixture <path>` for fixture parsing/replay harness.

## Next immediate steps
1. Replace synthetic WAL construction with real captured WAL fixture files.
2. Add checksum rolling validation according to SQLite WAL spec.
3. Start Phase-2 Raft integration in `raft-core`.
4. Add a local 3-node docker-compose skeleton for upcoming Raft tests.
