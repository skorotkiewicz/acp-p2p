// In-memory state for a single agent node

use crate::protocol::{AcpMessage, AgentInfo};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// A question this agent asked, tracking incoming answers
#[derive(Debug)]
#[allow(dead_code)]
pub struct PendingQuestion {
    pub question_id: String,
    pub content: String,
    pub asked_at: DateTime<Utc>,
    pub answers: Vec<ReceivedAnswer>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ReceivedAnswer {
    pub from_alias: String,
    pub from_peer: String,
    pub content: String,
    pub confidence: f32,
    pub received_at: DateTime<Utc>,
}

/// Full agent state
pub struct AgentState {
    /// This agent's own info
    pub me: AgentInfo,

    /// All known peers in the mesh (peer_id -> info)
    pub peers: HashMap<String, AgentInfo>,

    /// Questions this agent is waiting answers for
    pub pending_questions: HashMap<String, PendingQuestion>,

    /// Questions received from others (question_id -> content), to avoid re-answering
    pub seen_questions: HashMap<String, String>,
}

impl AgentState {
    pub fn new(me: AgentInfo) -> Self {
        Self {
            me,
            peers: HashMap::new(),
            pending_questions: HashMap::new(),
            seen_questions: HashMap::new(),
        }
    }

    pub fn add_peer(&mut self, info: AgentInfo) {
        if info.peer_id == self.me.peer_id {
            return;
        }

        match self.peers.entry(info.peer_id.clone()) {
            Entry::Vacant(entry) => {
                println!(
                    "  [mesh] peer joined: {} ({})",
                    info.alias,
                    short_peer_id(&info.peer_id)
                );
                entry.insert(info);
            }
            Entry::Occupied(mut entry) => {
                if entry.get() != &info {
                    println!(
                        "  [mesh] peer updated: {} ({})",
                        info.alias,
                        short_peer_id(&info.peer_id)
                    );
                    entry.insert(info);
                }
            }
        }
    }

    pub fn remove_peer(&mut self, peer_id: &str) {
        if let Some(info) = self.peers.remove(peer_id) {
            println!("  [mesh] peer left: {}", info.alias);
        }
    }

    pub fn record_question(&mut self, q: &AcpMessage) {
        if let AcpMessage::Question {
            question_id,
            content,
            ..
        } = q
        {
            self.seen_questions
                .insert(question_id.clone(), content.clone());
        }
    }

    pub fn add_answer(&mut self, answer: ReceivedAnswer, question_id: &str) {
        if let Some(pq) = self.pending_questions.get_mut(question_id) {
            println!(
                "\n  ✉  Answer from {} (confidence {:.0}%): {}",
                answer.from_alias,
                answer.confidence * 100.0,
                answer.content
            );
            pq.answers.push(answer);
        }
    }

    /// Should this agent answer a given question based on capability match?
    pub fn should_answer(&self, required_caps: &[String]) -> bool {
        if required_caps.is_empty() {
            return true; // broadcast to all
        }
        required_caps
            .iter()
            .any(|cap| self.me.capabilities.contains(cap))
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn list_peers(&self) {
        if self.peers.is_empty() {
            println!("  No peers connected yet.");
        } else {
            for p in self.peers.values() {
                println!(
                    "  - {} [{}] caps: {:?}",
                    p.alias,
                    short_peer_id(&p.peer_id),
                    p.capabilities
                );
            }
        }
    }
}

fn short_peer_id(peer_id: &str) -> &str {
    let end = peer_id.len().min(12);
    &peer_id[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(peer_id: &str, alias: &str, capabilities: &[&str]) -> AgentInfo {
        AgentInfo {
            peer_id: peer_id.to_string(),
            alias: alias.to_string(),
            capabilities: capabilities.iter().map(|cap| cap.to_string()).collect(),
        }
    }

    #[test]
    fn ignores_self_peer() {
        let me = info("peer-a", "alice", &["rust"]);
        let mut state = AgentState::new(me.clone());

        state.add_peer(me);

        assert_eq!(state.peer_count(), 0);
    }

    #[test]
    fn updates_peer_without_duplicating() {
        let mut state = AgentState::new(info("peer-a", "alice", &[]));

        state.add_peer(info("peer-b", "bob", &["rust"]));
        state.add_peer(info("peer-b", "bob", &["rust", "math"]));

        assert_eq!(state.peer_count(), 1);
        assert_eq!(
            state.peers.get("peer-b").unwrap().capabilities,
            vec!["rust", "math"]
        );
    }

    #[test]
    fn answers_when_any_required_capability_matches() {
        let state = AgentState::new(info("peer-a", "alice", &["rust", "math"]));

        assert!(state.should_answer(&[]));
        assert!(state.should_answer(&["python".to_string(), "rust".to_string()]));
        assert!(!state.should_answer(&["python".to_string()]));
    }

    #[test]
    fn records_seen_questions() {
        let mut state = AgentState::new(info("peer-a", "alice", &[]));
        let msg = AcpMessage::Question {
            question_id: "q1".to_string(),
            from_peer: "peer-b".to_string(),
            from_alias: "bob".to_string(),
            content: "hello".to_string(),
            required_caps: vec![],
            timestamp: Utc::now(),
        };

        state.record_question(&msg);

        assert_eq!(state.seen_questions.get("q1").unwrap(), "hello");
    }
}
