---
description: Add HTTP API to Titan with /execute, /query, and /status endpoints
---
// turbo-all

This workflow adds an HTTP REST API to the titan-node binary so users can interact with the cluster via HTTP instead of raw UDP.

1. Add Axum HTTP dependencies to `cmd/titan-node/Cargo.toml` (axum, tokio, serde_json already in workspace).

2. Create an HTTP server that runs alongside the UDP Raft loop in `cmd/titan-node/src/main.rs`:
   - `POST /execute` — accepts JSON `{"sql": "..."}`, sends it through Raft, returns confirmation
   - `GET /query?sql=...` — reads directly from local SQLite (no Raft needed for reads)
   - `GET /status` — returns node role, term, commit_index, peer info as JSON

3. The HTTP server should listen on port `8000 + node_id` (e.g., Node 1 = 8001).

4. Build and verify:
```
cargo build --release 2>&1
```

5. Run tests:
```
cargo test --workspace 2>&1
```

6. Report results to user.
