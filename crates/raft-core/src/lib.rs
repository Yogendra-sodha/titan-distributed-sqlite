use std::collections::HashMap;
use std::time::{Duration, Instant};

pub mod rpc;
pub mod store;

use rpc::{
    AppendEntriesArgs, AppendEntriesReply, MessagePayload, RaftMessage, RequestVoteArgs,
    RequestVoteReply,
};
use store::RaftStateStore;

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RaftNodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub index: u64,
    pub term: u64,
    pub payload: Vec<u8>,
}

pub struct RaftNode {
    pub node_id: RaftNodeId,
    pub peers: Vec<RaftNodeId>,
    pub current_term: u64,
    pub voted_for: Option<RaftNodeId>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub role: Role,
    pub election_timeout: Duration,
    pub election_deadline: Instant,
    pub log: Vec<LogEntry>,

    // Leader state
    pub next_index: HashMap<RaftNodeId, u64>,
    pub match_index: HashMap<RaftNodeId, u64>,

    // Candidate state
    pub votes_received: usize,

    // Outbox
    pub outbox: Vec<RaftMessage>,

    // Optional persistent store
    store: Option<Box<dyn RaftStateStore + Send>>,
}

impl std::fmt::Debug for RaftNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RaftNode")
            .field("node_id", &self.node_id)
            .field("role", &self.role)
            .field("current_term", &self.current_term)
            .field("commit_index", &self.commit_index)
            .field("log_len", &self.log.len())
            .finish()
    }
}

impl RaftNode {
    /// Create a new in-memory node (for tests and backward compatibility).
    pub fn new(node_id: RaftNodeId, peers: Vec<RaftNodeId>, election_timeout: Duration) -> Self {
        let now = Instant::now();
        Self {
            node_id,
            peers,
            current_term: 0,
            voted_for: None,
            commit_index: 0,
            last_applied: 0,
            role: Role::Follower,
            election_timeout,
            election_deadline: now + election_timeout,
            log: Vec::new(),
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            votes_received: 0,
            outbox: Vec::new(),
            store: None,
        }
    }

    /// Create a node backed by a persistent store. Recovers term, votedFor,
    /// log, and commit_index from the store on startup.
    pub fn new_with_store(
        node_id: RaftNodeId,
        peers: Vec<RaftNodeId>,
        election_timeout: Duration,
        store: Box<dyn RaftStateStore + Send>,
    ) -> Self {
        let now = Instant::now();

        // Recover persisted state
        let current_term = store.current_term();
        let voted_for = store.voted_for();
        let log: Vec<LogEntry> = store.entries().to_vec();
        let commit_index = store.commit_index();
        let last_applied = commit_index; // Will re-apply from commit_index on startup

        Self {
            node_id,
            peers,
            current_term,
            voted_for,
            commit_index,
            last_applied,
            role: Role::Follower, // Always start as follower
            election_timeout,
            election_deadline: now + election_timeout,
            log,
            next_index: HashMap::new(),
            match_index: HashMap::new(),
            votes_received: 0,
            outbox: Vec::new(),
            store: Some(store),
        }
    }

    /// Persist current term to the store (if present).
    fn persist_term(&mut self) {
        if let Some(ref mut store) = self.store {
            store.set_current_term(self.current_term);
        }
    }

    /// Persist voted_for to the store (if present).
    fn persist_voted_for(&mut self) {
        if let Some(ref mut store) = self.store {
            store.set_voted_for(self.voted_for);
        }
    }

    /// Persist a log entry to the store (if present).
    fn persist_log_append(&mut self, entry: &LogEntry) {
        if let Some(ref mut store) = self.store {
            store.append_entry(entry.clone());
        }
    }

    /// Persist log truncation to the store (if present).
    fn persist_log_truncate(&mut self, from_index: u64) {
        if let Some(ref mut store) = self.store {
            store.truncate_from(from_index);
        }
    }

