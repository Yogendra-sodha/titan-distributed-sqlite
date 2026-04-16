# Titan DB — Python Client

> **Zero-setup distributed SQLite.** Install with pip, start a fault-tolerant cluster with one command.

## Install

```bash
pip install titan-db
```

## Start the Server (One Command!)

```bash
titan-server start
```

This automatically:
1. Downloads the correct `titan-node` binary for your OS (Windows/Mac/Linux)
2. Starts a 3-node distributed cluster on localhost
3. Prints the connection URLs

To stop the cluster:
```bash
titan-server stop
```

To check health:
```bash
titan-server status
```

## Use the Client

```python
from titan_db import TitanClient

# Connect to the cluster
db = TitanClient([
    "http://127.0.0.1:8001",
    "http://127.0.0.1:8002",
    "http://127.0.0.1:8003",
])

# Write — automatically finds and routes to the Leader node
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
db.execute("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')")

# Read — queries any reachable node
rows = db.query("SELECT * FROM users")
print(rows)
# [{'id': '1', 'name': 'Alice', 'email': 'alice@example.com'}]

# Transactions — multiple statements, atomic
db.transaction([
    "INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')",
    "INSERT INTO users (name, email) VALUES ('Charlie', 'charlie@example.com')",
])

# Cluster health
print(db.status())
print(db.leader())
```

## Start the Server from Python

You can also manage the cluster programmatically:

```python
from titan_db.server import TitanServer
from titan_db import TitanClient

# Use as a context manager — auto starts and stops
with TitanServer() as server:
    db = TitanClient(server.node_urls())
    db.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
    db.execute("INSERT INTO test (value) VALUES ('hello distributed world!')")
    print(db.query("SELECT * FROM test"))
# Cluster automatically stops when the block exits
```

## Authentication

If the server is started with `TITAN_API_KEY`, pass it to the client:

```python
db = TitanClient(
    nodes=["http://127.0.0.1:8001", "http://127.0.0.1:8002", "http://127.0.0.1:8003"],
    api_key="my_secret_key"
)
```

## API Reference

| Method | Description |
|--------|-------------|
| `execute(sql)` | Write SQL — auto-routes to Leader via Raft |
| `query(sql)` | Read SQL — returns list of dicts |
| `transaction([...])` | Atomic multi-statement transaction |
| `status()` | All nodes health |
| `leader()` | Current leader info |

## CLI Reference

| Command | Description |
|---------|-------------|
| `titan-server start` | Download binary + start 3-node cluster |
| `titan-server stop` | Stop all running nodes |
| `titan-server status` | Check cluster health |
| `titan-server download` | Just download the binary (no start) |

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| `No Titan nodes are reachable` | Cluster not running | Run `titan-server start` |
| `Nodes reachable but no Leader` | Election in progress | Wait 1-2 seconds, retry |
| `Binary not found for your platform` | No pre-built binary | Build from source (see error message) |
| `Unauthorized` | API key required | Pass `api_key=` to TitanClient |

## Links

- **Repository**: [github.com/Yogendra-sodha/titan-distributed-sqlite](https://github.com/Yogendra-sodha/titan-distributed-sqlite)
- **Dashboard**: Open `http://127.0.0.1:8001/` when the cluster is running
