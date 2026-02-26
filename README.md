# Titan Distributed SQLite Engine

Titan is a local-first, fault-tolerant distributed SQLite system using a Raft-based replication layer over SQLite WAL semantics.

## Vision
- Keep SQLite developer ergonomics
- Provide HA with single-writer/multi-replica consistency
- Enable deterministic failover and recovery

## V1 Scope
- Single-writer (leader), multi-replica
- Raft log replication
- WAL frame replication/apply ordering
- Snapshot + log compaction
- Basic cluster ops and health endpoints

## Non-goals (V1)
- Multi-writer CRDT conflict resolution
- Multi-region geo-distributed consensus
- Full SQLite wire protocol replacement

## Architecture (docs)
- `docs/architecture/ARCHITECTURE.md`
- `docs/architecture/CONSISTENCY.md`
- `docs/architecture/FAILURE_MODES.md`

## Execution
- `docs/ROADMAP_90_DAYS.md`
- `docs/TICKETS_MVP.md`

## Planned Repo Layout

```text
cmd/
  titan-node/
crates/
  raft-core/
  wal-replicator/
  sqlite-adapter/
  snapshotter/
  rpc-api/
  observability/
tests/
  integration/
  chaos/
  perf/
deploy/
  docker/
```

## Next Milestone
Initialize Rust workspace and implement a single-node WAL extraction + replay harness.
