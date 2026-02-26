use anyhow::Result;
use sqlite_adapter::SqliteAdapter;
use std::env;
use std::fs;
use wal_replicator::WalReplicator;

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let args: Vec<String> = env::args().collect();
    if args.len() >= 3 && args[1] == "parse-fixture" {
        return run_parse_fixture(&args[2]);
    }

    let adapter = SqliteAdapter::new("./data/titan.db")?;
    let replicator = WalReplicator::new();

    tracing::info!("titan-node bootstrapped");
    tracing::info!("db_path = {}", adapter.db_path());
    tracing::info!("wal replicator initialized: {}", replicator.name());
    tracing::info!("try: titan-node parse-fixture <path-to-fixture>");

    Ok(())
}

fn run_parse_fixture(path: &str) -> Result<()> {
    let bytes = fs::read(path)?;
    let repl = WalReplicator::new();

    // Try binary WAL first; fallback to legacy line fixture format.
    match repl.parse_wal_bytes(&bytes) {
        Ok((_h, frames)) => {
            tracing::info!("Parsed binary WAL fixture: {} frames", frames.len());
            for f in frames.iter().take(5) {
                tracing::info!("frame index={} size={} checksum={}", f.index, f.size_bytes, f.checksum);
            }
            Ok(())
        }
        Err(_) => {
            let frames = repl.parse_frame_batch(&bytes)?;
            tracing::info!("Parsed line fixture: {} frames", frames.len());
            for f in frames.iter().take(5) {
                tracing::info!("frame index={} size={} checksum={}", f.index, f.size_bytes, f.checksum);
            }
            Ok(())
        }
    }
}
