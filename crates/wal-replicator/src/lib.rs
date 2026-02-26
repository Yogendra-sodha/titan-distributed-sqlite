use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

    /// Phase-1 harness parser (not raw sqlite WAL bytes yet).
    /// Input format (one frame per line):
    /// index,checksum,size
    pub fn parse_frame_batch(&self, raw: &[u8]) -> Result<Vec<WalFrameMeta>> {
        let s = std::str::from_utf8(raw)?;
        let mut frames = Vec::new();

        for (ln, line) in s.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split(',').map(|p| p.trim()).collect();
            if parts.len() != 3 {
                return Err(anyhow!("invalid frame format at line {}", ln + 1));
            }

            let f = WalFrameMeta {
                index: parts[0].parse()?,
                checksum: parts[1].parse()?,
                size_bytes: parts[2].parse()?,
            };
            frames.push(f);
        }

        // basic monotonic index check
        for w in frames.windows(2) {
            if w[1].index <= w[0].index {
                return Err(anyhow!("non-monotonic frame index detected"));
            }
        }

        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_batch() {
        let repl = WalReplicator::new();
        let raw = b"1,111,4096\n2,222,4096\n3,333,2048\n";
        let frames = repl.parse_frame_batch(raw).unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].index, 1);
        assert_eq!(frames[2].size_bytes, 2048);
    }

    #[test]
    fn parse_rejects_non_monotonic() {
        let repl = WalReplicator::new();
        let raw = b"2,111,4096\n2,222,4096\n";
        let err = repl.parse_frame_batch(raw).unwrap_err();
        assert!(err.to_string().contains("non-monotonic"));
    }
}
