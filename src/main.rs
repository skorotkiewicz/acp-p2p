// ACP P2P Agent -- libp2p-powered mesh where agents ask and answer each other.
//
// Usage:
//   cargo run -- --alias alice --caps rust,math
//   cargo run -- --alias bob --caps python,networking --peer /ip4/127.0.0.1/tcp/12345/p2p/<peer>

mod answerer;
mod protocol;
mod state;

use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io as std_io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use futures::StreamExt;
use libp2p::{
    Multiaddr, gossipsub, identify, identity, mdns,
    multiaddr::Protocol,
    swarm::{NetworkBehaviour, SwarmEvent, behaviour::toggle::Toggle},
};
use tokio::io::{self as tokio_io, AsyncBufReadExt, BufReader};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use answerer::Answerer;
#[cfg(feature = "acp")]
use protocol::AcpDirection;
use protocol::{AcpMessage, AgentInfo};
use state::{AgentState, PendingQuestion, ReceivedAnswer};

#[derive(Parser, Debug)]
#[command(name = "acp-agent", about = "ACP P2P Agent Node")]
struct Cli {
    /// Human-readable alias for this agent
    #[arg(long, default_value = "agent")]
    alias: String,

    /// Comma-separated capability tags (e.g. rust,math,python)
    #[arg(long, default_value = "")]
    caps: String,

    /// Persistent libp2p identity key path
    #[arg(long, default_value = ".acp-p2p.identity")]
    identity: PathBuf,

    /// Generate a new in-memory identity for this run
    #[arg(long)]
    ephemeral: bool,

    /// Disable mDNS discovery and rely on --peer / dial commands
    #[arg(long)]
    no_mdns: bool,

    /// Listen multiaddr. Can be passed more than once.
    #[arg(long = "listen", default_value = "/ip4/0.0.0.0/tcp/0")]
    listen: Vec<String>,

    /// Peer multiaddr to dial at startup. Can be passed more than once.
    #[arg(long = "peer")]
    peers: Vec<String>,
}

#[derive(NetworkBehaviour)]
struct AgentBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: Toggle<mdns::tokio::Behaviour>,
    identify: identify::Behaviour,
}

