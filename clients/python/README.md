# Titan DB — Python Client

> **This is a client-only library.** It connects to an already-running [Titan Distributed SQLite](https://github.com/Yogendra-sodha/titan-distributed-sqlite) server cluster over HTTP. You must start the server separately before using this client.

## Install

```bash
pip install titan-db
```

## Prerequisites — Start the Server First!

Before using this client, you need the Titan server cluster running. Here's how:

```bash
# 1. Clone and build the server (requires Rust)
git clone https://github.com/Yogendra-sodha/titan-distributed-sqlite.git
cd titan-distributed-sqlite
cargo build --release

# 2. Start 3 nodes (each in a separate terminal)
cargo run --release --bin titan-node -- run 1 2,3   # Terminal 1
cargo run --release --bin titan-node -- run 2 1,3   # Terminal 2
cargo run --release --bin titan-node -- run 3 1,2   # Terminal 3
```

Once you see `🌐 HTTP API live at http://127.0.0.1:8001` in each terminal, the cluster is ready.

## Usage

```python
from titan_db import TitanClient

# Connect to the 3-node cluster
db = TitanClient([
    "http://127.0.0.1:8001",
    "http://127.0.0.1:8002",
    "http://127.0.0.1:8003",
])

# Write — automatically finds and routes to the Leader node
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
db.execute("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')")
db.execute("INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')")

# Read — queries any reachable node
rows = db.query("SELECT * FROM users")
for row in rows:
    print(row)
# {'id': '1', 'name': 'Alice', 'email': 'alice@example.com'}
# {'id': '2', 'name': 'Bob', 'email': 'bob@example.com'}

# Transactions — multiple statements, atomic
db.transaction([
    "INSERT INTO users (name, email) VALUES ('Charlie', 'charlie@test.com')",
    "INSERT INTO users (name, email) VALUES ('Diana', 'diana@test.com')",
])

# Authentication (if server has TITAN_API_KEY set)
db = TitanClient(
    nodes=["http://127.0.0.1:8001", "http://127.0.0.1:8002", "http://127.0.0.1:8003"],
    api_key="my_secret_key"
)

# Cluster health
print(db.status())     # Status of all nodes
print(db.leader())     # Current leader info
```

## API Reference

| Method | Description |
|--------|-------------|
| `execute(sql)` | Write SQL — auto-routes to the Leader via Raft consensus |
| `query(sql)` | Read SQL — returns list of dicts from any reachable node |
| `transaction([sql1, sql2, ...])` | Atomic multi-statement transaction via Raft |
| `status()` | Get status of all nodes |
| `leader()` | Get current leader info |

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `No Titan nodes are reachable` | Server cluster is not running | Start the 3 Titan nodes first (see Prerequisites above) |
| `Nodes are reachable but no Leader elected` | Election in progress | Wait 1-2 seconds and retry. Ensure at least 2 of 3 nodes are running. |
| `Unauthorized: Check your API key` | Server requires auth | Pass `api_key="..."` to `TitanClient()` |
| `Connection failed to ...` | A node crashed mid-request | Client will auto-retry with the new leader on next call |

## Links

- **Server Repository**: [github.com/Yogendra-sodha/titan-distributed-sqlite](https://github.com/Yogendra-sodha/titan-distributed-sqlite)
- **Dashboard**: Open `http://127.0.0.1:8001/` in your browser when the cluster is running
