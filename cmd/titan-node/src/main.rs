use anyhow::Result;
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use raft_core::{rpc::RaftMessage, RaftNode, RaftNodeId, Role};
use serde::{Deserialize, Serialize};
use sqlite_adapter::SqliteAdapter;
use std::env;
use std::fs;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use wal_replicator::WalReplicator;

// ── Shared State between UDP Raft loop and HTTP server ──

struct SharedState {
    node: Mutex<RaftNode>,
    db: Mutex<SqliteAdapter>,
    node_id: u64,
}

// ── HTTP API Types ──

#[derive(Deserialize)]
struct ExecuteRequest {
    sql: String,
}

#[derive(Serialize)]
struct ExecuteResponse {
    success: bool,
    message: String,
    log_index: Option<u64>,
}

#[derive(Deserialize)]
struct QueryParams {
    sql: String,
}

#[derive(Serialize)]
struct QueryResponse {
    success: bool,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    error: Option<String>,
}

#[derive(Serialize)]
struct StatusResponse {
    node_id: u64,
    role: String,
    term: u64,
    commit_index: u64,
    last_applied: u64,
    log_length: usize,
    peers: Vec<u64>,
    udp_port: u64,
    http_port: u64,
}

// ── Main ──

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("titan_node=info".parse().unwrap()),
        )
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
    tracing::info!("");
    tracing::info!("HTTP API available when running:");
    tracing::info!("  POST http://127.0.0.1:<8000+id>/execute  {{\"sql\": \"...\"}}");
    tracing::info!("  GET  http://127.0.0.1:<8000+id>/query?sql=SELECT...");
    tracing::info!("  GET  http://127.0.0.1:<8000+id>/status");

    Ok(())
}

fn run_server(id: u64, peers: Vec<RaftNodeId>) -> Result<()> {
    let http_port = 8000 + id;
    let udp_port = 5000 + id;

    tracing::info!("===================================");
    tracing::info!("🚀 Starting Titan Node {} ", id);
    tracing::info!("📡 Peers: {:?}", peers.iter().map(|p| p.0).collect::<Vec<_>>());
    tracing::info!("🌐 UDP Raft port: {}", udp_port);
    tracing::info!("🌐 HTTP API port: {}", http_port);
    tracing::info!("===================================");

    // Give node 1 a slight advantage to reliably become the first leader
    let timeout = if id == 1 { 300 } else { 450 + id * 50 };
    let node = RaftNode::new(RaftNodeId(id), peers, Duration::from_millis(timeout));

    let db_adapter = SqliteAdapter::new(&format!("./data/titan_node_{}.db", id)).unwrap_or_else(|_| {
        fs::create_dir_all("./data").unwrap();
        SqliteAdapter::new(&format!("./data/titan_node_{}.db", id)).unwrap()
    });
    db_adapter
        .execute_write("CREATE TABLE IF NOT EXISTS demo (id INTEGER PRIMARY KEY, msg TEXT);")
        ?;

    let shared = Arc::new(SharedState {
        node: Mutex::new(node),
        db: Mutex::new(db_adapter),
        node_id: id,
    });

    // ── Spawn the UDP Raft loop in a background thread ──
    let raft_shared = Arc::clone(&shared);
    std::thread::spawn(move || {
        run_raft_loop(id, raft_shared);
    });

    // ── Run the HTTP API on the main thread with tokio ──
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let app = Router::new()
            .route("/execute", post(handle_execute))
            .route("/query", get(handle_query))
            .route("/status", get(handle_status))
            .with_state(Arc::clone(&shared));

        let listener = TcpListener::bind(format!("0.0.0.0:{}", http_port))
            .await
            .unwrap();

        tracing::info!("🌐 HTTP API live at http://127.0.0.1:{}", http_port);
        axum::serve(listener, app).await.unwrap();
    });

    Ok(())
}

// ── UDP Raft Loop (runs in background thread) ──