const ACP_TOPIC: &str = "acp-mesh-v1";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let capabilities = parse_capabilities(&cli.caps);
    let (identity_key, identity_label) = load_identity(&cli)?;
    let mdns_enabled = !cli.no_mdns;

    let mut swarm = libp2p::SwarmBuilder::with_existing_identity(identity_key)
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_behaviour(move |key| {
            let msg_id_fn = |msg: &gossipsub::Message| {
                let mut s = DefaultHasher::new();
                msg.data.hash(&mut s);
                gossipsub::MessageId::from(s.finish().to_string())
            };

            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(3))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .message_id_fn(msg_id_fn)
                .build()
                .expect("valid gossipsub config");

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )
            .expect("gossipsub behaviour");

            let mdns = if mdns_enabled {
                match mdns::tokio::Behaviour::new(
                    mdns::Config::default(),
                    key.public().to_peer_id(),
                ) {
                    Ok(mdns) => Some(mdns),
                    Err(error) => {
                        eprintln!("  [warn] mDNS disabled: {error}");
                        None
                    }
                }
            } else {
                None
            };

            let identify = identify::Behaviour::new(identify::Config::new(
                "/acp/1.0.0".to_string(),
                key.public(),
            ));

            Ok(AgentBehaviour {
                gossipsub,
                mdns: Toggle::from(mdns),
                identify,
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    let topic = gossipsub::IdentTopic::new(ACP_TOPIC);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    for addr in parse_multiaddrs(&cli.listen, "listen")? {
        swarm.listen_on(addr)?;
    }

    let peer_id = swarm.local_peer_id().to_string();
    let me = AgentInfo {
        peer_id: peer_id.clone(),
        alias: cli.alias.clone(),
        capabilities: capabilities.clone(),
    };

    let mut state = AgentState::new(me.clone());
    let answerer = Answerer::new(cli.alias.clone(), capabilities.clone());
    let announce_msg = AcpMessage::Announce {
        agent: me,
        timestamp: Utc::now(),
    };

    println!("ACP P2P Agent Mesh");
    println!("  alias      : {}", cli.alias);
    println!("  peer_id    : {}", short_id(&peer_id));
    println!("  identity   : {identity_label}");
    println!(
        "  discovery  : {}",
        if mdns_enabled {
            "mDNS + manual peers"
        } else {
            "manual peers"
        }
    );
    println!("  caps       : {:?}", capabilities);
    println!("\n  Commands:");
    println!("    ask <question>                  - broadcast a question");
    println!("    ask --caps rust,math <question> - ask agents with matching caps");
    println!("    ask @rust,math <question>       - short form for capability routing");
    #[cfg(feature = "acp")]
    println!("    acp <method> <json-params>      - broadcast ACP JSON-RPC payload");
    #[cfg(not(feature = "acp"))]
    println!("    acp <method> <json-params>      - available with --features acp");
    println!("    dial <multiaddr>                - dial a peer");
    println!("    peers                           - list known peers");
    println!("    quit                            - leave the mesh\n");

    for addr in parse_multiaddrs(&cli.peers, "peer")? {
        dial_address(&mut swarm, addr);
    }

    let stdin = BufReader::new(tokio_io::stdin());
    let mut lines = stdin.lines();
    let mut announced = false;

    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("  [net] listening on {address}");

                    if !announced {
                        announced = true;
                        publish_or_log(&mut swarm, &topic, &announce_msg, "announce");
                    }
                }

                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    publish_or_log(&mut swarm, &topic, &announce_msg, "announce");
                }

                SwarmEvent::ConnectionClosed { peer_id, num_established, .. } => {
                    if num_established == 0 {
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                }

                SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _addr) in list {
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                }

                SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _addr) in list {
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                }

                SwarmEvent::Behaviour(AgentBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. }
                )) => {
                    let signed_source = message.source.as_ref().map(ToString::to_string);
                    match AcpMessage::deserialize(&message.data) {
                        Ok(msg) => handle_message(
                            msg,
                            signed_source.as_deref(),
                            &mut state,
                            &answerer,
                            &mut swarm,
                            &topic,
                        ),
                        Err(e) => tracing::warn!("bad message: {e}"),
                    }
                }

                SwarmEvent::Behaviour(AgentBehaviourEvent::Gossipsub(
                    gossipsub::Event::Subscribed { peer_id, topic: subscribed_topic }
                )) => {
                    tracing::debug!("{peer_id} subscribed to {subscribed_topic}");
                    publish_or_log(&mut swarm, &topic, &announce_msg, "announce");
                }

                _ => {}
            },

            Ok(Some(line)) = lines.next_line() => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                if line == "peers" {
                    println!("  [{} peer(s) known]", state.peer_count());
                    state.list_peers();
                } else if line == "quit" || line == "exit" {
                    let bye = AcpMessage::Goodbye {
                        peer_id: peer_id.clone(),
                        alias: cli.alias.clone(),
                    };
                    publish_or_log(&mut swarm, &topic, &bye, "goodbye");
                    println!("  Goodbye!");
                    break;
                } else if let Some(addr) = line.strip_prefix("dial ") {
                    match parse_multiaddr(addr.trim(), "dial") {
                        Ok(addr) => dial_address(&mut swarm, addr),
                        Err(e) => println!("  [err] {e}"),
                    }
                } else if let Some(rest) = line.strip_prefix("ask ") {
                    match parse_ask_command(rest) {
                        Ok((required_caps, question)) => {
                            let question_id = Uuid::new_v4().to_string();
                            let msg = AcpMessage::Question {
                                question_id: question_id.clone(),
                                from_peer: peer_id.clone(),
                                from_alias: cli.alias.clone(),
                                content: question.clone(),
                                required_caps: required_caps.clone(),
                                timestamp: Utc::now(),
                            };

                            state.pending_questions.insert(question_id.clone(), PendingQuestion {
                                question_id: question_id.clone(),
                                content: question.clone(),
                                asked_at: Utc::now(),
                                answers: vec![],
                            });

                            match swarm.behaviour_mut().gossipsub.publish(topic.clone(), msg.serialize()) {
                                Ok(_) if required_caps.is_empty() => {
                                    println!("  Question broadcast [{}]: {}", short_id(&question_id), question);
                                }
                                Ok(_) => {
                                    println!(
                                        "  Question routed [{}] caps {:?}: {}",
                                        short_id(&question_id),
                                        required_caps,
                                        question
                                    );
                                }
                                Err(e) => println!("  [err] publish failed: {e} (are there peers?)"),
                            }
                        }
                        Err(e) => println!("  [err] {e}"),
                    }
                } else if let Some(rest) = line.strip_prefix("acp ") {
                    #[cfg(feature = "acp")]
                    {
                        match parse_acp_command(rest) {
                            Ok(payload) => {
                                let message_id = Uuid::new_v4().to_string();
                                let msg = AcpMessage::AcpJsonRpc {
                                    message_id: message_id.clone(),
                                    from_peer: peer_id.clone(),
                                    from_alias: cli.alias.clone(),
                                    direction: AcpDirection::ClientToAgent,
                                    payload,
                                    timestamp: Utc::now(),
                                };

                                match swarm.behaviour_mut().gossipsub.publish(topic.clone(), msg.serialize()) {
                                    Ok(_) => println!("  ACP payload broadcast [{}]", short_id(&message_id)),
                                    Err(e) => println!("  [err] publish failed: {e} (are there peers?)"),
                                }
                            }
                            Err(e) => println!("  [err] {e}"),
                        }
                    }

                    #[cfg(not(feature = "acp"))]
                    {
                        let _ = rest;
                        println!("  ACP relay disabled. Rebuild with: cargo run --features acp -- ...");
                    }
                } else {
                    println!("  Unknown command. Try: ask <question> | dial <multiaddr> | peers | quit");
                }
            }
        }
    }

    Ok(())
}

