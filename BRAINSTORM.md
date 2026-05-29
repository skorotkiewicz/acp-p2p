**sled** as the local KV store + **SIE** for embeddings/reranking = a proper semantic search agent right inside the mesh.

Imagine an agent like:

```rust
// On question received:
// 1. Embed the question via SIE
let question_vec = sie.encode("NovaSearch/stella_en_400M_v5", question).await?;

// 2. Search sled for nearest neighbours
let candidates = sled_index.search(question_vec, top_k=10)?;

// 3. Rerank with SIE
let ranked = sie.score("BAAI/bge-reranker-v2-m3", question, candidates).await?;

// 4. Return best result as Answer with ranked[0].score as confidence
```

And the confidence score in your ACP protocol maps **perfectly** to SIE's reranker score — `0.998` vs `0.012` is exactly the kind of signal the mesh needs to pick the best answer between multiple agents.

The extract/NER feature is also interesting for **capability routing** — an agent could auto-tag incoming questions with entities and route them without the user specifying `--caps` manually.

The whole stack would be:
- `libp2p` — mesh transport
- `sled` — local persistent index
- `SIE` — semantic understanding
- `acp-p2p` — the glue 🦀

That's a genuinely useful decentralized knowledge system. No cloud required. 👀

## references
- [superlinked/sie](https://github.com/superlinked/sie)
- [spacejam/sled](https://github.com/spacejam/sled)
