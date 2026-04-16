"""Titan DB — Python client for Titan Distributed SQLite.

IMPORTANT: This is a CLIENT LIBRARY only. You must have a Titan cluster
running separately before using this client. titan-db connects to an
already-running Titan Distributed SQLite server over HTTP.

Quick Start:
    1. Clone and build the server:
       git clone https://github.com/Yogendra-sodha/titan-distributed-sqlite.git
       cd titan-distributed-sqlite
       cargo build --release

    2. Start a 3-node cluster (in 3 separate terminals):
       cargo run --release --bin titan-node -- run 1 2,3
       cargo run --release --bin titan-node -- run 2 1,3
       cargo run --release --bin titan-node -- run 3 1,2

    3. Now use this client:
       from titan_db import TitanClient
       db = TitanClient(["http://127.0.0.1:8001", "http://127.0.0.1:8002", "http://127.0.0.1:8003"])
       db.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
       db.execute("INSERT INTO users (name) VALUES ('Alice')")
       print(db.query("SELECT * FROM users"))

Server repo: https://github.com/Yogendra-sodha/titan-distributed-sqlite
"""

import requests
from typing import List, Dict, Optional, Any


class TitanError(Exception):
    """Raised when a Titan operation fails."""
    pass


class TitanClient:
    """Client for interacting with a Titan Distributed SQLite cluster.

    The client auto-discovers the Leader for writes and can read from any node.

    Args:
        nodes: List of node URLs, e.g. ["http://127.0.0.1:8001", "http://127.0.0.1:8002"]
        timeout: Request timeout in seconds (default: 5)
    """

    def __init__(self, nodes: List[str], timeout: float = 5.0, api_key: Optional[str] = None):
        self.nodes = [url.rstrip("/") for url in nodes]
        self.timeout = timeout
        self.api_key = api_key
        self._leader_url: Optional[str] = None
        self._session = requests.Session()
        if self.api_key:
            self._session.headers.update({"Authorization": f"Bearer {self.api_key}"})

    def _find_leader(self) -> str:
        """Poll all nodes to find the current Leader."""
        reachable = []
        unreachable = []
        for url in self.nodes:
            try:
                r = self._session.get(f"{url}/status", timeout=self.timeout)
                data = r.json()
                if data.get("role") == "Leader":
                    self._leader_url = url
                    return url
                reachable.append((url, data.get("role", "Unknown")))
            except (requests.ConnectionError, requests.Timeout):
                unreachable.append(url)
                continue

        # Build a helpful error message
        if not reachable and unreachable:
            raise TitanError(
                f"No Titan nodes are reachable. Tried: {', '.join(unreachable)}\n"
                f"\n"
                f"titan-db is a CLIENT library — you need to start the Titan server first.\n"
                f"\n"
                f"Quick fix:\n"
                f"  1. Clone: git clone https://github.com/Yogendra-sodha/titan-distributed-sqlite.git\n"
                f"  2. Build: cd titan-distributed-sqlite && cargo build --release\n"
                f"  3. Start: cargo run --release --bin titan-node -- run 1 2,3\n"
                f"     (repeat with 'run 2 1,3' and 'run 3 1,2' in separate terminals)\n"
                f"\n"
                f"Docs: https://github.com/Yogendra-sodha/titan-distributed-sqlite#readme"
            )
        elif reachable:
            roles = ', '.join(f"{url} ({role})" for url, role in reachable)
            raise TitanError(
                f"Nodes are reachable but no Leader has been elected yet.\n"
                f"  Reachable nodes: {roles}\n"
                f"  Unreachable nodes: {', '.join(unreachable) if unreachable else 'none'}\n"
                f"\n"
                f"This usually resolves within 1-2 seconds as Raft elects a leader.\n"
                f"Try again shortly, or check that at least 2 of 3 nodes are running."
            )
        else:
            raise TitanError("No nodes configured. Pass node URLs to TitanClient().")

    def _get_leader(self) -> str:
        """Return cached leader URL, or discover it."""
        if self._leader_url:
            try:
                r = self._session.get(f"{self._leader_url}/status", timeout=self.timeout)
                if r.json().get("role") == "Leader":
                    return self._leader_url
            except (requests.ConnectionError, requests.Timeout):
                pass
        return self._find_leader()

    def _get_any_node(self) -> str:
        """Return any reachable node URL."""
        for url in self.nodes:
            try:
                self._session.get(f"{url}/status", timeout=self.timeout)
                return url
            except (requests.ConnectionError, requests.Timeout):
                continue
        raise TitanError(
            f"No Titan nodes are reachable. Tried: {', '.join(self.nodes)}\n"
            f"\n"
            f"titan-db is a CLIENT library — you need to start the Titan server first.\n"
            f"See: https://github.com/Yogendra-sodha/titan-distributed-sqlite#readme"
        )

    def execute(self, sql: str) -> Dict[str, Any]:
        """Execute a write SQL statement (INSERT, CREATE, UPDATE, DELETE)."""
        leader = self._get_leader()
        try:
            r = self._session.post(
                f"{leader}/execute",
                json={"sql": sql},
                timeout=self.timeout,
            )
            data = r.json()
            if not data.get("success"):
                if r.status_code == 401:
                    raise TitanError("Unauthorized: Check your API key")
                # Leader may have changed — retry once
                self._leader_url = None
                leader = self._find_leader()
                r = self._session.post(
                    f"{leader}/execute",
                    json={"sql": sql},
                    timeout=self.timeout,
                )
                data = r.json()
                if not data.get("success"):
                    raise TitanError(data.get("message", "Unknown error"))
            return data
        except requests.ConnectionError:
            self._leader_url = None
            raise TitanError(f"Connection failed to {leader}")

    def transaction(self, statements: List[str]) -> Dict[str, Any]:
        """Execute multiple SQL statements sequentially in a single atomic transaction."""
        leader = self._get_leader()
        try:
            r = self._session.post(
                f"{leader}/transaction",
                json={"statements": statements},
                timeout=self.timeout,
            )
            data = r.json()
            if not data.get("success"):
                if r.status_code == 401:
                    raise TitanError("Unauthorized: Check your API key")
                raise TitanError(data.get("message", "Transaction failed"))
            return data
        except requests.ConnectionError:
            self._leader_url = None
            raise TitanError(f"Connection failed to {leader}")

    def query(self, sql: str, node_url: Optional[str] = None) -> List[Dict[str, str]]:
        """Execute a read SQL query and return results as a list of dicts."""
        url = node_url or self._get_any_node()
        try:
            r = self._session.get(
                f"{url}/query",
                params={"sql": sql},
                timeout=self.timeout,
            )
            data = r.json()
            if not data.get("success"):
                if r.status_code == 401:
                    raise TitanError("Unauthorized: Check your API key")
                raise TitanError(data.get("error", "Query failed"))

            columns = data.get("columns", [])
            rows = data.get("rows", [])

            if not columns:
                return [dict(enumerate(row)) for row in rows]

            return [dict(zip(columns, row)) for row in rows]
        except requests.ConnectionError:
            raise TitanError(f"Connection failed to {url}")

    def status(self, node_url: Optional[str] = None) -> Any:
        """Get status of a specific node or all nodes.

        Args:
            node_url: If provided, get status of that specific node.
                      If None, get status of all nodes.

        Returns:
            A single status dict (if node_url given) or list of status dicts.
        """
        if node_url:
            r = requests.get(f"{node_url}/status", timeout=self.timeout)
            return r.json()

        results = []
        for url in self.nodes:
            try:
                r = requests.get(f"{url}/status", timeout=self.timeout)
                results.append(r.json())
            except (requests.ConnectionError, requests.Timeout):
                results.append({"url": url, "role": "Offline", "error": "unreachable"})
        return results

    def leader(self) -> Dict[str, Any]:
        """Get the current leader's status.

        Returns:
            Status dict of the leader node.

        Raises:
            TitanError: If no leader is found.
        """
        leader_url = self._get_leader()
        return self.status(node_url=leader_url)

    def __repr__(self) -> str:
        return f"TitanClient(nodes={self.nodes})"
