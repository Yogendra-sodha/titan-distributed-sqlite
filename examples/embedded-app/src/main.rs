use axum::{
    routing::{get, post},
    Router, Json, Extension,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use raft_core::{RaftNode, RaftNodeId};
use sqlite_adapter::SqliteAdapter;
use tokio::net::TcpListener;

// This is how you embed Titan directly into your own Rust Backends!
// You just hold the `RaftNode` and `SqliteAdapter` in your web server's application state.

struct AppState {
    db: Mutex<SqliteAdapter>,
    raft: Mutex<RaftNode>, // Usually you'd use a channel to communicate with a dedicated Raft thread
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[tokio::main]
async fn main() {
    // 1. Initialize standard SQLite Adapter mapped to your local app's data folder
    let mut db = SqliteAdapter::new("./app_data.db").unwrap();
    db.execute_write("CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT, email TEXT);").unwrap();

    // 2. Initialize Raft Node as a library
    // Let's pretend this is Node 1 of a 3-node cluster
    let peers = vec![RaftNodeId(2), RaftNodeId(3)];
    let raft = RaftNode::new(RaftNodeId(1), peers, Duration::from_millis(300));
    
    let shared_state = Arc::new(AppState {
        db: Mutex::new(db),
        raft: Mutex::new(raft),
    });

    // 3. Build your normal Web API (using Axum, Actix, or whatever you want)
    let app = Router::new()
        .route("/users", post(create_user))
        .route("/users", get(get_users))
        .layer(Extension(shared_state));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("🚀 Production App running on http://127.0.0.1:3000");
    axum::serve(listener, app).await.unwrap();
}

// 4. In your HTTP routes, you just send the SQL through the Raft Consensus first!
async fn create_user(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<CreateUser>,
) -> &'static str {
    
    // Instead of executing SQL immediately, give it to Titan's Raft core
    let sql = format!("INSERT INTO users (name, email) VALUES ('{}', '{}')", payload.name, payload.email);
    
    let mut raft = state.raft.lock().unwrap();
    
    // If we are the leader, Titan accepts it and starts replicating to other servers.
    // (In a real app, you'd await the commit confirmation here before returning HTTP 200)
    if let Some(_) = raft.append_local_entry(sql.as_bytes().to_vec()) {
        "User creation initiated and replicating across cluster!"
    } else {
        "Error: This node is not the Leader. Please redirect to Leader."
    }
}

async fn get_users(Extension(state): Extension<Arc<AppState>>) -> &'static str {
    // Reads can happen immediately from the hyper-fast local SQLite disk wrapper!
    let db = state.db.lock().unwrap();
    let _results = db.execute_read("SELECT * FROM users;").unwrap();
    "Fetched users from local fast disk!"
}
