// ACP message types exchanged between agents over gossipsub

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A capability tag an agent can declare (e.g. "rust", "python", "math")
pub type Capability = String;

/// Unique agent identity (derived from libp2p PeerId, but human-readable alias too)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub peer_id: String,
    pub alias: String,
    pub capabilities: Vec<Capability>,
}

/// All message types flowing over the P2P network
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpMessage {
    /// Agent announces itself to the mesh
    Announce {
        agent: AgentInfo,
        timestamp: DateTime<Utc>,
    },

    /// Agent asks a question to the whole mesh
    Question {
        question_id: String,
        from_peer: String,
        from_alias: String,
        content: String,
        /// Optional: only agents with these capabilities should answer
        required_caps: Vec<Capability>,
        timestamp: DateTime<Utc>,
    },

    /// Agent answers a previously asked question
    Answer {
        question_id: String, // links back to Question
        from_peer: String,
        from_alias: String,
        content: String,
        confidence: f32, // 0.0 - 1.0, agent's self-reported confidence
        timestamp: DateTime<Utc>,
    },

    /// Agent gracefully leaves the mesh
    Goodbye { peer_id: String, alias: String },
}

impl AcpMessage {
    pub fn serialize(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serialize AcpMessage")
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}
