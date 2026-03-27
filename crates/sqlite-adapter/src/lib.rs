use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::fs;

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    db_path: String,
}

impl SqliteAdapter {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        Ok(Self {
            db_path: db_path.to_string(),
        })
    }

    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    pub fn execute_write(&self, sql: &str) -> Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(sql)?;
        Ok(())
    }

    pub fn execute_read(&self, sql: &str) -> Result<Vec<Vec<String>>> {
        let (_cols, rows) = self.execute_read_with_columns(sql)?;
        Ok(rows)
    }

    pub fn execute_read_with_columns(&self, sql: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();

        // Get column names
        let columns: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let mut rows_out = stmt.query([])?;
        let mut out = Vec::new();

        while let Some(row) = rows_out.next()? {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let val_str = match row.get_ref(i) {
                    Ok(rusqlite::types::ValueRef::Null) => "NULL".to_string(),
                    Ok(rusqlite::types::ValueRef::Integer(n)) => n.to_string(),
                    Ok(rusqlite::types::ValueRef::Real(f)) => f.to_string(),
                    Ok(rusqlite::types::ValueRef::Text(t)) => {
                        String::from_utf8_lossy(t).to_string()
                    }
                    Ok(rusqlite::types::ValueRef::Blob(b)) => {
                        format!("<blob {} bytes>", b.len())
                    }
                    Err(_) => "ERROR".to_string(),
                };
                vals.push(val_str);
            }
            out.push(vals);
        }

        Ok((columns, out))
    }

    /// Generate a realistic SQLite WAL fixture by creating writes in WAL mode
    /// and exporting the live `-wal` file.
    pub fn generate_wal_fixture(output_path: &str) -> Result<()> {
        let mut db = std::env::temp_dir();
        db.push(format!("titan-wal-fixture-{}.db", unique_ts()));

        // Keep connection open while reading -wal file.
        let conn = Connection::open(&db)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS events(
              id INTEGER PRIMARY KEY,
              k TEXT NOT NULL,
              v TEXT NOT NULL
            );
            INSERT INTO events(k,v) VALUES ('alpha','1');
            INSERT INTO events(k,v) VALUES ('beta','2');
            INSERT INTO events(k,v) VALUES ('gamma','3');
            ",
        )?;

        // Force WAL content to flush from page cache to file.
        conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;

        let wal_path = format!("{}-wal", db.to_string_lossy());
        if !std::path::Path::new(&wal_path).exists() {
            return Err(anyhow!("wal file was not created at {}", wal_path));
        }

        let bytes = fs::read(&wal_path)?;
        if bytes.len() < 32 {
            return Err(anyhow!("wal fixture too small"));
        }
        fs::write(output_path, bytes)?;

        // best-effort cleanup
        let _ = fs::remove_file(&wal_path);
        let _ = fs::remove_file(&db);
        let shm_path = format!("{}-shm", db.to_string_lossy());
        let _ = fs::remove_file(&shm_path);

        Ok(())
    }
}

fn unique_ts() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("titan-test-{}.db", unique_ts()));
        p.to_string_lossy().to_string()
    }

    #[test]
    fn create_write_read() {
        let db = temp_db();
        let adapter = SqliteAdapter::new(&db).unwrap();
        adapter
            .execute_write(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT);\n                 INSERT INTO t(name) VALUES ('alice');",
            )
            .unwrap();

        let rows = adapter.execute_read("SELECT name FROM t ORDER BY id;").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0], "alice");
    }

    #[test]
    fn generate_wal_fixture_file() {
        let mut out = std::env::temp_dir();
        out.push(format!("titan-fixture-{}.wal", unique_ts()));
        SqliteAdapter::generate_wal_fixture(out.to_string_lossy().as_ref()).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() >= 32);
        let _ = std::fs::remove_file(out);
    }
}
