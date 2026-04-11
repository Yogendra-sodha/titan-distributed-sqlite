use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{Method, Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use raft_core::{rpc::RaftMessage, store::SqliteStateStore, DeterministicApplyLoop, RaftNode, RaftNodeId, Role};
use serde::{Deserialize, Serialize};
use sqlite_adapter::SqliteAdapter;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use wal_replicator::WalReplicator;

// ── Shared State between UDP Raft loop and HTTP server ──

struct SharedState {
    node: Mutex<RaftNode>,
    db: Mutex<SqliteAdapter>,
    node_id: u64,
    /// Pending write confirmations: log_index → oneshot sender
    pending_confirms: Mutex<HashMap<u64, oneshot::Sender<WriteConfirmation>>>,
    /// API key for authentication (None = no auth required)
    api_key: Option<String>,
}

#[derive(Debug, Clone)]
struct WriteConfirmation {
    log_index: u64,
    committed: bool,
}

// ── HTTP API Types ──

#[derive(Deserialize)]
struct ExecuteRequest {
    sql: String,
    #[serde(default = "default_true")]
    wait_for_commit: bool,
}

#[derive(Deserialize)]
struct TransactionRequest {
    statements: Vec<String>,
    #[serde(default = "default_true")]
    wait_for_commit: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize)]
struct ExecuteResponse {
    success: bool,
    message: String,
    log_index: Option<u64>,
    committed: bool,
}

#[derive(Serialize)]
struct TransactionResponse {
    success: bool,
    message: String,
    log_indices: Vec<u64>,
    committed: bool,
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
    persistent_state: bool,
    auth_enabled: bool,
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

    // ── API Key Authentication ──
    let api_key = env::var("TITAN_API_KEY").ok();
    let auth_status = if api_key.is_some() { "ENABLED" } else { "DISABLED (set TITAN_API_KEY to enable)" };

    tracing::info!("===================================");
    tracing::info!("🚀 Starting Titan Node {} ", id);
    tracing::info!("📡 Peers: {:?}", peers.iter().map(|p| p.0).collect::<Vec<_>>());
    tracing::info!("🌐 UDP Raft port: {}", udp_port);
    tracing::info!("🌐 HTTP API port: {}", http_port);
    tracing::info!("🔐 Auth: {}", auth_status);
    tracing::info!("===================================");

    // Ensure data directory exists
    fs::create_dir_all("./data")?;

    // ── Persistent Raft State Store ──
    let raft_store_path = format!("./data/titan_node_{}_raft.db", id);
    let store = SqliteStateStore::new(&raft_store_path);
    tracing::info!("💾 Raft state store: {}", raft_store_path);

    // Give node 1 a slight advantage to reliably become the first leader
    let timeout = if id == 1 { 300 } else { 450 + id * 50 };
    let node = RaftNode::new_with_store(
        RaftNodeId(id),
        peers,
        Duration::from_millis(timeout),
        Box::new(store),
    );

    // Log recovered state
    tracing::info!(
        "📂 Recovered state: term={}, log_len={}, commit_index={}",
        node.current_term,
        node.log.len(),
        node.commit_index
    );

    let db_adapter = SqliteAdapter::new(&format!("./data/titan_node_{}.db", id)).unwrap_or_else(|_| {
        SqliteAdapter::new(&format!("./data/titan_node_{}.db", id)).unwrap()
    });
    db_adapter
        .execute_write("CREATE TABLE IF NOT EXISTS demo (id INTEGER PRIMARY KEY, msg TEXT);")
        ?;

    // If recovering, the apply loop starts from last_applied (= commit_index on restart)
    let recovered_last_applied = node.last_applied;

    let shared = Arc::new(SharedState {
        node: Mutex::new(node),
        db: Mutex::new(db_adapter),
        node_id: id,
        pending_confirms: Mutex::new(HashMap::new()),
        api_key,
    });

    // ── Spawn the UDP Raft loop in a background thread ──
    let raft_shared = Arc::clone(&shared);
    std::thread::spawn(move || {
        run_raft_loop(id, raft_shared, recovered_last_applied);
    });

    // ── Run the HTTP API on the main thread with tokio ──
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        // CORS layer so the dashboard on :8001 can call APIs on :8002, :8003
        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

        let shared_for_auth = Arc::clone(&shared);
        let app = Router::new()
            .route("/", get(handle_dashboard))
            .route("/execute", post(handle_execute))
            .route("/query", get(handle_query))
            .route("/transaction", post(handle_transaction))
            .route("/status", get(handle_status))
            .layer(middleware::from_fn(move |req, next| {
                auth_middleware(Arc::clone(&shared_for_auth), req, next)
            }))
            .layer(cors)
            .with_state(Arc::clone(&shared));

        // Check if TLS is enabled
        let tls_enabled = env::var("TITAN_TLS").unwrap_or_default() == "1";

