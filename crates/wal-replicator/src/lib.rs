use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalFrameMeta {
    pub index: u64,
    pub checksum: u64,
    pub size_bytes: u32,
}

pub struct WalReplicator;

impl WalReplicator {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &'static str {
        "wal-replicator"
    }

    pub fn parse_frame_batch(&self, _raw: &[u8]) -> Result<Vec<WalFrameMeta>> {
        // Phase 1 TODO: implement WAL frame parsing + validation.
        Ok(vec![])
    }
}
