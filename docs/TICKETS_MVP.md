# MVP Tickets (Initial 12)

## T-001: Rust workspace scaffold
**Acceptance:** cargo workspace builds with empty crates.

## T-002: sqlite-adapter crate with write API contract
**Acceptance:** adapter exposes `execute_write()` and `execute_read()` interfaces.

## T-003: WAL frame parser abstraction
**Acceptance:** parse and validate frame metadata + checksum.

## T-004: replay harness for WAL batches
**Acceptance:** deterministic replay test passes with fixture data.

## T-005: raft-core bootstrap
**Acceptance:** node starts with persistent term/index state.

## T-006: append entries pipeline
**Acceptance:** follower receives and persists entries.

## T-007: commit/apply loop
**Acceptance:** committed entries applied in-order exactly once.

## T-008: leader-only write routing
**Acceptance:** non-leader write request redirects/rejects with leader hint.

## T-009: 3-node integration test
**Acceptance:** replicated write visible on all nodes after commit.

## T-010: leader failover test
**Acceptance:** old leader down, new leader elected, writes continue.

## T-011: snapshot create/install
**Acceptance:** lagging node catches up using snapshot path.

## T-012: observability baseline
**Acceptance:** `/healthz` and `/metrics` expose Raft + apply lag metrics.
