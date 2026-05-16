# Wish Demos

A reference set of `.wishworld/` worlds used by integration tests,
keynote demos, and documentation. Every world here is loadable by
`wish_world_model::read_world_dir` and exists in tree form so AI agents
can read it as easily as humans can.

## Worlds

### `shanhai-fintech-harbor.wishworld/`

The North Star demo — an ancient Chinese harbor city where AI merchants
teach stablecoin, credit, trade, and risk management. Connects every
pillar of the MapleAI civilization stack:

- **Wish** — terminal + code + canvas + scene + world studio
- **Hermon** — agent runtime, world architect, model routing
- **Finalverse** — live world hosting
- **OpeniBank** — agent finance (NPC merchant transactions)
- **CreditChain** — WorldLine anchoring + provenance
- **iWallet** — signing layer

See:
- `wish-design/wish-plan-20260514/10-launch-and-go-to-market/03-north-star-demo.md`
- `crates/wish-world-model/tests/fixture_shanhai.rs` (loads this world
  end-to-end as a `cargo test` integration test).

### Layout

```
shanhai-fintech-harbor.wishworld/
├── world.json                          # top-level manifest
├── entities/                           # one .entity.json per world entity
│   ├── dragon_temple.entity.json
│   ├── merchant_liu.entity.json        # stablecoin teacher (NPC)
│   └── banker_sun.entity.json          # credit teacher (NPC)
├── scenes/
│   └── main.scene.json                 # the main 3D scene
├── agents/
│   └── world_agents/
│       └── world_architect.agent.json  # generated this world
├── governance/
│   └── policies.toml                   # risk gates, capabilities, anchoring
├── memory/
│   ├── lore.md                         # human-readable lore
│   └── design_rationale.md             # (TODO)
├── assets/                             # binary assets referenced by entities
├── scripts/
│   ├── behaviors/                      # NPC behavior scripts
│   └── quests/                         # quest DSL
├── missions/                           # Mission + VerifiableArtifact JSON
└── provenance/                         # WorldLine append-only ledger
```

### Why this format matters

`.wishworld/` is **portable, semantic, git-friendly, AI-readable**. Any
agent can clone the directory, mutate JSON, write a new entity, and the
change appears across the Wish editor, canvas, scene, and (when
deployed) Finalverse runtime — all because every visible object has a
SemanticId that the WWM enforces.

Compare to:

| Format | Portable | Semantic | Git-friendly | AI-readable | Multi-realm |
|---|---|---|---|---|---|
| `.blend` (Blender) | yes | partial | no (binary) | no | no |
| Unity / Unreal project | no | no | partial | partial | no |
| Antigravity task | no | partial | no | yes | no |
| AI Studio chat | no | no | no | yes | no |
| **`.wishworld/`** | **yes** | **yes** | **yes** | **yes** | **yes** |
