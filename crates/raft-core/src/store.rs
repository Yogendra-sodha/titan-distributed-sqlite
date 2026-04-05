use crate::{LogEntry, RaftNodeId};
use rusqlite::Connection;

pub trait RaftStateStore {
    fn current_term(&self) -> u64;
    fn set_current_term(&mut self, term: u64);

    fn voted_for(&self) -> Option<RaftNodeId>;
    fn set_voted_for(&mut self, node: Option<RaftNodeId>);

    fn append_entry(&mut self, entry: LogEntry);
    fn entries(&self) -> &[LogEntry];

    /// Truncate log from index onwards (used during conflict resolution)
    fn truncate_from(&mut self, from_index: u64);

    /// Get the last log entry's index, or 0 if empty
    fn last_log_index(&self) -> u64;

    /// Get the commit_index persisted to disk
    fn commit_index(&self) -> u64;
    fn set_commit_index(&mut self, idx: u64);
}

// ── In-Memory Store (for unit tests) ──

#[derive(Debug, Default)]
pub struct InMemoryStateStore {
    term: u64,
    voted_for: Option<RaftNodeId>,
    log: Vec<LogEntry>,
    commit_index: u64,
}

impl RaftStateStore for InMemoryStateStore {
    fn current_term(&self) -> u64 {
        self.term
    }

    fn set_current_term(&mut self, term: u64) {
        self.term = term;
    }

    fn voted_for(&self) -> Option<RaftNodeId> {
        self.voted_for
    }

    fn set_voted_for(&mut self, node: Option<RaftNodeId>) {
        self.voted_for = node;
    }

    fn append_entry(&mut self, entry: LogEntry) {
        self.log.push(entry);
    }

    fn entries(&self) -> &[LogEntry] {
        &self.log
    }

    fn truncate_from(&mut self, from_index: u64) {
        self.log.retain(|e| e.index < from_index);
    }

    fn last_log_index(&self) -> u64 {
        self.log.last().map(|e| e.index).unwrap_or(0)
    }

    fn commit_index(&self) -> u64 {
        self.commit_index
    }

    fn set_commit_index(&mut self, idx: u64) {
        self.commit_index = idx;
    }
}

// ── SQLite-Backed Persistent Store ──

pub struct SqliteStateStore {
    conn: Connection,
    /// Cached in-memory copy of the log for fast reads (authoritative copy is on disk)
    log_cache: Vec<LogEntry>,
    /// Cached term (authoritative copy is on disk)
    term_cache: u64,
    /// Cached voted_for (authoritative copy is on disk)
    voted_for_cache: Option<RaftNodeId>,
    /// Cached commit_index
    commit_index_cache: u64,
}

impl std::fmt::Debug for SqliteStateStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStateStore")
            .field("term", &self.term_cache)
            .field("voted_for", &self.voted_for_cache)
            .field("log_len", &self.log_cache.len())
            .field("commit_index", &self.commit_index_cache)
            .finish()
    }
}

impl SqliteStateStore {
    /// Open or create a persistent Raft state store at the given path.
    pub fn new(db_path: &str) -> Self {
        let conn = Connection::open(db_path)
            .unwrap_or_else(|e| panic!("Failed to open raft state db at {}: {}", db_path, e));

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
            .expect("Failed to set WAL mode on raft state db");

        // Create tables if they don't exist
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS raft_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS raft_log (
                idx INTEGER PRIMARY KEY,
                term INTEGER NOT NULL,
                payload BLOB NOT NULL
            );"
        ).expect("Failed to create raft state tables");

        // Load existing state
        let term_cache = Self::load_u64(&conn, "current_term").unwrap_or(0);
        let commit_index_cache = Self::load_u64(&conn, "commit_index").unwrap_or(0);
        let voted_for_cache = Self::load_string(&conn, "voted_for")
            .and_then(|s| s.parse::<u64>().ok())
            .map(RaftNodeId);

        let log_cache = Self::load_log(&conn);

        Self {
            conn,
            log_cache,
            term_cache,
            voted_for_cache,
            commit_index_cache,
        }
    }

    fn load_u64(conn: &Connection, key: &str) -> Option<u64> {
        conn.query_row(
            "SELECT value FROM raft_meta WHERE key = ?1",
            [key],
            |row| {
                let s: String = row.get(0)?;
                Ok(s.parse::<u64>().unwrap_or(0))
            },
        ).ok()
    }

    fn load_string(conn: &Connection, key: &str) -> Option<String> {
        conn.query_row(
            "SELECT value FROM raft_meta WHERE key = ?1",
            [key],
            |row| row.get(0),
        ).ok()
    }

    fn save_meta(&self, key: &str, value: &str) {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO raft_meta (key, value) VALUES (?1, ?2)",
                rusqlite::params![key, value],
            )
            .expect("Failed to save raft metadata");
    }

    fn load_log(conn: &Connection) -> Vec<LogEntry> {
        let mut stmt = conn
            .prepare("SELECT idx, term, payload FROM raft_log ORDER BY idx ASC")
            .expect("Failed to prepare log query");

        stmt.query_map([], |row| {
            Ok(LogEntry {
                index: row.get::<_, i64>(0)? as u64,
                term: row.get::<_, i64>(1)? as u64,
                payload: row.get(2)?,
            })
        })
        .expect("Failed to query raft log")
        .filter_map(|r| r.ok())
        .collect()
    }
}