fn load_identity(cli: &Cli) -> Result<(identity::Keypair, String), Box<dyn Error>> {
    if cli.ephemeral {
        return Ok((
            identity::Keypair::generate_ed25519(),
            "ephemeral".to_string(),
        ));
    }

    let key = load_or_create_identity(&cli.identity)?;
    Ok((key, cli.identity.display().to_string()))
}

fn load_or_create_identity(path: &Path) -> Result<identity::Keypair, Box<dyn Error>> {
    if path.exists() {
        let encoded = fs::read_to_string(path)?;
        let bytes = hex_decode(encoded.trim())?;
        let key = identity::Keypair::from_protobuf_encoding(&bytes)?;
        return Ok(key);
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let key = identity::Keypair::generate_ed25519();
    let bytes = key.to_protobuf_encoding()?;
    fs::write(path, hex_encode(&bytes))?;
    set_private_permissions(path)?;
    Ok(key)
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), Box<dyn Error>> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), Box<dyn Error>> {
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_decode(input: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !input.len().is_multiple_of(2) {
        return Err(std_io::Error::new(
            std_io::ErrorKind::InvalidData,
            "identity file has an odd number of hex digits",
        )
        .into());
    }

    let mut bytes = Vec::with_capacity(input.len() / 2);
    let input = input.as_bytes();
    for pair in input.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_value(byte: u8) -> Result<u8, Box<dyn Error>> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(std_io::Error::new(
            std_io::ErrorKind::InvalidData,
            format!("invalid hex digit `{}`", byte as char),
        )
        .into()),
    }
}

fn parse_capabilities(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|cap| cap.trim().to_lowercase())
        .filter(|cap| !cap.is_empty())
        .collect()
}

fn parse_multiaddrs(raw: &[String], label: &str) -> Result<Vec<Multiaddr>, Box<dyn Error>> {
    raw.iter()
        .map(|addr| parse_multiaddr(addr, label))
        .collect::<Result<Vec<_>, _>>()
}

fn parse_multiaddr(raw: &str, label: &str) -> Result<Multiaddr, Box<dyn Error>> {
    raw.parse::<Multiaddr>().map_err(|error| {
        std_io::Error::new(
            std_io::ErrorKind::InvalidInput,
            format!("invalid {label} multiaddr `{raw}`: {error}"),
        )
        .into()
    })
}