    /// Persist commit_index to the store (if present).
    fn persist_commit_index(&mut self) {
        if let Some(ref mut store) = self.store {
            store.set_commit_index(self.commit_index);
        }
    }

    pub fn reset_election_deadline(&mut self) {
        // In a real system we'd add some random jitter to the timeout.
        self.election_deadline = Instant::now() + self.election_timeout;
    }

    pub fn become_follower(&mut self, term: u64) {
        self.role = Role::Follower;
        self.current_term = term;
        self.voted_for = None;
        self.persist_term();
        self.persist_voted_for();
        self.reset_election_deadline();
    }

    pub fn become_candidate(&mut self) {
        self.current_term += 1;
        self.role = Role::Candidate;
        self.voted_for = Some(self.node_id);
        self.votes_received = 1;
        self.persist_term();
        self.persist_voted_for();
        self.reset_election_deadline();

        let lli = self.log.last().map(|e| e.index).unwrap_or(0);
        let llt = self.log.last().map(|e| e.term).unwrap_or(0);

        let peers = self.peers.clone();
        for peer in peers {
            self.send_message(
                peer,
                MessagePayload::RequestVote(RequestVoteArgs {
                    term: self.current_term,
                    candidate_id: self.node_id,
                    last_log_index: lli,
                    last_log_term: llt,
                }),
            );
        }
    }

    pub fn become_leader(&mut self) {
        self.role = Role::Leader;
        let next_idx = self.log.last().map(|e| e.index + 1).unwrap_or(1);
        for peer in &self.peers {
            self.next_index.insert(*peer, next_idx);
            self.match_index.insert(*peer, 0);
        }
        self.broadcast_append_entries();
    }

    pub fn broadcast_append_entries(&mut self) {
        for peer in self.peers.clone() {
            self.send_append_entries(peer);
        }
    }

    pub fn send_append_entries(&mut self, peer: RaftNodeId) {
        let nidx = *self.next_index.get(&peer).unwrap_or(&1);
        let prev_log_index = if nidx > 0 { nidx - 1 } else { 0 };
        let prev_log_term = if prev_log_index > 0 && prev_log_index as usize <= self.log.len() {
            self.log[prev_log_index as usize - 1].term
        } else {
            0
        };

        let entries = if nidx as usize <= self.log.len() {
            self.log[(nidx as usize - 1)..].to_vec()
        } else {
            Vec::new()
        };

        self.send_message(
            peer,
            MessagePayload::AppendEntries(AppendEntriesArgs {
                term: self.current_term,
                leader_id: self.node_id,
                prev_log_index,
                prev_log_term,
                entries,
                leader_commit: self.commit_index,
            }),
        );
    }

    fn send_message(&mut self, to: RaftNodeId, payload: MessagePayload) {
        self.outbox.push(RaftMessage {
            from: self.node_id,
            to,
            payload,
        });
    }

    pub fn tick(&mut self, now: Instant) {
        if self.role != Role::Leader && now >= self.election_deadline {
            self.become_candidate();
        } else if self.role == Role::Leader {
            // Heartbeat
            self.broadcast_append_entries();
        }
    }

