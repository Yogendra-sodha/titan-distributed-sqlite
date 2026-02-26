use anyhow::Result;
use sqlite_adapter::SqliteAdapter;
use wal_replicator::WalReplicator;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let adapter = SqliteAdapter::new("./data/titan.db")?;
    let replicator = WalReplicator::new();

    tracing::info!("titan-node bootstrapped");
    tracing::info!("db_path = {}", adapter.db_path());
    tracing::info!("wal replicator initialized: {}", replicator.name());

    // Phase 1 placeholder:
    // - wire a controlled write path
    // - extract wal batch metadata
    // - replay harness against fixtures

    Ok(())
}