fn parse_ask_command(raw: &str) -> Result<(Vec<String>, String), Box<dyn Error>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(invalid_input("missing question").into());
    }

    if let Some(rest) = raw.strip_prefix("--caps ") {
        let (caps, question) = rest
            .trim()
            .split_once(' ')
            .ok_or_else(|| invalid_input("missing question after --caps"))?;
        let caps = parse_capabilities(caps);
        if caps.is_empty() {
            return Err(invalid_input("missing capability after --caps").into());
        }
        let question = question.trim();
        if question.is_empty() {
            return Err(invalid_input("missing question after --caps").into());
        }
        return Ok((caps, question.to_string()));
    }

    if let Some(rest) = raw.strip_prefix('@') {
        let (caps, question) = rest
            .trim()
            .split_once(' ')
            .ok_or_else(|| invalid_input("missing question after capability filter"))?;
        let caps = parse_capabilities(caps);
        if caps.is_empty() {
            return Err(invalid_input("missing capability after @").into());
        }
        let question = question.trim();
        if question.is_empty() {
            return Err(invalid_input("missing question after capability filter").into());
        }
        return Ok((caps, question.to_string()));
    }

    Ok((vec![], raw.to_string()))
}

#[cfg(feature = "acp")]
fn parse_acp_command(raw: &str) -> Result<agent_client_protocol::UntypedMessage, Box<dyn Error>> {
    let raw = raw.trim();
    let (method, params) = raw
        .split_once(' ')
        .map_or((raw, "{}"), |(method, params)| (method, params.trim()));
    if method.is_empty() {
        return Err(invalid_input("missing ACP method").into());
    }

    let params = if params.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(params)?
    };
    Ok(agent_client_protocol::UntypedMessage::new(method, params)?)
}

fn invalid_input(message: &str) -> std_io::Error {
    std_io::Error::new(std_io::ErrorKind::InvalidInput, message)
}

fn dial_address(swarm: &mut libp2p::Swarm<AgentBehaviour>, addr: Multiaddr) {
    if let Some(peer_id) = peer_id_from_addr(&addr) {
        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
    }

    match swarm.dial(addr.clone()) {
        Ok(_) => println!("  [net] dialing {addr}"),
        Err(e) => println!("  [err] dial failed for {addr}: {e}"),
    }
}

fn peer_id_from_addr(addr: &Multiaddr) -> Option<libp2p::PeerId> {
    addr.iter().find_map(|protocol| match protocol {
        Protocol::P2p(peer_id) => Some(peer_id),
        _ => None,
    })
}

fn publish_or_log(
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    topic: &gossipsub::IdentTopic,
    msg: &AcpMessage,
    label: &str,
) {
    if let Err(e) = swarm
        .behaviour_mut()
        .gossipsub
        .publish(topic.clone(), msg.serialize())
    {
        tracing::debug!("{label} publish failed: {e}");
    }
}