        if tls_enabled {
            // Generate or load self-signed certificates
            let cert_path = "./data/cert.pem";
            let key_path = "./data/key.pem";

            if !std::path::Path::new(cert_path).exists() {
                tracing::info!("🔒 Generating self-signed TLS certificate...");
                let cert = rcgen::generate_simple_self_signed(vec![
                    "localhost".to_string(),
                    "127.0.0.1".to_string(),
                ]).expect("Failed to generate certificate");

                fs::write(cert_path, cert.cert.pem()).expect("Failed to write cert.pem");
                fs::write(key_path, cert.key_pair.serialize_pem()).expect("Failed to write key.pem");
                tracing::info!("🔒 Certificate saved to {} and {}", cert_path, key_path);
            }

            let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert_path, key_path)
                .await
                .expect("Failed to load TLS certificates");

            tracing::info!("🔒 HTTPS API live at https://127.0.0.1:{}", http_port);

            axum_server::bind_rustls(
                format!("0.0.0.0:{}", http_port).parse().unwrap(),
                tls_config,
            )
            .serve(app.into_make_service())
            .await
            .unwrap();
        } else {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", http_port))
                .await
                .unwrap();

            tracing::info!("🌐 HTTP API live at http://127.0.0.1:{}", http_port);
            axum::serve(listener, app).await.unwrap();
        }
    });

    Ok(())
}

// ── UDP Raft Loop (runs in background thread) ──

fn run_raft_loop(id: u64, shared: Arc<SharedState>, recovered_last_applied: u64) {
    let socket = UdpSocket::bind(format!("127.0.0.1:{}", 5000 + id)).unwrap();
    socket.set_nonblocking(true).unwrap();

    let mut buf = [0; 65535];
    let mut last_tick = Instant::now();
    let mut last_role = Role::Follower;
    let mut apply_loop = DeterministicApplyLoop::new_from(recovered_last_applied);

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

        // State Machine Apply Loop + Write Confirmation
        {
            let mut node = shared.node.lock().unwrap();
            let committed_entries = apply_loop.apply(&mut node);
            let db = shared.db.lock().unwrap();
            let mut confirms = shared.pending_confirms.lock().unwrap();

            for entry in committed_entries {
                let sql = String::from_utf8_lossy(&entry.payload).into_owned();
                tracing::info!("💾 Node {} Applying SQL to Local SQLite Disk: {}", id, sql);
                if let Err(e) = db.execute_write(&sql) {
                    tracing::error!("Failed to apply SQL: {}", e);
                }

                // Notify any waiting HTTP handler that this entry is committed
                if let Some(sender) = confirms.remove(&entry.index) {
                    let _ = sender.send(WriteConfirmation {
                        log_index: entry.index,
                        committed: true,
                    });
                }
            }
        }

        std::thread::sleep(Duration::from_millis(2));
    }
}

// ── Auth Middleware ──

async fn auth_middleware(
    shared: Arc<SharedState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Always allow: GET /status (for health checks), GET / (dashboard)
    let path = req.uri().path().to_string();
    if path == "/" || path == "/status" {
        return next.run(req).await;
    }

    // If no API key is configured, allow everything
    let api_key = match &shared.api_key {
        Some(key) => key.clone(),
        None => return next.run(req).await,
    };

    // Check Authorization header: "Bearer <key>"
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if auth_header == format!("Bearer {}", api_key) {
        return next.run(req).await;
    }

    // Also accept ?api_key=<key> query param (for curl convenience)
    if let Some(query) = req.uri().query() {
        if query.contains(&format!("api_key={}", api_key)) {
            return next.run(req).await;
        }
    }

    (StatusCode::UNAUTHORIZED, Json(serde_json::json!({
        "success": false,
        "message": "Unauthorized. Provide Authorization: Bearer <key> header or ?api_key=<key> param."
    }))).into_response()
}

// ── HTTP Handlers ──

async fn handle_dashboard() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

/// Validate SQL syntax by attempting to prepare the statement.
/// Returns Ok(()) if valid, Err(message) if invalid.
fn validate_sql(db: &SqliteAdapter, sql: &str) -> std::result::Result<(), String> {
    // Use the read connection to just prepare (not execute) the statement
    // This catches syntax errors without modifying data
    db.validate_sql(sql).map_err(|e| format!("SQL syntax error: {}", e))
}

