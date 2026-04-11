"""
Demo script for Titan DB Python Client.

Make sure the Titan cluster is running (3 nodes) before running this:
    cargo run --release --bin titan-node -- run 1 2,3
    cargo run --release --bin titan-node -- run 2 1,3
    cargo run --release --bin titan-node -- run 3 1,2

Then:
    cd clients/python
    pip install requests
    python demo.py
"""

import sys
sys.path.insert(0, ".")
from titan_db import TitanClient, TitanError

def main():
    print("=" * 50)
    print("  Titan Distributed SQLite — Python Client Demo")
    print("=" * 50)

    # Connect
    db = TitanClient([
        "http://127.0.0.1:8001",
        "http://127.0.0.1:8002",
        "http://127.0.0.1:8003",
    ])

    # 1. Cluster status
    print("\n--- Cluster Status ---")
    for node in db.status():
        if "error" in node:
            print(f"  Node: OFFLINE ({node.get('url', '?')})")
        else:
            print(f"  Node {node['node_id']}: {node['role']} (term={node['term']}, committed={node['commit_index']})")

    # 2. Find the leader
    print("\n--- Leader ---")
    leader = db.leader()
    print(f"  Leader is Node {leader['node_id']} on port {leader['http_port']}")

    # 3. Create table
    print("\n--- Create Table ---")
    result = db.execute("CREATE TABLE IF NOT EXISTS py_demo (id INTEGER PRIMARY KEY, name TEXT, score REAL)")
    print(f"  {result['message'].encode('ascii', 'replace').decode()}")

    # 4. Insert data
    print("\n--- Insert Data ---")
    names = [("Alice", 95.5), ("Bob", 87.3), ("Charlie", 92.1), ("Diana", 98.7)]
    for name, score in names:
        result = db.execute(f"INSERT INTO py_demo (name, score) VALUES ('{name}', {score})")
        print(f"  Inserted {name}: log_index={result.get('log_index')}")

    # 5. Query from different nodes
    print("\n--- Query (from any node) ---")
    rows = db.query("SELECT * FROM py_demo ORDER BY score DESC")
    print(f"  Got {len(rows)} rows:")
    for row in rows:
        print(f"    #{row['id']} {row['name']}: {row['score']}")

    # 6. Query from a specific node
    print("\n--- Query from Node 3 specifically ---")
    rows = db.query("SELECT name, score FROM py_demo WHERE score > 90", node_url="http://127.0.0.1:8003")
    print(f"  {len(rows)} students scored above 90:")
    for row in rows:
        print(f"    {row['name']}: {row['score']}")

    print("\n" + "=" * 50)
    print("  Demo complete! Data is replicated across all 3 nodes.")
    print("=" * 50)

if __name__ == "__main__":
    try:
        main()
    except TitanError as e:
        print(f"\nError: {e}")
        print("Make sure the Titan cluster is running first!")
