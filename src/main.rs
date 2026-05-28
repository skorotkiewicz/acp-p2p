// ACP P2P Agent — libp2p-powered mesh where agents ask and answer each other.
//
// Usage:
//   cargo run -- --alias alice --caps rust,math
//   cargo run -- --alias bob   --caps python,networking

mod answerer;
mod protocol;
mod state;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use futures::StreamExt;
use libp2p::{
    gossipsub, identify, mdns,
    swarm::{NetworkBehaviour, SwarmEvent},
};
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use answerer::Answerer;
use protocol::{AcpMessage, AgentInfo};
use state::{AgentState, PendingQuestion, ReceivedAnswer};

// -- CLI --─

#[derive(Parser, Debug)]
#[command(name = "acp-agent", about = "ACP P2P Agent Node")]
struct Cli {
    /// Human-readable alias for this agent
    #[arg(long, default_value = "agent")]
    alias: String,

    /// Comma-separated capability tags (e.g. rust,math,python)
    #[arg(long, default_value = "")]
    caps: String,
}

// -- Combined NetworkBehaviour --

#[derive(NetworkBehaviour)]
struct AgentBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
}

// -- Topic --

const ACP_TOPIC: &str = "acp-mesh-v1";

// -- Main --

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logging: set RUST_LOG=info or RUST_LOG=debug
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let capabilities: Vec<String> = if cli.caps.is_empty() {
        vec![]
    } else {
        cli.caps.split(',').map(|s| s.trim().to_string()).collect()
    };

    // -- Build the libp2p swarm --
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            libp2p::tcp::Config::default(),
            libp2p::noise::Config::new,
            libp2p::yamux::Config::default,
        )?
        .with_behaviour(|key| {
            // Gossipsub — content-addressed pub/sub
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

            // mDNS — automatic local peer discovery (no bootstrap needed)
            let mdns =
                mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())
                    .expect("mdns behaviour");

            // Identify — peers exchange protocol/version info on connect
            let identify = identify::Behaviour::new(identify::Config::new(
                "/acp/1.0.0".to_string(),
                key.public(),
            ));

            Ok(AgentBehaviour {
                gossipsub,
                mdns,
                identify,
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
        .build();

    // Subscribe to the ACP topic
    let topic = gossipsub::IdentTopic::new(ACP_TOPIC);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    // Listen on all interfaces, random port
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    // -- Agent state --
    let peer_id = swarm.local_peer_id().to_string();
    let me = AgentInfo {
        peer_id: peer_id.clone(),
        alias: cli.alias.clone(),
        capabilities: capabilities.clone(),
    };

    let mut state = AgentState::new(me.clone());
    let answerer = Answerer::new(cli.alias.clone(), capabilities.clone());

    println!("ACP P2P Agent Mesh");
    println!("  alias      : {}", cli.alias);
    println!("  peer_id    : {}", &peer_id[..20]);
    println!("  caps       : {:?}", capabilities);
    println!("\n  Commands:");
    println!("    ask <question>   - broadcast a question to all agents");
    println!("    peers            - list connected peers");
    println!("    quit             - leave the mesh\n");

    // -- Stdin reader --
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    // -- Announce ourselves after a short delay --
    // Give the swarm time to start listening before announcing
    let announce_msg = AcpMessage::Announce {
        agent: me.clone(),
        timestamp: Utc::now(),
    };

    // -- Main event loop --
    let mut announced = false;

    loop {
        tokio::select! {
            // -- Swarm events --
            event = swarm.select_next_some() => match event {

                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("  [net] listening on {address}");

                    // Announce once we have a listening address
                    if !announced {
                        announced = true;
                        let data = announce_msg.serialize();
                        if let Err(e) = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data) {
                            // Might fail if no peers yet — that's fine, they'll see us via mDNS
                            tracing::debug!("announce publish: {e}");
                        }
                    }
                }

                // mDNS found new peers on the local network
                SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _addr) in list {
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                }

                // mDNS peers expired
                SwarmEvent::Behaviour(AgentBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _addr) in list {
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                }

                // Gossipsub message received
                SwarmEvent::Behaviour(AgentBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. }
                )) => {
                    match AcpMessage::deserialize(&message.data) {
                        Ok(msg) => handle_message(msg, &mut state, &answerer, &mut swarm, &topic),
                        Err(e) => tracing::warn!("bad message: {e}"),
                    }
                }

                SwarmEvent::Behaviour(AgentBehaviourEvent::Gossipsub(
                    gossipsub::Event::Subscribed { peer_id, topic: t }
                )) => {
                    tracing::debug!("{peer_id} subscribed to {t}");
                    // Re-announce when new subscriber joins
                    let data = announce_msg.serialize();
                    let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), data);
                }

                _ => {}
            },

            // -- Stdin commands --
            Ok(Some(line)) = lines.next_line() => {
                let line = line.trim().to_string();
                if line.is_empty() { continue; }

                if line == "peers" {
                    println!("  [{} peer(s) known]", state.peer_count());
                    state.list_peers();

                } else if line == "quit" || line == "exit" {
                    let bye = AcpMessage::Goodbye {
                        peer_id: peer_id.clone(),
                        alias: cli.alias.clone(),
                    };
                    let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), bye.serialize());
                    println!("  Goodbye!");
                    break;

                } else if let Some(question) = line.strip_prefix("ask ") {
                    let question_id = Uuid::new_v4().to_string();
                    let msg = AcpMessage::Question {
                        question_id: question_id.clone(),
                        from_peer: peer_id.clone(),
                        from_alias: cli.alias.clone(),
                        content: question.to_string(),
                        required_caps: vec![], // ask everyone
                        timestamp: Utc::now(),
                    };

                    state.pending_questions.insert(question_id.clone(), PendingQuestion {
                        question_id: question_id.clone(),
                        content: question.to_string(),
                        asked_at: Utc::now(),
                        answers: vec![],
                    });

                    match swarm.behaviour_mut().gossipsub.publish(topic.clone(), msg.serialize()) {
                        Ok(_) => println!("  ❓ Question broadcast [{}]: {}", &question_id[..8], question),
                        Err(e) => println!("  [err] publish failed: {e} (are there peers?)"),
                    }

                } else {
                    println!("  Unknown command. Try: ask <question> | peers | quit");
                }
            }
        }
    }

    Ok(())
}

