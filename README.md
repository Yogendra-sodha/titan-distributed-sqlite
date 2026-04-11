# Titan Distributed SQLite 🚀

Titan is a distributed, highly-available SQLite database built in Rust. It wraps a standard SQLite database with the **Raft Consensus Algorithm**, allowing you to run a resilient 3-node cluster. If one node dies, the others seamlessly take over with zero data loss.

This is a complete, production-hardened system with a built-in Mission Control dashboard, an HTTP API, TLS support, Authentication, and a Python client.

---

## 🌟 Key Features

*   **Raft Consensus**: Guarantees strong consistency on writes. Built-in leader election and failover.
*   **Zero Data Loss Recovery**: All Raft state (Term, VotedFor, Log) persists to disk. Nodes seamlessly rejoin after a crash.
*   **Multi-Table Transactions**: Execute `BEGIN TRANSACTION`/`COMMIT` atomically across the cluster. 
*   **Security & Auth**: Optional API Key Authentication (`TITAN_API_KEY`) and Auto-generating HTTPS/TLS (`TITAN_TLS=1`).
*   **SQL Validation**: Catch syntax errors cleanly before they get replicated.
*   **Web Dashboard**: A beautiful, real-time "Mission Control" UI on `http://127.0.0.1:8001/`.
*   **Python Client**: A `pip`-installable library for programmatic access.

---

## 🛠️ Quick Start: Running a Local Cluster

To run a 3-node cluster on your local machine, open 3 separate terminals.

**Terminal 1 (Node 1):**
```bash
cargo run --release --bin titan-node -- run 1 2,3
```

**Terminal 2 (Node 2):**
```bash
cargo run --release --bin titan-node -- run 2 1,3
```

**Terminal 3 (Node 3):**
```bash
cargo run --release --bin titan-node -- run 3 1,2
```

Once running, navigate to **http://127.0.0.1:8001/** in your browser to view the Mission Control Dashboard. You can type SQL directly into the dashboard and watch it replicate!

---

## 🔒 Production Capabilities (Auth & TLS)

Titan now ships with production hardening. You can enable them via environment variables:

**1. API Key Authentication**
Start the nodes with this environment variable:
```bash
export TITAN_API_KEY="my_secret_key"
```
When enabled, requests to the cluster must include `Authorization: Bearer my_secret_key` or `?api_key=my_secret_key`.

**2. HTTPS / TLS Encryption**
Start the nodes with this environment variable:
```bash
export TITAN_TLS=1
```
Titan will automatically generate a self-signed SSL Certificate on its first run (`data/cert.pem`) and upgraded the API to strictly use `https://`.

---

## 🐍 Python Client Library

You can interact with your cluster directly from Python using the officially supported client.

### Installation
The client is available directly from this GitHub repository.
```bash
pip install git+https://github.com/Yogendra-sodha/titan-distributed-sqlite.git#subdirectory=clients/python
```

### Usage

```python
from titan_db import TitanClient

# Connect to the cluster
db = TitanClient(
    nodes=["http://127.0.0.1:8001", "http://127.0.0.1:8002", "http://127.0.0.1:8003"],
    api_key="my_secret_key"  # Leave empty if Auth is disabled
)

# 1. Single Statement (Automatically routes to the Leader!)
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
db.execute("INSERT INTO users (name) VALUES ('Alice')")

# 2. Querying (Can hit any node!)
rows = db.query("SELECT * FROM users")
print(rows)  # [{'id': '1', 'name': 'Alice'}]

# 3. Transactions (Atomic multiple statements)
db.transaction([
    "INSERT INTO users (name) VALUES ('Bob')",
    "INSERT INTO users (name) VALUES ('Charlie')"
])

# 4. Status Check
print(db.status())
```

---

## 📡 Raw HTTP API

If you aren't using the Python client, interacting with Titan over HTTP is extremely easy.

### Cluster Status (`GET /status`)
Returns current node status, Raft term, and commit index.
```bash
curl http://127.0.0.1:8001/status
```

### Execute SQL Write (`POST /execute`)
Send writes to the current **Leader** node. The Leader replicates it through Raft.
```bash
curl -X POST http://127.0.0.1:8002/execute \
  -H "Content-Type: application/json" \
  -d '{"sql": "INSERT INTO demo (msg) VALUES (''hello world'')"}'
```

### Read SQL (`GET /query`)
You can query from **any** node safely.
```bash
curl -G http://127.0.0.1:8001/query --data-urlencode "sql=SELECT * FROM demo"
```

### Transactions (`POST /transaction`)
Execute multiple SQL statements sequentially in one atomic pass.
```bash
curl -X POST http://127.0.0.1:8002/transaction \
  -H "Content-Type: application/json" \
  -d '{"statements": ["INSERT INTO demo (msg) VALUES (''A'')", "INSERT INTO demo (msg) VALUES (''B'')"]}'
```

---

## 📂 Architecture & Code Structure

*   `crates/raft-core`: The consensus engine. Handles Leader elections, term increments, network split-brain prevention, and robust disk persistence.
*   `crates/sqlite-adapter`: Connects the state machine to local `.db` files via `rusqlite`.
*   `cmd/titan-node`: The runtime executable wrapping Raft in a UDP thread and the HTTP API in a Tokio thread.
*   `data/`: Where persistent files rest (`titan_node_1.db` for actual data and `titan_node_1_raft.db` for consensus data).
