use anyhow::Result;

#[derive(Debug, Clone)]
pub struct SqliteAdapter {
    db_path: String,
}

impl SqliteAdapter {
    pub fn new(db_path: &str) -> Result<Self> {
        Ok(Self {
            db_path: db_path.to_string(),
        })
    }

    pub fn db_path(&self) -> &str {
        &self.db_path
    }

    pub fn execute_write(&self, _sql: &str) -> Result<()> {
        // Phase 1 TODO: integrate rusqlite + controlled tx lifecycle.
        Ok(())
    }

    pub fn execute_read(&self, _sql: &str) -> Result<Vec<Vec<String>>> {
        // Phase 1 TODO: return real query results.
        Ok(vec![])
    }
}
