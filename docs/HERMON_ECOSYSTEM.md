# Hermon AI Ecosystem Architecture

## Overview

The Hermon ecosystem is a unified platform for AI-powered developer tools,
centered around the Hermon control plane at `wish.hermon.ai`.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    hermon.ai Control Plane                            │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ │
│  │   Auth   │ │    AI    │ │  Agents  │ │   Drive  │ │ Telemetry│ │
│  │ /v1/auth │ │ /v1/ai   │ │/v1/agents│ │/v1/drive │ │/v1/telem.│ │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘ │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐                            │
│  │  Orgs    │ │ Sessions │ │  Convos  │                            │
│  │ /v1/orgs │ │/v1/sess. │ │/v1/convs │                            │
│  └──────────┘ └──────────┘ └──────────┘                            │
└─────────────────────────────────────────────────────────────────────┘
         │              │              │              │
    ┌────┴────┐   ┌────┴────┐   ┌────┴────┐   ┌────┴────┐
    │  Wish   │   │Wish Code│   │Wish CLI │   │Wish Web │
    │Terminal │   │  (IDE)  │   │  (CLI)  │   │(Browser)│
    └─────────┘   └─────────┘   └─────────┘   └─────────┘
```

## Products

### Wish Terminal (this repo)
- **Role**: Terminal-based IDE and agent management environment
- **Platform**: macOS, Windows, Linux (native Rust)
- **Backend**: hermon_client crate → wish.hermon.ai
- **Key features**:
  - Terminal emulator with AI agent integration
  - Built-in SDLC agents (planner, coder, reviewer, tester, etc.)
  - Wish Drive for cloud workflow/notebook storage
  - Agent management panel
  - Conversation history

### Wish Code (wishcode)
- **Role**: Agentic coding framework and desktop IDE
- **Platform**: Electron + React + TypeScript
- **Backend**: Same hermon.ai APIs
- **Key features**:
  - Multi-model support (Claude, OpenAI, Gemini, Ollama)
  - Self-evolving skills framework
  - MCP server integration
  - Web IDE at code.mapleai.org

### Wish CLI (wish-cli)
- **Role**: Command-line agent interaction
- **Platform**: Single-binary Rust CLI
- **Backend**: hermon_client crate (shared with terminal)
- **Key features**:
  - `wish ask` — single-turn agent queries
  - `wish chat` — multi-turn conversations
  - `wish agent` — agent management
  - `wish drive` — cloud object management
  - `wish login` — authentication

### Wish Music Studio
- **Role**: AI-powered music composition
- **Platform**: Web (React + Tone.js) + Rust audio engine
- **Backend**: hermon.ai for AI model routing
- **Key features**:
  - 7 genre AI agents
  - Rust audio-engine via WASM
  - Claude API integration

### Wish Design (planned)
- **Role**: AI-powered design tool
- **Platform**: Web + native
- **Backend**: hermon.ai for AI generation
- **Key features**: TBD

## Shared Infrastructure

### hermon_client (Rust crate)
Shared by Wish Terminal and Wish CLI. Provides typed API access to all
Hermon endpoints.

**Namespaces**:
- `auth` — register, login, session management
- `ai` — model routing, SSE streaming
- `agents` — CRUD, invoke, built-in listing
- `conversations` — history, messages, streaming
- `drive` — object storage, sync
- `sessions` — device sessions
- `orgs` — organization management
- `telemetry` — event ingestion

### Built-in SDLC Agents
Pre-registered system agents available to all users:

| Slug | Role | Model |
|------|------|-------|
| wish-planner | Architecture & planning | opus |
| wish-coder | Code implementation | sonnet |
| wish-reviewer | Code review | opus |
| wish-tester | Test generation & execution | sonnet |
| wish-debugger | Bug diagnosis & fixes | opus |
| wish-deployer | CI/CD & deployment | sonnet |
| wish-documenter | Documentation | haiku |
| wish-refactorer | Code improvement | sonnet |
| wish-security | Security auditing | opus |
| wish-orchestrator | Multi-agent coordination | opus |

### URL Configuration

| Environment | API Gateway | Dashboard |
|-------------|------------|-----------|
| Production (Stable/Preview/Oss) | https://wish.hermon.ai | https://wish.hermon.ai |
| Local Development | http://localhost:8080 | http://localhost:3000 |
| Override | HERMON_API_URL env var | HERMON_DASHBOARD_URL env var |

## Data Flow

```
User Input → Agent Selection → Model Routing → SSE Stream → UI Rendering
                  │                    │
                  │                    ├─ Tool Execution
                  │                    │      │
                  │                    │      └─ Approval (if required)
                  │                    │
                  │                    └─ Sub-agent Delegation
                  │
                  └─ Conversation History → Drive Storage
```

## Deployment

### Production
- Backend: wish.hermon.ai (Kubernetes)
- CDN: Cloudflare
- Auth: Firebase + Hermon tokens
- Storage: PostgreSQL + S3-compatible

### Development
- Gateway: localhost:8080 (Rust/axum)
- Dashboard: localhost:3000 (Next.js)
- Database: PostgreSQL local
- Model proxy: Ollama or direct API keys
