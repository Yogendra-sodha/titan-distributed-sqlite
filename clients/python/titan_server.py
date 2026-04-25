"""
Titan Server Manager — Downloads and runs the Titan Distributed SQLite cluster.

After `pip install titan-db`, users can start the server with:
    titan-server start          # Start a 3-node local cluster
    titan-server stop           # Stop all running nodes
    titan-server status         # Check cluster health

Or from Python:
    from titan_db.server import TitanServer
    server = TitanServer()
    server.start()
"""

import os
import sys
import platform
import subprocess
import time
import signal
import json
import shutil
import stat
import urllib.request
import zipfile
import tarfile
from pathlib import Path
from typing import Optional, List

# Where we cache the downloaded binary
TITAN_HOME = Path.home() / ".titan"
BINARY_DIR = TITAN_HOME / "bin"
DATA_DIR = TITAN_HOME / "data"
PID_FILE = TITAN_HOME / "cluster.pid"

# GitHub release info
GITHUB_REPO = "Yogendra-sodha/titan-distributed-sqlite"
BINARY_NAME = "titan-node"

def _get_platform_key() -> str:
    """Determine the platform key for downloading the correct binary."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    if system == "windows":
        return "windows-x86_64"
    elif system == "darwin":
        if machine == "arm64":
            return "macos-aarch64"
        return "macos-x86_64"
    elif system == "linux":
        if machine == "aarch64":
            return "linux-aarch64"
        return "linux-x86_64"
    else:
        raise RuntimeError(f"Unsupported platform: {system} {machine}")


def _get_binary_path() -> Path:
    """Return the path to the titan-node binary."""
    system = platform.system().lower()
    name = f"{BINARY_NAME}.exe" if system == "windows" else BINARY_NAME
    return BINARY_DIR / name


def _download_binary(version: str = "latest") -> Path:
    """Download the titan-node binary from GitHub Releases."""
    binary_path = _get_binary_path()

    if binary_path.exists():
        print(f"Binary already cached at {binary_path}")
        return binary_path

    platform_key = _get_platform_key()
    system = platform.system().lower()
    ext = "zip" if system == "windows" else "tar.gz"
    asset_name = f"titan-node-{platform_key}.{ext}"

    # Resolve 'latest' to actual tag
    if version == "latest":
        api_url = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
        print(f"Checking latest release from {GITHUB_REPO}...")
        try:
            req = urllib.request.Request(api_url, headers={"User-Agent": "titan-db-client"})
            with urllib.request.urlopen(req, timeout=15) as resp:
                release = json.loads(resp.read().decode())
                version = release["tag_name"]
        except Exception as e:
            raise RuntimeError(
                f"Failed to fetch latest release from GitHub: {e}\n"
                f"Check your internet connection or specify a version explicitly."
            )

    download_url = f"https://github.com/{GITHUB_REPO}/releases/download/{version}/{asset_name}"

    print(f"Downloading titan-node {version} for {platform_key}...")
    print(f"  URL: {download_url}")

    # Create directories
    BINARY_DIR.mkdir(parents=True, exist_ok=True)

    # Download
    archive_path = TITAN_HOME / asset_name
    try:
        req = urllib.request.Request(download_url, headers={"User-Agent": "titan-db-client"})
        with urllib.request.urlopen(req, timeout=120) as resp:
            with open(archive_path, "wb") as f:
                shutil.copyfileobj(resp, f)
    except urllib.error.HTTPError as e:
        if e.code == 404:
            raise RuntimeError(
                f"Binary not found for your platform ({platform_key}).\n"
                f"  Tried: {download_url}\n"
                f"\n"
                f"Pre-built binaries may not be available yet.\n"
                f"You can build from source instead:\n"
                f"  git clone https://github.com/{GITHUB_REPO}.git\n"
                f"  cd titan-distributed-sqlite && cargo build --release\n"
                f"  cp target/release/titan-node ~/.titan/bin/"
            )
        raise

    # Extract
    print(f"Extracting...")
    if ext == "zip":
        with zipfile.ZipFile(archive_path, "r") as zf:
            zf.extractall(BINARY_DIR)
    else:
        with tarfile.open(archive_path, "r:gz") as tf:
            tf.extractall(BINARY_DIR)

    # Make executable on Unix
    if system != "windows":
        binary_path.chmod(binary_path.stat().st_mode | stat.S_IEXEC)

    # Cleanup archive
    archive_path.unlink(missing_ok=True)

    print(f"Installed titan-node to {binary_path}")
    return binary_path


class TitanServer:
    """Manages a Titan Distributed SQLite cluster.

    Local mode (all nodes on one machine):
        server = TitanServer(nodes=3)
        server.start()

    Multi-server mode (this node + remote peers):
        server = TitanServer(
            node_id=1,
            peer_addresses={2: "10.0.1.11:5002", 3: "10.0.1.12:5003"}
        )
        server.start_node()  # starts only THIS node
    """

    def __init__(self, nodes: int = 3, base_http_port: int = 8000, base_udp_port: int = 5000,
                 node_id: Optional[int] = None, peer_addresses: Optional[dict] = None):
        self.num_nodes = nodes
        self.base_http_port = base_http_port
        self.base_udp_port = base_udp_port
        self.node_id = node_id
        self.peer_addresses = peer_addresses or {}
        self._processes: List[subprocess.Popen] = []

    def _build_peer_arg(self, node_id: int, all_node_ids: List[int]) -> str:
        """Build the peer argument string for a node.

        Returns either simple format '2,3' or address format '2@ip:port,3@ip:port'.
        """
        parts = []
        for pid in all_node_ids:
            if pid == node_id:
                continue
            if pid in self.peer_addresses:
                parts.append(f"{pid}@{self.peer_addresses[pid]}")
            else:
                parts.append(str(pid))
        return ",".join(parts)

    def start_node(self, version: str = "latest", data_dir: Optional[str] = None,
                   bind_addr: str = "0.0.0.0") -> str:
        """Start a single Titan node (for multi-server deployment).

        Args:
            version: Which release version to use
            data_dir: Directory for database files
            bind_addr: Address to bind to (default: 0.0.0.0)

        Returns:
            This node's URL, e.g. "http://0.0.0.0:8001"
        """
        if self.node_id is None:
            raise ValueError("node_id must be set for multi-server mode")

        binary = _download_binary(version)
        work_dir = Path(data_dir) if data_dir else DATA_DIR
        work_dir.mkdir(parents=True, exist_ok=True)

        all_ids = sorted(set([self.node_id] + list(self.peer_addresses.keys())))
        peers_arg = self._build_peer_arg(self.node_id, all_ids)
        cmd = [str(binary), "run", str(self.node_id), peers_arg, "--bind", bind_addr]

        print(f"Starting Node {self.node_id} (multi-server mode)")
        print(f"  HTTP: {bind_addr}:{self.base_http_port + self.node_id}")
        print(f"  UDP:  {bind_addr}:{self.base_udp_port + self.node_id}")
        print(f"  Peers: {peers_arg}")

        proc = subprocess.Popen(
            cmd,
            cwd=str(work_dir),
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            creationflags=subprocess.CREATE_NO_WINDOW if platform.system() == "Windows" else 0,
        )
        self._processes.append(proc)

        PID_FILE.write_text(json.dumps([proc.pid]))
        url = f"http://{bind_addr}:{self.base_http_port + self.node_id}"

        time.sleep(2)
        print(f"Node {self.node_id} started. URL: {url}")
        return url

    def start(self, version: str = "latest", data_dir: Optional[str] = None) -> List[str]:
        """Start the Titan cluster (local mode — all nodes on this machine).

        Args:
            version: Which release version to use (default: "latest")
            data_dir: Directory for database files (default: ~/.titan/data)

        Returns:
            List of node URLs, e.g. ["http://127.0.0.1:8001", ...]
        """
        binary = _download_binary(version)
        work_dir = Path(data_dir) if data_dir else DATA_DIR
        work_dir.mkdir(parents=True, exist_ok=True)

        # Check if already running
        if PID_FILE.exists():
            print("Cluster may already be running. Run 'titan-server stop' first.")
            print("Or delete ~/.titan/cluster.pid if stale.")
            return self.node_urls()

        node_ids = list(range(1, self.num_nodes + 1))
        pids = []
        urls = []

        for node_id in node_ids:
            peers_arg = self._build_peer_arg(node_id, node_ids)
            cmd = [str(binary), "run", str(node_id), peers_arg]

            print(f"Starting Node {node_id} (HTTP: :{self.base_http_port + node_id}, UDP: :{self.base_udp_port + node_id})...")

            proc = subprocess.Popen(
                cmd,
                cwd=str(work_dir),
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                creationflags=subprocess.CREATE_NO_WINDOW if platform.system() == "Windows" else 0,
            )
            self._processes.append(proc)
            pids.append(proc.pid)
            urls.append(f"http://127.0.0.1:{self.base_http_port + node_id}")

        # Save PIDs
        PID_FILE.write_text(json.dumps(pids))

        # Wait briefly for nodes to start
        print("Waiting for cluster to initialize...")
        time.sleep(2)

        # Verify at least one node is up
        import urllib.request
        for url in urls:
            try:
                req = urllib.request.Request(f"{url}/status", headers={"User-Agent": "titan-server"})
                with urllib.request.urlopen(req, timeout=3) as resp:
                    data = json.loads(resp.read().decode())
                    print(f"  Node {data.get('node_id')}: {data.get('role')} (term={data.get('term')})")
            except Exception:
                print(f"  Node at {url}: starting up...")

        print(f"\nCluster is running! {self.num_nodes} nodes active.")
        print(f"Dashboard: {urls[0]}")
        print(f"\nConnect with:")
        print(f'  from titan_db import TitanClient')
        print(f'  db = TitanClient({json.dumps(urls)})')

        return urls

    def stop(self):
        """Stop all running cluster nodes."""
        # Kill managed processes
        for proc in self._processes:
            try:
                proc.terminate()
                proc.wait(timeout=5)
            except Exception:
                proc.kill()
        self._processes.clear()

        # Also kill from PID file (for CLI usage)
        if PID_FILE.exists():
            try:
                pids = json.loads(PID_FILE.read_text())
                for pid in pids:
                    try:
                        if platform.system() == "Windows":
                            subprocess.run(["taskkill", "/F", "/PID", str(pid)],
                                         capture_output=True, timeout=5)
                        else:
                            os.kill(pid, signal.SIGTERM)
                    except (ProcessLookupError, OSError):
                        pass
                PID_FILE.unlink(missing_ok=True)
                print(f"Stopped {len(pids)} node(s).")
            except Exception as e:
                print(f"Error stopping nodes: {e}")
                PID_FILE.unlink(missing_ok=True)
        else:
            print("No cluster PID file found. Nothing to stop.")

    def node_urls(self) -> List[str]:
        """Return the list of node URLs for this cluster."""
        return [f"http://127.0.0.1:{self.base_http_port + i}" for i in range(1, self.num_nodes + 1)]

    def __enter__(self):
        self.start()
        return self

    def __exit__(self, *args):
        self.stop()


def _cli():
    """Command-line interface for titan-server."""
    if len(sys.argv) < 2:
        print("Titan Server Manager")
        print()
        print("Usage:")
        print("  titan-server start     Start a 3-node local cluster")
        print("  titan-server stop      Stop all running nodes")
        print("  titan-server status    Check cluster health")
        print("  titan-server download  Download the server binary only")
        print()
        print("After starting, connect with Python:")
        print('  from titan_db import TitanClient')
        print('  db = TitanClient(["http://127.0.0.1:8001", "http://127.0.0.1:8002", "http://127.0.0.1:8003"])')
        sys.exit(0)

    cmd = sys.argv[1].lower()

    if cmd == "start":
        server = TitanServer()
        server.start()
        print("\nCluster is running in the background.")
        print("To stop: titan-server stop")

    elif cmd == "stop":
        server = TitanServer()
        server.stop()

    elif cmd == "status":
        import urllib.request
        urls = [f"http://127.0.0.1:{8000 + i}" for i in range(1, 4)]
        any_up = False
        for url in urls:
            try:
                req = urllib.request.Request(f"{url}/status", headers={"User-Agent": "titan-server"})
                with urllib.request.urlopen(req, timeout=3) as resp:
                    data = json.loads(resp.read().decode())
                    role = data.get("role", "?")
                    marker = " (Leader)" if role == "Leader" else ""
                    print(f"  Node {data['node_id']}: {role}{marker}  term={data.get('term')} commit={data.get('commit_index')}")
                    any_up = True
            except Exception:
                node_id = int(url.split(":")[-1]) - 8000
                print(f"  Node {node_id}: OFFLINE")
        if not any_up:
            print("\nNo nodes are running. Start with: titan-server start")

    elif cmd == "download":
        _download_binary()
        print("Done! Binary is ready.")

    else:
        print(f"Unknown command: {cmd}")
        print("Available: start, stop, status, download")
        sys.exit(1)


if __name__ == "__main__":
    _cli()
