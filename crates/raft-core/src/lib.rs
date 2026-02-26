use std::time::{Duration, Instant};

pub mod store;
use store::RaftStateStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RaftNodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub index: u64,
    pub term: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug)]
pub struct RaftBootstrap {
    pub node_id: RaftNodeId,
    pub current_term: u64,
    pub voted_for: Option<RaftNodeId>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub role: Role,
    pub election_timeout: Duration,
    pub election_deadline: Instant,
    pub log: Vec<LogEntry>,
}

impl RaftBootstrap {
    pub fn new(node_id: RaftNodeId, election_timeout: Duration) -> Self {
        let now = Instant::now();
        Self {
            node_id,
            current_term: 0,
            voted_for: None,
            commit_index: 0,
            last_applied: 0,
            role: Role::Follower,
            election_timeout,
            election_deadline: now + election_timeout,
            log: Vec::new(),
        }
    }

    pub fn reset_election_deadline(&mut self) {
        self.election_deadline = Instant::now() + self.election_timeout;
    }

    pub fn become_candidate(&mut self) {
        self.current_term += 1;
        self.role = Role::Candidate;
        self.voted_for = Some(self.node_id);
        self.reset_election_deadline();
    }

    pub fn become_leader(&mut self) {
        self.role = Role::Leader;
    }

    pub fn append_local_entry(&mut self, payload: Vec<u8>) -> u64 {
        let index = self.log.last().map(|e| e.index + 1).unwrap_or(1);
        self.log.push(LogEntry {
            index,
            term: self.current_term,
            payload,
        });
        index
    }

    pub fn append_with_store<S: RaftStateStore>(&mut self, store: &mut S, payload: Vec<u8>) -> u64 {
        let index = store.entries().last().map(|e| e.index + 1).unwrap_or(1);
        let entry = LogEntry {
            index,
            term: self.current_term,
            payload,
        };
        store.append_entry(entry.clone());
        self.log.push(entry);
        index
    }

    pub fn persist_term_vote<S: RaftStateStore>(&self, store: &mut S) {
        store.set_current_term(self.current_term);
        store.set_voted_for(self.voted_for);
    }

    pub fn advance_commit(&mut self, index: u64) {
        if index > self.commit_index {
            self.commit_index = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{InMemoryStateStore, RaftStateStore};

    #[test]
    fn bootstrap_defaults() {
        let r = RaftBootstrap::new(RaftNodeId(1), Duration::from_millis(300));
        assert_eq!(r.current_term, 0);
        assert_eq!(r.role, Role::Follower);
        assert_eq!(r.commit_index, 0);
    }

    #[test]
    fn candidate_transition_votes_for_self() {
        let mut r = RaftBootstrap::new(RaftNodeId(7), Duration::from_millis(300));
        r.become_candidate();
        assert_eq!(r.current_term, 1);
        assert_eq!(r.role, Role::Candidate);
        assert_eq!(r.voted_for, Some(RaftNodeId(7)));
    }

    #[test]
    fn append_entry_increments_index() {
        let mut r = RaftBootstrap::new(RaftNodeId(1), Duration::from_millis(300));
        r.become_candidate();
        let i1 = r.append_local_entry(vec![1]);
        let i2 = r.append_local_entry(vec![2]);
        assert_eq!(i1, 1);
        assert_eq!(i2, 2);
    }

    #[test]
    fn persist_and_append_with_store() {
        let mut r = RaftBootstrap::new(RaftNodeId(9), Duration::from_millis(300));
        let mut s = InMemoryStateStore::default();

        r.become_candidate();
        r.persist_term_vote(&mut s);
        let idx = r.append_with_store(&mut s, vec![1, 2, 3]);

        assert_eq!(idx, 1);
        assert_eq!(s.current_term(), r.current_term);
        assert_eq!(s.voted_for(), Some(RaftNodeId(9)));
        assert_eq!(s.entries().len(), 1);
    }
}
