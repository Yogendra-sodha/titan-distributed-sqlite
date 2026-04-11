"""
Titan DB Real-World Example: A High-Availability To-Do API

This is a standard Flask web server API.
Instead of pointing to a generic local SQLite file, or a heavy PostgreSQL database,
we connect it to our 3-node Titan cluster.

If one of the Titan nodes dies while this Flask app is running, 
TitanClient automatically routes the data to the new leader!
"""

from flask import Flask, request, jsonify
import sys
import os

# (We are appending this path just so you can run it from the repo folder without installing)
sys.path.append("../../clients/python")
from titan_db import TitanClient, TitanError

app = Flask(__name__)

# Connect to the 3-node local cluster. 
db = TitanClient(
    nodes=[
        "http://127.0.0.1:8001",
        "http://127.0.0.1:8002",
        "http://127.0.0.1:8003"
    ],
    timeout=5.0
)

def setup_database():
    """Ensure our table schema exists before the app starts."""
    try:
        # We auto-route this to the leader.
        db.execute("""
            CREATE TABLE IF NOT EXISTS todos (
                id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                completed INTEGER DEFAULT 0
            )
        """)
        print("✅ Database schema initialized via Raft Consensus")
    except TitanError as e:
        print(f"Failed to reach cluster: {e}")
        print("Check if you are running the 3 nodes!")

@app.route("/todos", methods=["GET"])
def get_todos():
    """Read data - Titan queries from ANY of the 3 nodes instantly."""
    try:
        rows = db.query("SELECT * FROM todos ORDER BY id DESC")
        return jsonify({"success": True, "todos": rows})
    except TitanError as e:
        return jsonify({"success": False, "error": str(e)}), 500

@app.route("/todos", methods=["POST"])
def add_todo():
    """Write data - Titan automatically forwards this to the current Leader."""
    data = request.get_json()
    title = data.get("title")

    if not title:
        return jsonify({"success": False, "error": "Title required"}), 400

    try:
        # Note: In production you should sanitize inputs!
        res = db.execute(f"INSERT INTO todos (title, completed) VALUES ('{title}', 0)")
        return jsonify({"success": True, "message": "To-Do created!", "log_index": res.get("log_index")})
    except TitanError as e:
        return jsonify({"success": False, "error": str(e)}), 500

@app.route("/todos/<int:todo_id>/complete", methods=["POST"])
def complete_todo(todo_id):
    """Update data via the Leader."""
    try:
        db.execute(f"UPDATE todos SET completed = 1 WHERE id = {todo_id}")
        return jsonify({"success": True, "message": f"To-Do {todo_id} marked complete."})
    except TitanError as e:
        return jsonify({"success": False, "error": str(e)}), 500


if __name__ == "__main__":
    # Setup our schema once before listening
    setup_database()
    print("🚀 Flask app starting on http://127.0.0.1:5050")
    print("Testing locally? Try running: curl -X POST http://127.0.0.1:5050/todos -H 'Content-Type: application/json' -d '{\"title\": \"Buy Milk\"}'")
    
    app.run(port=5050, debug=True, use_reloader=False)
