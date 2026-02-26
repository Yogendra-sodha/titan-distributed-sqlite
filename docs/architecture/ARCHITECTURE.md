# Titan Architecture (V1)

## 1. Problem Statement
Build a distributed durability/availability layer around SQLite while preserving predictable transactional behavior.

## 2. Core Design
**Raft-replicated WAL shipping**
- Leader is the sole writer.
- Writes are represented as ordered WAL-frame batches in Raft log entries.
- Followers apply committed entries in exact sequence.

## 3. Components
1. **Titan Node** (Rust service)
   - Raft state machine
   - Replication/apply engine
   - Snapshot manager
2. **SQLite Adapter**
   - Controlled write path
   - WAL frame capture and validation
3. **RPC Layer (gRPC)**
   - cluster control + health
   - write routing to leader
4. **Storage Layer**
   - local SQLite DB
   - durable Raft log
   - snapshot artifacts

## 4. Data Flow
### Write flow
1) Client submits write to leader
2) Leader executes and captures WAL frames
3) Leader appends log entry and replicates to quorum
4) Entry commit => ACK to client
5) Followers apply committed WAL frame batch

### Read flow
- Default: leader-only linearizable reads
- Optional: bounded-staleness follower reads

## 5. Operational Model
- 3-node cluster default
- Quorum required for writes
- Snapshot interval by size/entry count
- Health/metrics endpoints for automation