    pub fn step(&mut self, msg: RaftMessage) {
        // Update term if needed
        let msg_term = match &msg.payload {
            MessagePayload::RequestVote(args) => args.term,
            MessagePayload::RequestVoteReply(reply) => reply.term,
            MessagePayload::AppendEntries(args) => args.term,
            MessagePayload::AppendEntriesReply(reply) => reply.term,
            MessagePayload::InstallSnapshot(args) => args.term,
            MessagePayload::InstallSnapshotReply(reply) => reply.term,
        };

        if msg_term > self.current_term {
            self.become_follower(msg_term);
        }

        match msg.payload {
            MessagePayload::RequestVote(args) => {
                let current_term = self.current_term;
                let mut vote_granted = false;

                let lli = self.log.last().map(|e| e.index).unwrap_or(0);
                let llt = self.log.last().map(|e| e.term).unwrap_or(0);

                let log_ok = args.last_log_term > llt || (args.last_log_term == llt && args.last_log_index >= lli);

                if args.term == current_term && (self.voted_for.is_none() || self.voted_for == Some(args.candidate_id)) && log_ok {
                    vote_granted = true;
                    self.voted_for = Some(args.candidate_id);
                    self.persist_voted_for();
                    self.reset_election_deadline();
                }

                self.send_message(
                    msg.from,
                    MessagePayload::RequestVoteReply(RequestVoteReply {
                        term: current_term,
                        vote_granted,
                    }),
                );
            }
            MessagePayload::RequestVoteReply(reply) => {
                if self.role == Role::Candidate && reply.term == self.current_term && reply.vote_granted {
                    self.votes_received += 1;
                    if self.votes_received > (self.peers.len() + 1) / 2 {
                        self.become_leader();
                    }
                }
            }
            MessagePayload::AppendEntries(args) => {
                let mut success = false;
                let mut match_index = 0;

                if args.term == self.current_term {
                    self.role = Role::Follower; // recognize leader
                    self.reset_election_deadline();

                    // Check prev log
                    let prev_ok = args.prev_log_index == 0 || (
                        args.prev_log_index as usize <= self.log.len() &&
                        self.log[args.prev_log_index as usize - 1].term == args.prev_log_term
                    );

                    if prev_ok {
                        success = true;
                        
                        // Append entries
                        let mut insert_idx = args.prev_log_index as usize;
                        for entry in args.entries {
                            if insert_idx < self.log.len() {
                                if self.log[insert_idx].term != entry.term {
                                    // Truncate conflicting entries — persist first
                                    let truncate_from = self.log[insert_idx].index;
                                    self.persist_log_truncate(truncate_from);
                                    self.log.truncate(insert_idx);
                                    self.persist_log_append(&entry);
                                    self.log.push(entry);
                                }
                            } else {
                                self.persist_log_append(&entry);
                                self.log.push(entry);
                            }
                            insert_idx += 1;
                        }
                        
                        match_index = args.prev_log_index + insert_idx as u64 - args.prev_log_index as u64;

                        if args.leader_commit > self.commit_index {
                            self.commit_index = std::cmp::min(args.leader_commit, self.log.last().map(|e| e.index).unwrap_or(0));
                            self.persist_commit_index();
                        }
                    }
                }

                self.send_message(
                    msg.from,
                    MessagePayload::AppendEntriesReply(AppendEntriesReply {
                        term: self.current_term,
                        success,
                        match_index,
                    }),
                );
            }
            MessagePayload::AppendEntriesReply(reply) => {
                if self.role == Role::Leader && reply.term == self.current_term {
                    if reply.success {
                        self.match_index.insert(msg.from, reply.match_index);
                        self.next_index.insert(msg.from, reply.match_index + 1);

                        // Update commit index
                        let mut matches: Vec<u64> = self.match_index.values().cloned().collect();
                        matches.push(self.log.last().map(|e| e.index).unwrap_or(0)); // self
                        matches.sort_unstable();
                        
                        let nci = matches[matches.len() - (self.peers.len() + 1) / 2];

                        if nci > self.commit_index && nci as usize <= self.log.len() && self.log[nci as usize - 1].term == self.current_term {
                            self.commit_index = nci;
                            self.persist_commit_index();
                        }
                    } else {
                        // decrement next_index and retry
                        let nidx = self.next_index.get_mut(&msg.from).unwrap();
                        if *nidx > 1 {
                            *nidx -= 1;
                            self.send_append_entries(msg.from);
                        }
                    }
                }
            }
            MessagePayload::InstallSnapshot(_) => {
                // Phase 4 stub
            }
            MessagePayload::InstallSnapshotReply(_) => {
                // Phase 4 stub
            }
        }
    }

