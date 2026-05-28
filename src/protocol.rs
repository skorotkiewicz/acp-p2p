// ACP message types exchanged between agents over gossipsub

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A capability tag an agent can declare (e.g. "rust", "python", "math")
pub type Capability = String;

/// Unique agent identity (derived from libp2p PeerId, but human-readable alias too)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub peer_id: String,
    pub alias: String,
    pub capabilities: Vec<Capability>,
}

/// Direction for embedded ACP JSON-RPC payloads.
#[cfg(feature = "acp")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcpDirection {
    ClientToAgent,
    AgentToClient,
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

    /// ACP JSON-RPC payload relayed through the mesh.
    #[cfg(feature = "acp")]
    AcpJsonRpc {
        message_id: String,
        from_peer: String,
        from_alias: String,
        direction: AcpDirection,
        payload: agent_client_protocol::UntypedMessage,
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

    pub fn claimed_peer_id(&self) -> &str {
        match self {
            AcpMessage::Announce { agent, .. } => &agent.peer_id,
            AcpMessage::Question { from_peer, .. } => from_peer,
            AcpMessage::Answer { from_peer, .. } => from_peer,
            #[cfg(feature = "acp")]
            AcpMessage::AcpJsonRpc { from_peer, .. } => from_peer,
            AcpMessage::Goodbye { peer_id, .. } => peer_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> AgentInfo {
        AgentInfo {
            peer_id: "peer-a".to_string(),
            alias: "alice".to_string(),
            capabilities: vec!["rust".to_string()],
        }
    }

    #[test]
    fn serializes_and_deserializes_announce() {
        let msg = AcpMessage::Announce {
            agent: agent(),
            timestamp: Utc::now(),
        };

        let decoded = AcpMessage::deserialize(&msg.serialize()).unwrap();

        match decoded {
            AcpMessage::Announce { agent, .. } => {
                assert_eq!(agent.peer_id, "peer-a");
                assert_eq!(agent.capabilities, vec!["rust"]);
            }
            _ => panic!("expected announce"),
        }
    }

    #[test]
    fn exposes_claimed_peer_id() {
        let msg = AcpMessage::Question {
            question_id: "q1".to_string(),
            from_peer: "peer-b".to_string(),
            from_alias: "bob".to_string(),
            content: "hello".to_string(),
            required_caps: vec![],
            timestamp: Utc::now(),
        };

        assert_eq!(msg.claimed_peer_id(), "peer-b");
    }

    #[cfg(feature = "acp")]
    #[test]
    fn serializes_embedded_acp_jsonrpc() {
        let payload =
            agent_client_protocol::UntypedMessage::new("session/prompt", serde_json::json!({}))
                .unwrap();
        let msg = AcpMessage::AcpJsonRpc {
            message_id: "m1".to_string(),
            from_peer: "peer-a".to_string(),
            from_alias: "alice".to_string(),
            direction: AcpDirection::ClientToAgent,
            payload,
            timestamp: Utc::now(),
        };

        let decoded = AcpMessage::deserialize(&msg.serialize()).unwrap();

        match decoded {
            AcpMessage::AcpJsonRpc { payload, .. } => {
                assert_eq!(payload.method(), "session/prompt");
            }
            _ => panic!("expected acp jsonrpc"),
        }
    }
}
