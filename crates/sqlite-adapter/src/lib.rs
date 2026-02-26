use anyhow::Result;
use rusqlite::Connection;

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
        conn.execute_batch(sql)?;
        Ok(())
    }

    pub fn execute_read(&self, sql: &str) -> Result<Vec<Vec<String>>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(sql)?;
        let col_count = stmt.column_count();
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();

        while let Some(row) = rows.next()? {
            let mut vals = Vec::with_capacity(col_count);
            for i in 0..col_count {
                let val: std::result::Result<String, _> = row.get(i);
                vals.push(val.unwrap_or_default());
            }
            out.push(vals);
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db() -> String {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
        format!("./target/test-{}.db", ts)
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
}