fn handle_message(
    msg: AcpMessage,
    signed_source: Option<&str>,
    state: &mut AgentState,
    answerer: &Answerer,
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    topic: &gossipsub::IdentTopic,
) {
    if !message_source_matches(&msg, signed_source) {
        return;
    }

    match msg {
        AcpMessage::Announce { agent, .. } => {
            state.add_peer(agent);
        }

        AcpMessage::Goodbye { peer_id, .. } => {
            state.remove_peer(&peer_id);
        }

        AcpMessage::Question {
            ref question_id,
            ref from_peer,
            ref from_alias,
            ref content,
            ref required_caps,
            ..
        } => {
            if from_peer == &state.me.peer_id {
                return;
            }
            if state.seen_questions.contains_key(question_id) {
                return;
            }
            state.record_question(&msg);

            println!(
                "\n  Question from {} [{}]: {}",
                from_alias,
                short_id(question_id),
                content
            );

            if !state.should_answer(required_caps) {
                println!("  (skipping: not in our capabilities)");
                return;
            }

            if let Some((answer_text, confidence)) = answerer.try_answer(content) {
                println!("  Answering with confidence {:.0}%", confidence * 100.0);

                let answer = AcpMessage::Answer {
                    question_id: question_id.clone(),
                    from_peer: state.me.peer_id.clone(),
                    from_alias: state.me.alias.clone(),
                    content: answer_text,
                    confidence,
                    timestamp: Utc::now(),
                };

                publish_or_log(swarm, topic, &answer, "answer");
            } else {
                println!("  (no answer for this question in our knowledge base)");
            }
        }

        AcpMessage::Answer {
            ref question_id,
            ref from_peer,
            ref from_alias,
            ref content,
            confidence,
            ..
        } => {
            if from_peer == &state.me.peer_id {
                return;
            }

            state.add_answer(
                ReceivedAnswer {
                    from_alias: from_alias.clone(),
                    from_peer: from_peer.clone(),
                    content: content.clone(),
                    confidence,
                    received_at: Utc::now(),
                },
                question_id,
            );
        }

        #[cfg(feature = "acp")]
        AcpMessage::AcpJsonRpc {
            ref message_id,
            ref from_peer,
            ref from_alias,
            ref direction,
            ref payload,
            ..
        } => {
            if from_peer == &state.me.peer_id {
                return;
            }

            println!(
                "\n  ACP {} from {} [{}]: {} {}",
                acp_direction_label(direction),
                from_alias,
                short_id(message_id),
                payload.method(),
                payload.params()
            );
        }
    }
}

fn message_source_matches(msg: &AcpMessage, signed_source: Option<&str>) -> bool {
    let claimed = msg.claimed_peer_id();
    match signed_source {
        Some(source) if source == claimed => true,
        Some(source) => {
            tracing::warn!(
                "rejecting message with mismatched signed source: claimed={claimed} signed={source}"
            );
            false
        }
        None => {
            tracing::warn!("rejecting unsigned gossipsub message: claimed={claimed}");
            false
        }
    }
}

#[cfg(feature = "acp")]
fn acp_direction_label(direction: &AcpDirection) -> &'static str {
    match direction {
        AcpDirection::ClientToAgent => "client->agent",
        AcpDirection::AgentToClient => "agent->client",
    }
}

fn short_id(id: &str) -> &str {
    let end = id.len().min(12);
    &id[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_capabilities_lowercase_and_without_empty_values() {
        assert_eq!(
            parse_capabilities(" Rust, math,,PYTHON "),
            vec!["rust", "math", "python"]
        );
    }

    #[test]
    fn parses_broadcast_question() {
        let (caps, question) = parse_ask_command("how are you?").unwrap();

        assert!(caps.is_empty());
        assert_eq!(question, "how are you?");
    }

    #[test]
    fn parses_long_capability_question() {
        let (caps, question) =
            parse_ask_command("--caps rust,math how to write fibonacci?").unwrap();

        assert_eq!(caps, vec!["rust", "math"]);
        assert_eq!(question, "how to write fibonacci?");
    }

    #[test]
    fn parses_short_capability_question() {
        let (caps, question) = parse_ask_command("@rust,math how to write fibonacci?").unwrap();

        assert_eq!(caps, vec!["rust", "math"]);
        assert_eq!(question, "how to write fibonacci?");
    }

    #[test]
    fn decodes_hex_identity_bytes() {
        assert_eq!(hex_decode("00ff10").unwrap(), vec![0x00, 0xff, 0x10]);
        assert_eq!(hex_encode(&[0x00, 0xff, 0x10]), "00ff10");
    }

    #[test]
    fn rejects_mismatched_signed_source() {
        let msg = AcpMessage::Goodbye {
            peer_id: "peer-a".to_string(),
            alias: "alice".to_string(),
        };

        assert!(message_source_matches(&msg, Some("peer-a")));
        assert!(!message_source_matches(&msg, Some("peer-b")));
        assert!(!message_source_matches(&msg, None));
    }

    #[cfg(feature = "acp")]
    #[test]
    fn parses_acp_payload() {
        let payload =
            parse_acp_command("session/prompt {\"prompt\":[{\"type\":\"text\"}]}").unwrap();

        assert_eq!(payload.method(), "session/prompt");
        assert!(payload.params().is_object());
    }
}