    pub fn append_local_entry(&mut self, payload: Vec<u8>) -> Option<u64> {
        if self.role != Role::Leader {
            return None;
        }

        let index = self.log.last().map(|e| e.index + 1).unwrap_or(1);
        let entry = LogEntry {
            index,
            term: self.current_term,
            payload,
        };
        self.persist_log_append(&entry);
        self.log.push(entry);

        self.broadcast_append_entries();
        Some(index)
    }

    pub fn take_outbox(&mut self) -> Vec<RaftMessage> {
        std::mem::take(&mut self.outbox)
    }
}

pub struct DeterministicApplyLoop {
    pub last_applied: u64,
}

impl DeterministicApplyLoop {
    pub fn new() -> Self {
        Self { last_applied: 0 }
    }

    /// Create an apply loop that starts from a given index (for restart recovery).
    pub fn new_from(last_applied: u64) -> Self {
        Self { last_applied }
    }

    pub fn apply(&mut self, node: &mut RaftNode) -> Vec<LogEntry> {
        let mut applied = Vec::new();
        while self.last_applied < node.commit_index {
            self.last_applied += 1;
            node.last_applied = self.last_applied;
            let entry = node.log[self.last_applied as usize - 1].clone();
            applied.push(entry);
        }
        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn single_node_bootstrap() {
        let r = RaftNode::new(RaftNodeId(1), vec![], Duration::from_millis(300));
        assert_eq!(r.role, Role::Follower);
    }

    #[test]
    fn three_node_cluster_election_and_replicate() {
        let mut n1 = RaftNode::new(RaftNodeId(1), vec![RaftNodeId(2), RaftNodeId(3)], Duration::from_millis(100));
        let mut n2 = RaftNode::new(RaftNodeId(2), vec![RaftNodeId(1), RaftNodeId(3)], Duration::from_millis(150));
        let mut n3 = RaftNode::new(RaftNodeId(3), vec![RaftNodeId(1), RaftNodeId(2)], Duration::from_millis(200));

        // Node 1 times out first
        n1.tick(Instant::now() + Duration::from_millis(110));
        assert_eq!(n1.role, Role::Candidate);

        let mut msgs = VecDeque::new();
        msgs.extend(n1.take_outbox());

        let deliver = |msgs: &mut VecDeque<RaftMessage>, nodes: &mut [&mut RaftNode]| {
            while let Some(msg) = msgs.pop_front() {
                for node in nodes.iter_mut() {
                    if node.node_id == msg.to {
                        node.step(msg.clone());
                        msgs.extend(node.take_outbox());
                    }
                }
            }
        };

        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);

        assert_eq!(n1.role, Role::Leader);
        assert_eq!(n2.role, Role::Follower);
        assert_eq!(n3.role, Role::Follower);
        assert_eq!(n1.current_term, 1);

        // Client appends an entry to Leader n1
        let idx = n1.append_local_entry(b"SET x=1".to_vec()).unwrap();
        assert_eq!(idx, 1);

        msgs.extend(n1.take_outbox());
        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);

