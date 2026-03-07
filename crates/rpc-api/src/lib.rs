use anyhow::Result;
use observability::Metrics;
use raft_core::{RaftNode, Role};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct HealthResponse {
    pub status: String,
    pub role: String,
    pub term: u64,
    pub commit_index: u64,
    pub ops_applied: u64,
}

pub fn generate_health_report(node: &RaftNode, metrics: &Metrics) -> Result<String> {
    let role_str = match node.role {
        Role::Leader => "Leader",
        Role::Follower => "Follower",
        Role::Candidate => "Candidate",
    };

    let resp = HealthResponse {
        status: "OK".to_string(),
        role: role_str.to_string(),
        term: node.current_term,
        commit_index: node.commit_index,
        ops_applied: metrics.ops_applied.load(std::sync::atomic::Ordering::Relaxed),
    };

    let json = serde_json::to_string(&resp)?;
    Ok(json)
}
