use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn, error};

pub struct Metrics {
    pub ops_applied: AtomicU64,
    pub leader_changes: AtomicU64,
    pub snapshot_installs: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            ops_applied: AtomicU64::new(0),
            leader_changes: AtomicU64::new(0),
            snapshot_installs: AtomicU64::new(0),
        }
    }

    pub fn inc_ops(&self) {
        self.ops_applied.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_leader_changes(&self) {
        self.leader_changes.fetch_add(1, Ordering::Relaxed);
        info!("Leader elections changed");
    }

    pub fn inc_snapshot_installs(&self) {
        self.snapshot_installs.fetch_add(1, Ordering::Relaxed);
        warn!("Snapshot install initiated");
    }

    pub fn log_health(&self) {
        info!("Health OK | ops_applied: {} | leader_changes: {} | snapshot_installs: {}", 
            self.ops_applied.load(Ordering::Relaxed),
            self.leader_changes.load(Ordering::Relaxed),
            self.snapshot_installs.load(Ordering::Relaxed)
        );
    }
}

pub fn init_tracing() {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
}
