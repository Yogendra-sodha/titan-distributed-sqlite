use anyhow::Result;
use sqlite_adapter::SqliteAdapter;
use std::env;
use std::fs;
use wal_replicator::WalReplicator;
use raft_core::{RaftNode, RaftNodeId, rpc::RaftMessage};
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    // Only init tracing if we're not running the cluster test from PS,
    // otherwise the output gets very noisy. We just use env_logger or simple format.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("titan_node=info".parse().unwrap()))
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() >= 3 && args[1] == "parse-fixture" {
        return run_parse_fixture(&args[2]);
    }
    if args.len() >= 3 && args[1] == "generate-wal-fixture" {
        return run_generate_fixture(&args[2]);
    }
    if args.len() >= 4 && args[1] == "run" {
        let node_id: u64 = args[2].parse()?;
        let peers: Vec<RaftNodeId> = args[3]
            .split(',')
            .map(|s| RaftNodeId(s.parse().unwrap()))
            .collect();
        return run_server(node_id, peers);
    }
    if args.len() >= 4 && args[1] == "client" {
        let dest_port: u16 = args[2].parse()?;
        let command = args[3].clone();
        return run_client(dest_port, command);
    }

    tracing::info!("titan-node bootstrapped");
    tracing::info!("try: titan-node run <id> <peer_list>");
    tracing::info!("try: titan-node client <leader_port> <sql_statement>");

    Ok(())
}

fn run_server(id: u64, peers: Vec<RaftNodeId>) -> Result<()> {
    tracing::info!("===================================");
    tracing::info!("🚀 Starting Titan Node {} on UDP port {}", id, 5000 + id);
    tracing::info!("📡 Peers connected: {:?}", peers.iter().map(|p| p.0).collect::<Vec<_>>());
    tracing::info!("===================================");

    // Give node 1 a slight advantage to reliably become the first leader for our demo
    let timeout = if id == 1 { 300 } else { 450 + id * 50 };
    let mut node = RaftNode::new(RaftNodeId(id), peers, Duration::from_millis(timeout));
    
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", 5000 + id))?;
    socket.set_nonblocking(true)?;

    let mut buf = [0; 65535];
    let mut last_tick = Instant::now();
    let mut last_role = node.role;
    let mut db_adapter = SqliteAdapter::new(&format!("./data/titan_node_{}.db", id)).unwrap_or_else(|_| {
        fs::create_dir_all("./data").unwrap();
        SqliteAdapter::new(&format!("./data/titan_node_{}.db", id)).unwrap()
    });
    db_adapter.execute_write("CREATE TABLE IF NOT EXISTS demo (id INTEGER PRIMARY KEY, msg TEXT);")?;

    let mut apply_loop = raft_core::DeterministicApplyLoop::new();

    loop {
        // Handle incoming UDP
        while let Ok((amt, _src)) = socket.recv_from(&mut buf) {
            let msg_str = String::from_utf8_lossy(&buf[..amt]);
            
            // Client API
            if msg_str.starts_with("SQL:") {
                let sql = msg_str[4..].to_string();
                if let Some(idx) = node.append_local_entry(sql.as_bytes().to_vec()) {
                    tracing::info!("✅ Client Request Accepted: Adding to Raft Log at Index {}", idx);
                } else {
                    tracing::warn!("❌ Rejected Client Request: I am not the Leader!");
                }
            } else {
                // Internal Raft API
                if let Ok(raft_msg) = serde_json::from_str::<RaftMessage>(&msg_str) {
                    node.step(raft_msg);
                }
            }
        }

        // Timer ticks
        let now = Instant::now();
        if now.duration_since(last_tick) >= Duration::from_millis(10) {
            node.tick(now);
            last_tick = now;
        }

        // Role monitoring
        if node.role != last_role {
            tracing::info!("👑 Node {} transitioning to {:?}", id, node.role);
            last_role = node.role;
        }

        // Send outbox mesages
        for out_msg in node.take_outbox() {
            let dest_port = 5000 + out_msg.to.0;
            if let Ok(json) = serde_json::to_string(&out_msg) {
                let _ = socket.send_to(json.as_bytes(), format!("127.0.0.1:{}", dest_port));
            }
        }

        // State Machine Apply Loop
        let committed_entries = apply_loop.apply(&mut node);
        for entry in committed_entries {
            let sql = String::from_utf8_lossy(&entry.payload).into_owned();
            tracing::info!("💾 Node {} Applying SQL to Local SQLite Disk: {}", id, sql);
            if let Err(e) = db_adapter.execute_write(&sql) {
                tracing::error!("Failed to apply SQL: {}", e);
            }
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

fn run_client(port: u16, command: String) -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let msg = format!("SQL:{}", command);
    socket.send_to(msg.as_bytes(), format!("127.0.0.1:{}", port))?;
    tracing::info!("Sent: {}", msg);
    Ok(())
}


fn run_generate_fixture(output: &str) -> Result<()> {
    SqliteAdapter::generate_wal_fixture(output)?;
    Ok(())
}

fn run_parse_fixture(path: &str) -> Result<()> {
    let bytes = fs::read(path)?;
    let repl = WalReplicator::new();
    match repl.parse_wal_bytes(&bytes) {
        Ok((_h, frames)) => {
            tracing::info!("Parsed binary WAL fixture: {} frames", frames.len());
            Ok(())
        }
        Err(_) => Ok(())
    }
}
