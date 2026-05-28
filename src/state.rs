// In-memory state for a single agent node

use std::collections::HashMap;
use crate::protocol::{AgentInfo, AcpMessage};
use chrono::{DateTime, Utc};

/// A question this agent asked, tracking incoming answers
#[derive(Debug)]
pub struct PendingQuestion {
    pub question_id: String,
    pub content: String,
    pub asked_at: DateTime<Utc>,
    pub answers: Vec<ReceivedAnswer>,
}

#[derive(Debug, Clone)]
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
        if info.peer_id != self.me.peer_id {
            println!("  [mesh] peer joined: {} ({})", info.alias, &info.peer_id[..12]);
            self.peers.insert(info.peer_id.clone(), info);
        }
    }

    pub fn remove_peer(&mut self, peer_id: &str) {
        if let Some(info) = self.peers.remove(peer_id) {
            println!("  [mesh] peer left: {}", info.alias);
        }
    }

    pub fn record_question(&mut self, q: &AcpMessage) {
        if let AcpMessage::Question { question_id, content, .. } = q {
            self.seen_questions.insert(question_id.clone(), content.clone());
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
        required_caps.iter().any(|cap| self.me.capabilities.contains(cap))
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn list_peers(&self) {
        if self.peers.is_empty() {
            println!("  No peers connected yet.");
        } else {
            for (_, p) in &self.peers {
                println!("  - {} [{}] caps: {:?}", p.alias, &p.peer_id[..12], p.capabilities);
            }
        }
    }
}
