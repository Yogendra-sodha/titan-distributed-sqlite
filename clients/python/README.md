# Titan DB — Python Client

Python client for [Titan Distributed SQLite](https://github.com/Yogendra-sodha/titan-distributed-sqlite).

## Install

```bash
pip install -e .
```

Or just copy `titan_db.py` into your project.

## Usage

```python
from titan_db import TitanClient

# Connect to the cluster
db = TitanClient([
    "http://127.0.0.1:8001",
    "http://127.0.0.1:8002",
    "http://127.0.0.1:8003",
])

# Write (auto-discovers and sends to the Leader)
db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
db.execute("INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')")
db.execute("INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')")

# Read (goes to any reachable node)
rows = db.query("SELECT * FROM users")
for row in rows:
    print(row)
# {'id': '1', 'name': 'Alice', 'email': 'alice@example.com'}
# {'id': '2', 'name': 'Bob', 'email': 'bob@example.com'}

# Cluster status
print(db.status())
print(db.leader())
```

## API

| Method | Description |
|--------|-------------|
| `execute(sql)` | Write SQL — auto-routes to Leader via Raft |
| `query(sql)` | Read SQL — returns list of dicts |
| `status()` | Get status of all nodes |
| `leader()` | Get current leader info |
