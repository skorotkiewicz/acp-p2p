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
    pub me: AgentInfo,
    pub peers: HashMap<String, AgentInfo>,
    pub pending_questions: HashMap<String, PendingQuestion>,
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
            self.peers.insert(info.peer_id.clone(), info);
        }
    }

    pub fn remove_peer(&mut self, peer_id: &str) {
        self.peers.remove(peer_id);
    }

    pub fn record_question(&mut self, q: &AcpMessage) {
        if let AcpMessage::Question { question_id, content, .. } = q {
            self.seen_questions.insert(question_id.clone(), content.clone());
        }
    }

    pub fn add_answer(&mut self, answer: ReceivedAnswer, question_id: &str) {
        if let Some(pq) = self.pending_questions.get_mut(question_id) {
            pq.answers.push(answer);
        }
    }
}