impl RaftStateStore for SqliteStateStore {
    fn current_term(&self) -> u64 {
        self.term_cache
    }

    fn set_current_term(&mut self, term: u64) {
        self.term_cache = term;
        self.save_meta("current_term", &term.to_string());
    }

    fn voted_for(&self) -> Option<RaftNodeId> {
        self.voted_for_cache
    }

    fn set_voted_for(&mut self, node: Option<RaftNodeId>) {
        self.voted_for_cache = node;
        match node {
            Some(id) => self.save_meta("voted_for", &id.0.to_string()),
            None => {
                let _ = self.conn.execute("DELETE FROM raft_meta WHERE key = 'voted_for'", []);
            }
        }
    }

    fn append_entry(&mut self, entry: LogEntry) {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO raft_log (idx, term, payload) VALUES (?1, ?2, ?3)",
                rusqlite::params![entry.index as i64, entry.term as i64, &entry.payload],
            )
            .expect("Failed to persist log entry");
        self.log_cache.push(entry);
    }

    fn entries(&self) -> &[LogEntry] {
        &self.log_cache
    }

    fn truncate_from(&mut self, from_index: u64) {
        self.conn
            .execute(
                "DELETE FROM raft_log WHERE idx >= ?1",
                rusqlite::params![from_index as i64],
            )
            .expect("Failed to truncate raft log");
        self.log_cache.retain(|e| e.index < from_index);
    }

    fn last_log_index(&self) -> u64 {
        self.log_cache.last().map(|e| e.index).unwrap_or(0)
    }

    fn commit_index(&self) -> u64 {
        self.commit_index_cache
    }

    fn set_commit_index(&mut self, idx: u64) {
        self.commit_index_cache = idx;
        self.save_meta("commit_index", &idx.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut p = std::env::temp_dir();
        p.push(format!("titan-raft-store-test-{}.db", ts));
        p.to_string_lossy().to_string()
    }

    #[test]
    fn test_sqlite_store_term_and_vote() {
        let path = temp_path();
        {
            let mut store = SqliteStateStore::new(&path);
            assert_eq!(store.current_term(), 0);
            assert_eq!(store.voted_for(), None);

            store.set_current_term(5);
            store.set_voted_for(Some(RaftNodeId(3)));
            assert_eq!(store.current_term(), 5);
            assert_eq!(store.voted_for(), Some(RaftNodeId(3)));
        }
        // Reopen — state should persist
        {
            let store = SqliteStateStore::new(&path);
            assert_eq!(store.current_term(), 5);
            assert_eq!(store.voted_for(), Some(RaftNodeId(3)));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_sqlite_store_log_persistence() {
        let path = temp_path();
        {
            let mut store = SqliteStateStore::new(&path);
            store.append_entry(LogEntry { index: 1, term: 1, payload: b"INSERT 1".to_vec() });
            store.append_entry(LogEntry { index: 2, term: 1, payload: b"INSERT 2".to_vec() });
            store.append_entry(LogEntry { index: 3, term: 2, payload: b"INSERT 3".to_vec() });

            assert_eq!(store.entries().len(), 3);
            assert_eq!(store.last_log_index(), 3);
        }
        // Reopen — log should persist
        {
            let store = SqliteStateStore::new(&path);
            assert_eq!(store.entries().len(), 3);
            assert_eq!(store.entries()[0].payload, b"INSERT 1");
            assert_eq!(store.entries()[2].term, 2);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_sqlite_store_truncation() {
        let path = temp_path();
        {
            let mut store = SqliteStateStore::new(&path);
            store.append_entry(LogEntry { index: 1, term: 1, payload: b"a".to_vec() });
            store.append_entry(LogEntry { index: 2, term: 1, payload: b"b".to_vec() });
            store.append_entry(LogEntry { index: 3, term: 1, payload: b"c".to_vec() });

            // Truncate from index 2 (simulating conflict resolution)
            store.truncate_from(2);
            assert_eq!(store.entries().len(), 1);
            assert_eq!(store.last_log_index(), 1);
        }
        // Reopen — truncation should be persisted
        {
            let store = SqliteStateStore::new(&path);
            assert_eq!(store.entries().len(), 1);
            assert_eq!(store.entries()[0].payload, b"a");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_sqlite_store_commit_index() {
        let path = temp_path();
        {
            let mut store = SqliteStateStore::new(&path);
            assert_eq!(store.commit_index(), 0);
            store.set_commit_index(42);
            assert_eq!(store.commit_index(), 42);
        }
        {
            let store = SqliteStateStore::new(&path);
            assert_eq!(store.commit_index(), 42);
        }
        let _ = std::fs::remove_file(&path);
    }
}
