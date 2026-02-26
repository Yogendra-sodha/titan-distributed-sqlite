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
- Added `titan-node` CLI modes:
  - `parse-fixture <path>` for fixture parsing/replay harness.
  - `generate-wal-fixture <path>` to create realistic WAL fixtures from SQLite writes.
- Implemented `SqliteAdapter::generate_wal_fixture()` and test coverage.
- Began Phase-2 bootstrap in `raft-core` with role transitions, local log append, and baseline tests.

## Next immediate steps
1. Add strict rolling checksum validation according to SQLite WAL spec.
2. Add replay/validation integration test using generated real WAL fixture in CI path.
3. Expand `raft-core` with persistent state interface (term/vote/log store abstraction).
4. Add local 3-node docker-compose skeleton for upcoming Raft integration tests.
