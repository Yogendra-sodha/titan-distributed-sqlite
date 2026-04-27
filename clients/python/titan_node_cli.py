"""
titan-node CLI wrapper.

After `pip install titan-db`, users can run `titan-node` directly.
This wrapper automatically downloads the Rust binary on first use
and forwards all arguments to it transparently.

Usage:
    titan-node run 1 2@10.0.1.11:5002,3@10.0.1.12:5003 --bind 0.0.0.0
    titan-node --help
"""

import sys
import os
import subprocess


def main():
    """Download the titan-node binary (if needed) and run it with all passed arguments."""
    # Import the download logic from titan_server (shared code)
    from titan_server import _download_binary

    try:
        binary_path = _download_binary("latest")
    except Exception as e:
        print(f"Error: Failed to get titan-node binary: {e}", file=sys.stderr)
        sys.exit(1)

    # Forward ALL arguments to the real binary
    args = [str(binary_path)] + sys.argv[1:]

    # Replace this process with the binary (Unix) or run as subprocess (Windows)
    if os.name != "nt":
        # On Linux/Mac: replace the Python process entirely
        os.execv(str(binary_path), args)
    else:
        # On Windows: run as subprocess and forward exit code
        result = subprocess.run(args)
        sys.exit(result.returncode)


if __name__ == "__main__":
    main()
