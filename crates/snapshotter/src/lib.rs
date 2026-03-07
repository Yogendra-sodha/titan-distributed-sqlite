use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct SnapshotManager {
    base_dir: String,
}

impl SnapshotManager {
    pub fn new(base_dir: &str) -> Result<Self> {
        fs::create_dir_all(base_dir)?;
        Ok(Self {
            base_dir: base_dir.to_string(),
        })
    }

    pub fn save_snapshot(&self, index: u64, data: &[u8]) -> Result<()> {
        let path = Path::new(&self.base_dir).join(format!("snapshot_{}.bin", index));
        fs::write(path, data)?;
        tracing::info!("Saved snapshot at index {}", index);
        Ok(())
    }

    pub fn load_latest_snapshot(&self) -> Result<Option<(u64, Vec<u8>)>> {
        let mut latest_idx = 0;
        let mut latest_data = None;

        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("snapshot_") && name.ends_with(".bin") {
                let s = &name["snapshot_".len()..name.len() - ".bin".len()];
                if let Ok(idx) = s.parse::<u64>() {
                    if idx > latest_idx {
                        latest_idx = idx;
                        latest_data = Some(fs::read(entry.path())?);
                    }
                }
            }
        }

        if let Some(data) = latest_data {
            Ok(Some((latest_idx, data)))
        } else {
            Ok(None)
        }
    }
}
