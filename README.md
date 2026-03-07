# Titan Distributed SQLite Engine

Titan is a local-first, fault-tolerant distributed SQLite system using a Raft-based replication layer over SQLite WAL semantics. It transforms a standard, single-node SQLite database into a highly available clustered database by intercepting and replicating binary Write-Ahead Log (WAL) frames across a network of nodes.

## Vision and Capabilities
- **Keep SQLite developer ergonomics**: Use standard SQL.
- **Provide High Availability**: Single-writer/multi-replica consistency using the Raft consensus algorithm.
- **Zero Downtime Failover**: Deterministic leader election and split-brain resolution through quorums.

## System Architecture

The project is built as a modular Rust workspace consisting of several core components:

### Crates
- **`raft-core`**: The networking heartbeat. Implements the Raft distributed consensus protocol, including leader election, term management, idempotent log append entries, deterministic state machine apply loops, and split-brain/network partition recovery mechanics.
- **`wal-replicator`**: The binary payload parser. Extracts and parses raw SQLite Write-Ahead Log (`-wal`) binary files, chunking them into discrete replication frames that can be shipped over the network.
- **`sqlite-adapter`**: The database wrapper. Forcibly configures `rusqlite` connections into `PRAGMA journal_mode=WAL` and executes read/write commands safely.
- **`rpc-api`**: The networking interface. Exposes health checks and cluster state endpoints.
- **`observability`**: The metrics layer. Tracks thread-safe atomic metrics like `ops_applied` and `leader_changes`.
- **`snapshotter`**: The disk archival system. Compresses the Raft log state into point-in-time binary snapshots to prevent infinite log growth.

### Executable
- **`cmd/titan-node`**: The main executable binary that spins up a Titan cluster node.

## How to Test and Run

The system is rigorously tested through simulated clustered environments.

### Run the Raft Chaos Tests
Prove that the clustering logic handles catastrophic network partitions and repairs its own corrupted data dynamically without downtime:
```bash
cargo test -p raft-core test_network_partition_chaos -- --nocapture
```

### Run the Full Workspace Suite
Run all unit tests across the SQLite binary parsers and the Raft consensus engines:
```bash
cargo test --workspace
```

### Run the Node Binary (WAL Parsing)
You can use the CLI to dynamically generate a dummy SQLite database, force it to dump a binary `-wal` file to your disk, and parse it back into Rust metadata:
```bash
cargo run -p titan-node -- generate-wal-fixture test.wal
cargo run -p titan-node -- parse-fixture test.wal
```
