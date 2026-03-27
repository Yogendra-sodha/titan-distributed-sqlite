# ⚡ Titan — Distributed SQLite

A distributed SQLite database built from scratch using the **Raft consensus protocol**, written in Rust. Titan replicates SQL writes across a cluster of nodes, providing fault tolerance, automatic leader election, and seamless failover — all while using SQLite as the local storage engine.

> **Think of it as:** SQLite + Raft = a database that survives server failures.

![Rust](https://img.shields.io/badge/Rust-000000?style=flat&logo=rust&logoColor=white)
![SQLite](https://img.shields.io/badge/SQLite-003B57?style=flat&logo=sqlite&logoColor=white)
![Raft](https://img.shields.io/badge/Consensus-Raft-blue)
![License](https://img.shields.io/badge/License-MIT-green)

---

## 🏗️ Architecture

```
                    ┌─────────────────────┐
  Client ──HTTP──▶  │   Titan Node 1      │
                    │   Role: LEADER       │
                    │   ┌─── Raft Core ──┐ │
                    │   │ Election Timer  │ │
                    │   │ Log Replication │ │
                    │   └─────┬──────────┘ │
                    │         │            │
                    │   ┌─────▼──────────┐ │
                    │   │  SQLite (WAL)  │ │
                    │   └────────────────┘ │
                    └──────┬────┬──────────┘
                      UDP  │    │  UDP
              ┌────────────┘    └────────────┐
              ▼                              ▼
  ┌─────────────────────┐      ┌─────────────────────┐
  │   Titan Node 2      │      │   Titan Node 3      │
  │   Role: FOLLOWER    │      │   Role: FOLLOWER    │
  │   ┌─── Raft Core ──┐│      │   ┌─── Raft Core ──┐│
  │   │ Replicates logs ││      │   │ Replicates logs ││
  │   └─────┬───────────┘│      │   └─────┬───────────┘│
  │   ┌─────▼───────────┐│      │   ┌─────▼───────────┐│
  │   │  SQLite (WAL)   ││      │   │  SQLite (WAL)   ││
  │   └─────────────────┘│      │   └─────────────────┘│
  └──────────────────────┘      └──────────────────────┘
```

## ✨ Features

- **Raft Consensus** — Full implementation: leader election, log replication, term management
- **Automatic Failover** — Kill the leader, a new one is elected in < 1 second
- **Split-Brain Safety** — Partitioned leader's uncommitted writes are safely discarded
- **HTTP REST API** — `POST /execute` for writes, `GET /query` for reads, `GET /status` for health
- **Web Dashboard** — Real-time cluster monitoring UI at `http://127.0.0.1:8001/`
- **SQLite WAL Mode** — Non-blocking reads during write replication
- **WAL Binary Parser** — Parse raw SQLite WAL files for frame-level replication
- **Embedded Library** — Use Titan as a library inside your own Rust backend

## 📦 Project Structure

```
titan-distributed-sqlite/
├── cmd/
│   └── titan-node/          # Main binary — runs a Titan cluster node
│       ├── src/main.rs       # Server: UDP Raft loop + HTTP API + Dashboard
│       └── static/           # Web dashboard files
├── crates/
│   ├── raft-core/            # Raft consensus: election, replication, failover
│   ├── sqlite-adapter/       # SQLite read/write with WAL mode
│   ├── wal-replicator/       # Binary WAL parser
│   ├── snapshotter/          # Snapshot management
│   ├── rpc-api/              # Health reporting / RPC types
│   └── observability/        # Metrics and tracing
├── examples/
│   └── embedded-app/         # Example: embed Titan into an Axum web server
└── tests/
    └── integration/          # Integration test fixtures
```

## 🚀 Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.70+)
- No other dependencies — SQLite is bundled via `rusqlite`

### Build

```bash
cargo build --release
```

### Run a 3-Node Cluster

Open **3 terminals** and start each node:

```bash
# Terminal 1 — Node 1
cargo run --release --bin titan-node -- run 1 2,3

# Terminal 2 — Node 2
cargo run --release --bin titan-node -- run 2 1,3

# Terminal 3 — Node 3
cargo run --release --bin titan-node -- run 3 1,2
```

Node 1 becomes the Leader automatically. You'll see:
```
🚀 Starting Titan Node 1
📡 Peers: [2, 3]
🌐 UDP Raft port: 5001
🌐 HTTP API port: 8001
👑 Node 1 transitioning to Leader
🌐 HTTP API live at http://127.0.0.1:8001
```

### Open the Dashboard

Open **http://127.0.0.1:8001** in your browser to see the real-time cluster dashboard.

### Use the HTTP API

```bash
# Write data (sends through Raft consensus to all nodes)
curl -X POST http://127.0.0.1:8001/execute \
  -H "Content-Type: application/json" \
  -d '{"sql": "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)"}'

curl -X POST http://127.0.0.1:8001/execute \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO users (name, email) VALUES ('"'"'Alice'"'"', '"'"'alice@example.com'"'"')"}'

# Read data (reads from local SQLite — fast, no consensus needed)
curl --get http://127.0.0.1:8002/query --data-urlencode "sql=SELECT * FROM users"

# Check node status
curl http://127.0.0.1:8001/status
curl http://127.0.0.1:8002/status
curl http://127.0.0.1:8003/status
```

### Test Failover

1. Kill Node 1: `Ctrl+C` in Terminal 1
2. Watch Node 2 become the new Leader automatically
3. Send data to Node 2: port `8002`
4. Restart Node 1 — it rejoins as a Follower

### Run Tests

```bash
cargo test --workspace
```

10 tests covering: leader election, log replication, failover, split-brain recovery, SQLite I/O, and WAL parsing.

## 🔒 Safety Guarantees

| Scenario | What Happens |
|----------|-------------|
| **Leader dies** | Remaining nodes detect missing heartbeats, elect a new leader in < 1s |
| **Network partition** | Isolated leader can't commit (needs majority). After healing, its stale entries are overwritten |
| **Node restarts** | Rejoins cluster as Follower, discovers current Leader via term comparison |
| **Simultaneous elections** | Higher term always wins. Split votes trigger re-election with randomized timeouts |

## 🛠️ How It Works

### Raft Consensus (crates/raft-core)

1. **Election**: Nodes start as Followers. If no heartbeat arrives within the timeout, a node becomes a Candidate, increments its term, and requests votes from peers.
2. **Replication**: The Leader accepts client writes, appends them to its log, and replicates to Followers via `AppendEntries` RPCs. An entry is committed once a majority confirms it.
3. **Safety**: The `step()` function checks every incoming message's term. If a node sees a higher term, it immediately steps down to Follower — this prevents split-brain.

### SQLite WAL Mode (crates/sqlite-adapter)

SQLite runs in WAL (Write-Ahead Logging) mode. Writes go to a `-wal` file first, allowing concurrent reads from the main `.db` file without blocking.

### WAL Binary Parser (crates/wal-replicator)

Parses raw SQLite WAL files at the binary level: 32-byte file header → N frames (24-byte frame header + page data). Validates magic numbers and salt values for integrity.

## 📊 Ports

| Node | UDP Port (Raft) | HTTP Port (API) |
|------|----------------|-----------------|
| Node 1 | 5001 | 8001 |
| Node 2 | 5002 | 8002 |
| Node 3 | 5003 | 8003 |

Formula: UDP = `5000 + node_id`, HTTP = `8000 + node_id`

## 📄 License

MIT
