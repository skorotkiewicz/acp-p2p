# acp-p2p

a peer-to-peer implementation of the Agent Client Protocol (ACP) for decentralized agent communication.

## installation

```bash
# clone the repository
git clone --depth 1 https://github.com/skorotkiewicz/acp-p2p
cd acp-p2p

# build with ACP features enabled
cargo build --release --features acp
# or
just build-all
```

## usage

```bash
# run the agent
./target/release/agent --alias alice --caps rust,math
```

<details>
  <summary>examples</summary>

```bash
# connect to a specific peer at startup
just run --peer /ip4/127.0.0.1/tcp/12345/p2p/<peer-id> --alias alice

# generate ephemeral identity (don't persist to file)
just run --ephemeral --alias test

# disable mDNS discovery
just run --no-mdns --alias isolated

# custom listen address
just run --listen /ip4/0.0.0.0/tcp/9000
```

</details>


the agent is configured via command-line arguments:

- `--alias <name>` - human-readable alias for this agent
- `--caps <list>` - comma-separated capability tags (e.g. rust,math,python)
- `--peer <multiaddr>` - peer multiaddr to connect to at startup
- `--ephemeral` - generate a new in-memory identity (don't persist)
- `--no-mdns` - disable automatic peer discovery
- `--listen <multiaddr>` - custom listen address (can be repeated)

once running, the agent accepts interactive commands:

- `ask <question>` - broadcast a question to all peers
- `ask --caps rust,math <question>` - ask agents with specific capabilities
- `ask @rust,math <question>` - short form for capability routing
- `acp <method> <json-params>` - broadcast ACP JSON-RPC payload (requires acp feature)
- `dial <multiaddr>` - manually connect to a peer
- `peers` - list known peers
- `quit` - exit the mesh


<details>
  <summary>features</summary>

- peer-to-peer networking using libp2p
- agent client protocol (ACP) implementation
- support for gossipsub messaging
- automatic peer discovery via mDNS
- async runtime with Tokio
- command-line interface with clap

</details>


## contributing

contributions are welcome! please feel free to submit a pull request.

## license

MIT license
