# Titan Roadmap

## Phase 1: Foundations
- Create Rust workspace and crate boundaries
- Implement single-node SQLite adapter shell
- WAL extraction and replay harness
- Baseline unit tests + CI

## Phase 2: Raft Core Integration
- Add Raft node bootstrap and persistent log
- Leader election and append entries
- Deterministic apply loop
- 3-node local integration test

## Phase 3: Replication Correctness
- Quorum commit semantics
- Idempotent follower apply
- Leader failover + recovery tests
- Read linearizability guardrails

## Phase 4: Snapshot + Compaction
- Snapshot creation/install
- Log truncation safety
- Lagging follower catch-up path

## Phase 5: Hardening + Demo
- Metrics and health endpoints
- Chaos tests (kill, partition)
- Performance baseline run
- MVP demo script and docs