fn run_raft_loop(id: u64, shared: Arc<SharedState>) {
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", 5000 + id)).unwrap();
    socket.set_nonblocking(true).unwrap();

    let mut buf = [0; 65535];
    let mut last_tick = Instant::now();
    let mut last_role = Role::Follower;
    let mut apply_loop = raft_core::DeterministicApplyLoop::new();

    loop {
        // Handle incoming UDP
        while let Ok((amt, _src)) = socket.recv_from(&mut buf) {
            let msg_str = String::from_utf8_lossy(&buf[..amt]);

            // Client API (legacy UDP client)
            if msg_str.starts_with("SQL:") {
                let sql = msg_str[4..].to_string();
                let mut node = shared.node.lock().unwrap();
                if let Some(idx) = node.append_local_entry(sql.as_bytes().to_vec()) {
                    tracing::info!("✅ Client Request Accepted: Adding to Raft Log at Index {}", idx);
                } else {
                    tracing::warn!("❌ Rejected Client Request: I am not the Leader!");
                }
            } else {
                // Internal Raft API
                if let Ok(raft_msg) = serde_json::from_str::<RaftMessage>(&msg_str) {
                    let mut node = shared.node.lock().unwrap();
                    node.step(raft_msg);
                }
            }
        }

        // Timer ticks
        let now = Instant::now();
        if now.duration_since(last_tick) >= Duration::from_millis(10) {
            let mut node = shared.node.lock().unwrap();
            node.tick(now);
            last_tick = now;
        }

        // Role monitoring
        {
            let node = shared.node.lock().unwrap();
            if node.role != last_role {
                tracing::info!("👑 Node {} transitioning to {:?}", id, node.role);
                last_role = node.role;
            }
        }

        // Send outbox messages
        {
            let mut node = shared.node.lock().unwrap();
            for out_msg in node.take_outbox() {
                let dest_port = 5000 + out_msg.to.0;
                if let Ok(json) = serde_json::to_string(&out_msg) {
                    let _ = socket.send_to(json.as_bytes(), format!("127.0.0.1:{}", dest_port));
                }
            }
        }

        // State Machine Apply Loop
        {
            let mut node = shared.node.lock().unwrap();
            let committed_entries = apply_loop.apply(&mut node);
            let db = shared.db.lock().unwrap();
            for entry in committed_entries {
                let sql = String::from_utf8_lossy(&entry.payload).into_owned();
                tracing::info!("💾 Node {} Applying SQL to Local SQLite Disk: {}", id, sql);
                if let Err(e) = db.execute_write(&sql) {
                    tracing::error!("Failed to apply SQL: {}", e);
                }
            }
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

// ── HTTP Handlers ──

async fn handle_execute(
    State(shared): State<Arc<SharedState>>,
    Json(req): Json<ExecuteRequest>,
) -> Json<ExecuteResponse> {
    let mut node = shared.node.lock().unwrap();

    if node.role != Role::Leader {
        return Json(ExecuteResponse {
            success: false,
            message: format!(
                "This node is not the Leader. Current role: {:?}. Try another node.",
                node.role
            ),
            log_index: None,
        });
    }

    match node.append_local_entry(req.sql.as_bytes().to_vec()) {
        Some(idx) => {
            tracing::info!("✅ HTTP Execute Accepted: Raft Log Index {}", idx);
            Json(ExecuteResponse {
                success: true,
                message: format!("SQL accepted and replicating across cluster (log index {})", idx),
                log_index: Some(idx),
            })
        }
        None => Json(ExecuteResponse {
            success: false,
            message: "Failed to append entry. Node may have lost leadership.".to_string(),
            log_index: None,
        }),
    }
}

async fn handle_query(
    State(shared): State<Arc<SharedState>>,
    Query(params): Query<QueryParams>,
) -> Json<QueryResponse> {
    let db = shared.db.lock().unwrap();
    match db.execute_read_with_columns(&params.sql) {
        Ok((columns, rows)) => Json(QueryResponse {
            success: true,
            columns,
            rows,
            error: None,
        }),
        Err(e) => Json(QueryResponse {
            success: false,
            columns: vec![],
            rows: vec![],
            error: Some(e.to_string()),
        }),
    }
}

async fn handle_status(
    State(shared): State<Arc<SharedState>>,
) -> Json<StatusResponse> {
    let node = shared.node.lock().unwrap();
    let role_str = match node.role {
        Role::Leader => "Leader",
        Role::Follower => "Follower",
        Role::Candidate => "Candidate",
    };

    Json(StatusResponse {
        node_id: shared.node_id,
        role: role_str.to_string(),
        term: node.current_term,
        commit_index: node.commit_index,
        last_applied: node.last_applied,
        log_length: node.log.len(),
        peers: node.peers.iter().map(|p| p.0).collect(),
        udp_port: 5000 + shared.node_id,
        http_port: 8000 + shared.node_id,
    })
}

// ── Legacy commands ──

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
        Err(_) => Ok(()),
    }
}
