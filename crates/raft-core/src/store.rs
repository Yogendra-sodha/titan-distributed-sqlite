use crate::{LogEntry, RaftNodeId};

pub trait RaftStateStore {
    fn current_term(&self) -> u64;
    fn set_current_term(&mut self, term: u64);

    fn voted_for(&self) -> Option<RaftNodeId>;
    fn set_voted_for(&mut self, node: Option<RaftNodeId>);

    fn append_entry(&mut self, entry: LogEntry);
    fn entries(&self) -> &[LogEntry];
}

#[derive(Debug, Default)]
pub struct InMemoryStateStore {
    term: u64,
    voted_for: Option<RaftNodeId>,
    log: Vec<LogEntry>,
}

impl RaftStateStore for InMemoryStateStore {
    fn current_term(&self) -> u64 {
        self.term
    }

    fn set_current_term(&mut self, term: u64) {
        self.term = term;
    }

    fn voted_for(&self) -> Option<RaftNodeId> {
        self.voted_for
    }

    fn set_voted_for(&mut self, node: Option<RaftNodeId>) {
        self.voted_for = node;
    }

    fn append_entry(&mut self, entry: LogEntry) {
        self.log.push(entry);
    }

    fn entries(&self) -> &[LogEntry] {
        &self.log
    }
}
