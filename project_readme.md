## 1. The "Titan" Distributed SQLite Engine
**Combines:** Distributed Systems + Database Internals

*   **Real Problem:** SQLite is perfect for edge/local apps, but syncing data between devices (e.g., iPhone to Server) is incredibly hard. Existing solutions (like Firebase) are expensive and proprietary. Developers want "local-first" apps with SQL power.
*   **The Idea:** Build a Go or Rust-based wrapper around SQLite that adds a **Raft consensus layer**. It replicates WAL (Write-Ahead Log) frames across a cluster of nodes, turning a local disk DB into a fault-tolerant distributed system.
*   **Technical Depth:** You will deal with leader election, log replication, finite state machines, and handling network partitions (Split-brain scenarios).
*   **Stack:** Rust (recommended) or Go, SQLite, gRPC.
*   **Senior Thinking:** Handling "conflict resolution" (Last-Write-Wins vs. CRDTs).
*   **Stretch:** Implement a custom VFS (Virtual File System) for SQLite to stream writes directly to S3 for infinite archival.
*   **Open Source Potential:** High. (See implementations like rqlite or Litestream for inspiration, but aim to build a simplified core yourself).