        // Tick leader to send heartbeat and propagate the new commit_index
        n1.tick(Instant::now());
        msgs.extend(n1.take_outbox());
        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);

        assert_eq!(n1.commit_index, 1);
        assert_eq!(n2.commit_index, 1);
        assert_eq!(n3.commit_index, 1);

        let mut applier = DeterministicApplyLoop::new();
        let applied = applier.apply(&mut n2);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].payload, b"SET x=1");
    }
    #[test]
    fn test_leader_failover_and_recovery() {
        let mut n1 = RaftNode::new(RaftNodeId(1), vec![RaftNodeId(2), RaftNodeId(3)], Duration::from_millis(100));
        let mut n2 = RaftNode::new(RaftNodeId(2), vec![RaftNodeId(1), RaftNodeId(3)], Duration::from_millis(150));
        let mut n3 = RaftNode::new(RaftNodeId(3), vec![RaftNodeId(1), RaftNodeId(2)], Duration::from_millis(200));

        // N1 becomes leader
        n1.tick(Instant::now() + Duration::from_millis(110));
        let mut msgs = VecDeque::new();
        msgs.extend(n1.take_outbox());

        let deliver = |msgs: &mut VecDeque<RaftMessage>, nodes: &mut [&mut RaftNode]| {
            while let Some(msg) = msgs.pop_front() {
                for node in nodes.iter_mut() {
                    if node.node_id == msg.to {
                        node.step(msg.clone());
                        msgs.extend(node.take_outbox());
                    }
                }
            }
        };

        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);
        assert_eq!(n1.role, Role::Leader);
        assert_eq!(n1.current_term, 1);

        // Failover! Node 1 is killed. Node 2 times out.
        n2.tick(Instant::now() + Duration::from_millis(300));
        msgs.extend(n2.take_outbox());

        // Deliver just between n2 and n3
        deliver(&mut msgs, &mut [&mut n2, &mut n3]);

        assert_eq!(n2.role, Role::Leader);
        assert_eq!(n2.current_term, 2);

        // Node 1 comes back up and ticks.
        n1.tick(Instant::now() + Duration::from_millis(400));
        msgs.extend(n1.take_outbox());
        
        // Let N2 heartbeat out so N1 sees N2 is leader
        n2.tick(Instant::now());
        msgs.extend(n2.take_outbox());
        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);

        assert_eq!(n1.role, Role::Follower);
        assert_eq!(n1.current_term, 2);
    }

    #[test]
    fn test_network_partition_chaos() {
        let mut n1 = RaftNode::new(RaftNodeId(1), vec![RaftNodeId(2), RaftNodeId(3)], Duration::from_millis(100));
        let mut n2 = RaftNode::new(RaftNodeId(2), vec![RaftNodeId(1), RaftNodeId(3)], Duration::from_millis(150));
        let mut n3 = RaftNode::new(RaftNodeId(3), vec![RaftNodeId(1), RaftNodeId(2)], Duration::from_millis(200));

        // Initial Leader N1
        n1.tick(Instant::now() + Duration::from_millis(110));
        let mut msgs = VecDeque::new();
        msgs.extend(n1.take_outbox());

        let deliver = |msgs: &mut VecDeque<RaftMessage>, nodes: &mut [&mut RaftNode]| {
            while let Some(msg) = msgs.pop_front() {
                for node in nodes.iter_mut() {
                    if node.node_id == msg.to {
                        node.step(msg.clone());
                        msgs.extend(node.take_outbox());
                    }
                }
            }
        };

        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);
        assert_eq!(n1.role, Role::Leader);

        // N1 is partitioned from N2 and N3
        // So N2 times out and can't reach N1. N2 and N3 elect N2.
        n2.tick(Instant::now() + Duration::from_millis(300));
        msgs.extend(n2.take_outbox());
        deliver(&mut msgs, &mut [&mut n2, &mut n3]);
        
        assert_eq!(n2.role, Role::Leader);
        assert_eq!(n2.current_term, 2);

        // N1 still thinks it's leader because no news arrived, but it can't commit.
        n1.append_local_entry(b"lost".to_vec());
        msgs.extend(n1.take_outbox());
        // partition drops msgs
        msgs.clear();

        // Partition heals. N2 appends a new entry as the true leader.
        n2.append_local_entry(b"found".to_vec());
        msgs.extend(n2.take_outbox());
        deliver(&mut msgs, &mut [&mut n1, &mut n2, &mut n3]);

        assert_eq!(n1.role, Role::Follower);
        // N1's log should have been truncated and overwritten by N2's entry
        assert_eq!(n1.log.len(), 1);
        assert_eq!(n1.log[0].payload, b"found");
    }
}
