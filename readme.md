# ACP-P2P

A peer-to-peer implementation of the Agent Client Protocol (ACP) for decentralized agent communication.

## Installation

```bash
# Clone the repository
git clone --depth 1 https://github.com/skorotkiewicz/acp-p2p
cd acp-p2p

# Build with ACP features enabled
cargo build --release --features acp
# or
just build-all
```

## Usage

```bash
# Run the agent
./target/release/agent --alias alice --caps rust,math

# Connect to a specific peer at startup
just run --peer /ip4/127.0.0.1/tcp/12345/p2p/<peer-id> --alias alice

# Generate ephemeral identity (don't persist to file)
just run --ephemeral --alias test

# Disable mDNS discovery
just run --no-mdns --alias isolated

# Custom listen address
just run --listen /ip4/0.0.0.0/tcp/9000
```

<details>
  <summary>examples</summary>

```bash
# Connect to a specific peer at startup
just run --peer /ip4/127.0.0.1/tcp/12345/p2p/<peer-id> --alias alice

# Generate ephemeral identity (don't persist to file)
just run --ephemeral --alias test

# Disable mDNS discovery
just run --no-mdns --alias isolated

# Custom listen address
just run --listen /ip4/0.0.0.0/tcp/9000
```

</details>


The agent is configured via command-line arguments:

- `--alias <name>` - Human-readable alias for this agent
- `--caps <list>` - Comma-separated capability tags (e.g. rust,math,python)
- `--peer <multiaddr>` - Peer multiaddr to connect to at startup
- `--ephemeral` - Generate a new in-memory identity (don't persist)
- `--no-mdns` - Disable automatic peer discovery
- `--listen <multiaddr>` - Custom listen address (can be repeated)

Once running, the agent accepts interactive commands:

- `ask <question>` - broadcast a question to all peers
- `ask --caps rust,math <question>` - ask agents with specific capabilities
- `ask @rust,math <question>` - short form for capability routing
- `acp <method> <json-params>` - broadcast ACP JSON-RPC payload (requires acp feature)
- `dial <multiaddr>` - manually connect to a peer
- `peers` - list known peers
- `quit` - exit the mesh

## Features

- Peer-to-peer networking using libp2p
- Agent Client Protocol (ACP) implementation
- Support for gossipsub messaging
- Automatic peer discovery via mDNS
- Async runtime with Tokio
- Command-line interface with clap

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT License