// -- Message handler --

fn handle_message(
    msg: AcpMessage,
    state: &mut AgentState,
    answerer: &Answerer,
    swarm: &mut libp2p::Swarm<AgentBehaviour>,
    topic: &gossipsub::IdentTopic,
) {
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
            // Don't answer our own questions
            if from_peer == &state.me.peer_id {
                return;
            }
            // Don't answer if we've seen this question already
            if state.seen_questions.contains_key(question_id) {
                return;
            }
            state.record_question(&msg);

            println!(
                "\n  ❓ Question from {} [{}]: {}",
                from_alias,
                &question_id[..8],
                content
            );

            // Check capability match
            if !state.should_answer(required_caps) {
                println!("  (skipping — not in our capabilities)");
                return;
            }

            // Try to generate an answer
            if let Some((answer_text, confidence)) = answerer.try_answer(content) {
                println!("  → Answering with confidence {:.0}%", confidence * 100.0);

                let answer = AcpMessage::Answer {
                    question_id: question_id.clone(),
                    from_peer: state.me.peer_id.clone(),
                    from_alias: state.me.alias.clone(),
                    content: answer_text,
                    confidence,
                    timestamp: Utc::now(),
                };

                match swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(topic.clone(), answer.serialize())
                {
                    Ok(_) => {}
                    Err(e) => tracing::warn!("answer publish failed: {e}"),
                }
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
            // Ignore answers from ourselves
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
    }
}
