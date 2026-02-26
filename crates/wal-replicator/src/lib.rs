use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

const WAL_MAGIC_BE: u32 = 0x377f0682;
const WAL_MAGIC_LE: u32 = 0x377f0683;
const WAL_HEADER_SIZE: usize = 32;
const WAL_FRAME_HEADER_SIZE: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalFrameMeta {
    pub index: u64,
    pub checksum: u64,
    pub size_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalHeader {
    pub magic: u32,
    pub format_version: u32,
    pub page_size: u32,
    pub checkpoint_seq: u32,
    pub salt1: u32,
    pub salt2: u32,
    pub checksum1: u32,
    pub checksum2: u32,
    pub big_endian_checksums: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalFrameHeader {
    pub page_number: u32,
    pub db_size_after_commit: u32,
    pub salt1: u32,
    pub salt2: u32,
    pub checksum1: u32,
    pub checksum2: u32,
}

pub struct WalReplicator;

impl WalReplicator {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &'static str {
        "wal-replicator"
    }

    /// Legacy Phase-1 harness parser (line format):
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

        for w in frames.windows(2) {
            if w[1].index <= w[0].index {
                return Err(anyhow!("non-monotonic frame index detected"));
            }
        }

        Ok(frames)
    }

    /// Parse SQLite WAL binary header + frame headers to frame metadata.
    /// Notes:
    /// - Validates basic WAL shape and salts.
    /// - Does NOT validate rolling checksums yet (Phase 1.5/2).
    pub fn parse_wal_bytes(&self, raw: &[u8]) -> Result<(WalHeader, Vec<WalFrameMeta>)> {
        let header = parse_wal_header(raw)?;
        let page_size = if header.page_size == 0 { 65536 } else { header.page_size } as usize;

        if raw.len() < WAL_HEADER_SIZE {
            return Err(anyhow!("wal data too small"));
        }

        let frame_size = WAL_FRAME_HEADER_SIZE + page_size;
        let remaining = raw.len() - WAL_HEADER_SIZE;
        if remaining % frame_size != 0 {
            return Err(anyhow!(
                "wal size not aligned to frame boundary: remaining={} frame_size={}",
                remaining,
                frame_size
            ));
        }

        let frame_count = remaining / frame_size;
        let mut metas = Vec::with_capacity(frame_count);

        for i in 0..frame_count {
            let base = WAL_HEADER_SIZE + (i * frame_size);
            let fh = parse_wal_frame_header(&raw[base..base + WAL_FRAME_HEADER_SIZE], header.big_endian_checksums)?;

            if fh.salt1 != header.salt1 || fh.salt2 != header.salt2 {
                return Err(anyhow!("salt mismatch at frame {}", i + 1));
            }

            let checksum = ((fh.checksum1 as u64) << 32) | fh.checksum2 as u64;
            metas.push(WalFrameMeta {
                index: (i + 1) as u64,
                checksum,
                size_bytes: page_size as u32,
            });
        }

        Ok((header, metas))
    }
}

fn parse_wal_header(raw: &[u8]) -> Result<WalHeader> {
    if raw.len() < WAL_HEADER_SIZE {
        return Err(anyhow!("wal header too small"));
    }

    let magic_be = u32::from_be_bytes(raw[0..4].try_into().unwrap());
    let big_endian_checksums = match magic_be {
        WAL_MAGIC_BE => true,
        WAL_MAGIC_LE => false,
        _ => return Err(anyhow!("invalid wal magic: 0x{magic_be:08x}")),
    };

    let read_u32 = |offset: usize| -> u32 {
        // WAL header fields are stored big-endian per SQLite WAL format.
        u32::from_be_bytes(raw[offset..offset + 4].try_into().unwrap())
    };

    Ok(WalHeader {
        magic: magic_be,
        format_version: read_u32(4),
        page_size: read_u32(8),
        checkpoint_seq: read_u32(12),
        salt1: read_u32(16),
        salt2: read_u32(20),
        checksum1: read_u32(24),
        checksum2: read_u32(28),
        big_endian_checksums,
    })
}

fn parse_wal_frame_header(raw: &[u8], big_endian_checksums: bool) -> Result<WalFrameHeader> {
    if raw.len() < WAL_FRAME_HEADER_SIZE {
        return Err(anyhow!("wal frame header too small"));
    }

    let be = |offset: usize| -> u32 { u32::from_be_bytes(raw[offset..offset + 4].try_into().unwrap()) };
    let cks = |offset: usize| -> u32 {
        if big_endian_checksums {
            u32::from_be_bytes(raw[offset..offset + 4].try_into().unwrap())
        } else {
            u32::from_le_bytes(raw[offset..offset + 4].try_into().unwrap())
        }
    };

    Ok(WalFrameHeader {
        page_number: be(0),
        db_size_after_commit: be(4),
        salt1: be(8),
        salt2: be(12),
        checksum1: cks(16),
        checksum2: cks(20),
    })
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

    fn build_fake_wal(page_size: u32, frame_count: usize) -> Vec<u8> {
        let mut out = Vec::new();

        // WAL header (32 bytes)
        out.extend_from_slice(&WAL_MAGIC_BE.to_be_bytes()); // magic
        out.extend_from_slice(&3007000u32.to_be_bytes()); // format version
        out.extend_from_slice(&page_size.to_be_bytes()); // page size
        out.extend_from_slice(&0u32.to_be_bytes()); // checkpoint seq
        out.extend_from_slice(&0xAABBCCDDu32.to_be_bytes()); // salt1
        out.extend_from_slice(&0x11223344u32.to_be_bytes()); // salt2
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum1
        out.extend_from_slice(&0u32.to_be_bytes()); // checksum2

        let page_sz = if page_size == 0 { 65536 } else { page_size } as usize;

        for i in 0..frame_count {
            // frame header (24 bytes)
            out.extend_from_slice(&((i + 1) as u32).to_be_bytes()); // page_number
            out.extend_from_slice(&0u32.to_be_bytes()); // db_size_after_commit
            out.extend_from_slice(&0xAABBCCDDu32.to_be_bytes()); // salt1
            out.extend_from_slice(&0x11223344u32.to_be_bytes()); // salt2
            out.extend_from_slice(&(100 + i as u32).to_be_bytes()); // checksum1
            out.extend_from_slice(&(200 + i as u32).to_be_bytes()); // checksum2

            // page bytes
            out.extend(vec![0u8; page_sz]);
        }

        out
    }

    #[test]
    fn parse_wal_bytes_header_and_frames() {
        let repl = WalReplicator::new();
        let wal = build_fake_wal(4096, 2);
        let (h, frames) = repl.parse_wal_bytes(&wal).unwrap();

        assert_eq!(h.magic, WAL_MAGIC_BE);
        assert_eq!(h.page_size, 4096);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].index, 1);
        assert_eq!(frames[1].size_bytes, 4096);
    }

    #[test]
    fn parse_wal_bytes_rejects_bad_magic() {
        let repl = WalReplicator::new();
        let mut wal = build_fake_wal(4096, 1);
        wal[0..4].copy_from_slice(&0x12345678u32.to_be_bytes());
        let err = repl.parse_wal_bytes(&wal).unwrap_err();
        assert!(err.to_string().contains("invalid wal magic"));
    }
}