async fn handle_execute(
    State(shared): State<Arc<SharedState>>,
    Json(req): Json<ExecuteRequest>,
) -> Json<ExecuteResponse> {
    // Validate SQL syntax before sending through Raft
    {
        let db = shared.db.lock().unwrap();
        if let Err(e) = validate_sql(&db, &req.sql) {
            return Json(ExecuteResponse {
                success: false,
                message: e,
                log_index: None,
                committed: false,
            });
        }
    }

    let (log_index, rx) = {
        let mut node = shared.node.lock().unwrap();

        if node.role != Role::Leader {
            return Json(ExecuteResponse {
                success: false,
                message: format!(
                    "This node is not the Leader. Current role: {:?}. Try another node.",
                    node.role
                ),
                log_index: None,
                committed: false,
            });
        }

        match node.append_local_entry(req.sql.as_bytes().to_vec()) {
            Some(idx) => {
                tracing::info!("✅ HTTP Execute Accepted: Raft Log Index {}", idx);

                if req.wait_for_commit {
                    // Create a oneshot channel for commit notification
                    let (tx, rx) = oneshot::channel();
                    shared.pending_confirms.lock().unwrap().insert(idx, tx);
                    (idx, Some(rx))
                } else {
                    (idx, None)
                }
            }
            None => {
                return Json(ExecuteResponse {
                    success: false,
                    message: "Failed to append entry. Node may have lost leadership.".to_string(),
                    log_index: None,
                    committed: false,
                });
            }
        }
    };

    // If not waiting for commit, return immediately
    if rx.is_none() {
        return Json(ExecuteResponse {
            success: true,
            message: format!("SQL accepted and replicating across cluster (log index {})", log_index),
            log_index: Some(log_index),
            committed: false,
        });
    }

    // Wait for commit confirmation with a 5 second timeout
    let rx = rx.unwrap();
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(confirmation)) => Json(ExecuteResponse {
            success: true,
            message: format!(
                "✅ SQL committed across cluster at log index {}",
                confirmation.log_index
            ),
            log_index: Some(confirmation.log_index),
            committed: true,
        }),
        Ok(Err(_)) => Json(ExecuteResponse {
            success: true,
            message: format!(
                "SQL accepted (log index {}), but confirmation channel dropped. Entry may still commit.",
                log_index
            ),
            log_index: Some(log_index),
            committed: false,
        }),
        Err(_) => Json(ExecuteResponse {
            success: true,
            message: format!(
                "SQL accepted (log index {}), but commit confirmation timed out after 5s. Entry is still replicating.",
                log_index
            ),
            log_index: Some(log_index),
            committed: false,
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

async fn handle_transaction(
    State(shared): State<Arc<SharedState>>,
    Json(req): Json<TransactionRequest>,
) -> Json<TransactionResponse> {
    if req.statements.is_empty() {
        return Json(TransactionResponse {
            success: false,
            message: "No statements provided".to_string(),
            log_indices: vec![],
            committed: false,
        });
    }

    // Validate ALL statements first
    {
        let db = shared.db.lock().unwrap();
        for (i, sql) in req.statements.iter().enumerate() {
            if let Err(e) = validate_sql(&db, sql) {
                return Json(TransactionResponse {
                    success: false,
                    message: format!("Statement {} failed validation: {}", i + 1, e),
                    log_indices: vec![],
                    committed: false,
                });
            }
        }
    }

    // Wrap all statements in BEGIN/COMMIT and send as single Raft entry
    let mut wrapped = String::from("BEGIN TRANSACTION;\n");
    for sql in &req.statements {
        wrapped.push_str(sql.trim_end_matches(';'));
        wrapped.push_str(";\n");
    }
    wrapped.push_str("COMMIT;");

    let (log_index, rx) = {
        let mut node = shared.node.lock().unwrap();

        if node.role != Role::Leader {
            return Json(TransactionResponse {
                success: false,
                message: format!("This node is not the Leader. Current role: {:?}.", node.role),
                log_indices: vec![],
                committed: false,
            });
        }

        match node.append_local_entry(wrapped.as_bytes().to_vec()) {
            Some(idx) => {
                if req.wait_for_commit {
                    let (tx, rx) = oneshot::channel();
                    shared.pending_confirms.lock().unwrap().insert(idx, tx);
                    (idx, Some(rx))
                } else {
                    (idx, None)
                }
            }
            None => {
                return Json(TransactionResponse {
                    success: false,
                    message: "Failed to append entry.".to_string(),
                    log_indices: vec![],
                    committed: false,
                });
            }
        }
    };

    if rx.is_none() {
        return Json(TransactionResponse {
            success: true,
            message: format!("Transaction ({} statements) accepted at log index {}", req.statements.len(), log_index),
            log_indices: vec![log_index],
            committed: false,
        });
    }

    let rx = rx.unwrap();
    match tokio::time::timeout(Duration::from_secs(5), rx).await {
        Ok(Ok(confirmation)) => Json(TransactionResponse {
            success: true,
            message: format!("Transaction ({} statements) committed at log index {}", req.statements.len(), confirmation.log_index),
            log_indices: vec![confirmation.log_index],
            committed: true,
        }),
        Ok(Err(_)) => Json(TransactionResponse {
            success: true,
            message: "Transaction accepted but confirmation channel dropped.".to_string(),
            log_indices: vec![log_index],
            committed: false,
        }),
        Err(_) => Json(TransactionResponse {
            success: true,
            message: "Transaction accepted but commit timed out.".to_string(),
            log_indices: vec![log_index],
            committed: false,
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
        persistent_state: true,
        auth_enabled: shared.api_key.is_some(),
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
