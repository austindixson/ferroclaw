# Ferroclaw

A security-first, single-binary AI agent framework written in Rust.

**5.4 MB binary. 4 LLM providers. 84 skills across 16 categories. 7 messaging channels. Native MCP + DietMCP. 584 tests. Zero runtime dependencies.**

```
ferroclaw (single binary)
├── Agent Loop       ReAct cycle with streaming, backpressure, budget limits
├── LLM Providers    Anthropic, OpenAI, Zai GLM, OpenRouter (trait-based)
├── MCP Client       Official MCP protocol + DietMCP compression (70-93% token reduction)
├── Skills           84 bundled skills across 16 categories + custom TOML + AgentSkills.io
├── Tool Registry    Built-in tools + skill tools + MCP tools, capability-gated
├── Channels         Telegram, Discord, Slack, WhatsApp, Signal, Email, Home Assistant
├── Memory           SQLite + FTS5 full-text search
├── Security         8 capability types, hash-chained audit log
└── Gateway          HTTP API (127.0.0.1 only)
```

---

## Why Ferroclaw?

| Problem | Existing Frameworks | Ferroclaw |
|---------|-------------------|-----------|
| Security | OpenClaw: CVE-2026-25253, 0.0.0.0 default, 20% malicious skills | 127.0.0.1 default, 8 capability types, hash-chained audit |
| Weight | 150-200 MB installed, requires Node.js/Python runtime | 5.4 MB single binary, zero runtime deps |
| Context waste | Raw MCP schemas consume thousands of tokens | DietMCP compresses schemas 70-93% |
| Provider lock-in | Most agents support 1-2 providers | 4 providers: Anthropic, OpenAI, Zai GLM, OpenRouter |

---

## Quick Start

```bash
# Build
cargo build --release

# Run the onboarding wizard (providers, security, skills, channels)
./target/release/ferroclaw setup

# Or manual config
./target/release/ferroclaw config init
export ANTHROPIC_API_KEY=sk-ant-...

# Interactive mode
./target/release/ferroclaw run

# One-shot
./target/release/ferroclaw exec "List files in /tmp"
```

## Configuration

Config file: `~/.config/ferroclaw/config.toml`

```toml
[agent]
default_model = "claude-sonnet-4-20250514"   # or "glm-5" or "openai/gpt-4o"
max_iterations = 30
token_budget = 200000

[providers.anthropic]
api_key_env = "ANTHROPIC_API_KEY"

# [providers.zai]
# api_key_env = "ZAI_API_KEY"

# [providers.openrouter]
# api_key_env = "OPENROUTER_API_KEY"
# site_url = "https://your-app.com"

[security]
default_capabilities = ["fs_read", "net_outbound", "memory_read", "memory_write"]
audit_enabled = true

[gateway]
bind = "127.0.0.1"    # NEVER defaults to 0.0.0.0
port = 8420

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

---

## Providers

| Provider | Config Key | Model Examples | Env Variable |
|----------|-----------|----------------|-------------|
| **Anthropic** | `providers.anthropic` | `claude-sonnet-4-20250514`, `claude-opus-4-20250514` | `ANTHROPIC_API_KEY` |
| **Zai GLM** | `providers.zai` | `glm-5`, `glm-5-turbo`, `glm-4.5`, `glm-4.6` | `ZAI_API_KEY` |
| **OpenRouter** | `providers.openrouter` | `openai/gpt-4o`, `anthropic/claude-sonnet-4`, `meta-llama/llama-3.1-70b` | `OPENROUTER_API_KEY` |
| **OpenAI** | `providers.openai` | `gpt-4o`, `gpt-4o-mini` | `OPENAI_API_KEY` |

Routing is automatic based on model name:
- `glm-*` → Zai
- `provider/model` (contains `/`) → OpenRouter
- `claude-*` → Anthropic
- Everything else → OpenAI

---

## Security

Ferroclaw was built to fix the security problems in OpenClaw and similar frameworks.

### Capability System

8 independent capabilities, 4 enabled by default:

| Capability | Default | What it guards |
|-----------|---------|----------------|
| `fs_read` | Yes | File reading, directory listing |
| `fs_write` | **No** | File writing, deletion |
| `net_outbound` | Yes | HTTP requests |
| `net_listen` | **No** | Binding server sockets |
| `process_exec` | **No** | Shell command execution |
| `memory_read` | Yes | Memory search |
| `memory_write` | Yes | Memory storage |
| `browser_control` | **No** | Browser automation |

Every tool call is checked in **15.5 nanoseconds**. Denied calls return actionable error messages.

### Audit Log

Every tool call is logged in a SHA256 hash-chained append-only log:

```bash
# Verify integrity
ferroclaw audit verify
# → Audit log valid: 1,247 entries verified

# View path
ferroclaw audit path
# → ~/.local/share/ferroclaw/audit.jsonl
```

Arguments and results are hashed (never stored in full). Tampering is detected instantly.

### Gateway Safety

The HTTP gateway **refuses to bind 0.0.0.0 without a bearer token**:

```toml
[gateway]
bind = "127.0.0.1"        # Safe: localhost only
# bind = "0.0.0.0"        # Blocked without bearer_token
# bearer_token_env = "FERROCLAW_TOKEN"  # Required for 0.0.0.0
```

---

## DietMCP

Native integration of DietMCP's context compression:

```
Raw JSON schema (9 filesystem tools):     ~4,200 bytes  (~1,050 tokens)
DietMCP compact summary:                  ~800 bytes    (~200 tokens)
Savings:                                  ~81%          (~850 tokens)
```

With 5 MCP servers: **~4,250 tokens saved per LLM request**.

```bash
# View diet summaries for a server
ferroclaw mcp diet filesystem

# Output:
# # filesystem (9 tools)
# ## File Operations
# - read_file(path: str) -- Read the complete contents of a file
# - write_file(path: str, content: str) -- Create or overwrite a file
# ...
```

---

## Tools & Skills

### 7 Built-in Tools

| Tool | Capability | Description |
|------|-----------|-------------|
| `read_file` | `fs_read` | Read file contents |
| `write_file` | `fs_write` | Write to file |
| `list_directory` | `fs_read` | List directory entries |
| `bash` | `process_exec` | Execute shell commands |
| `web_fetch` | `net_outbound` | HTTP GET with size limits |
| `memory_search` | `memory_read` | Full-text search of memories |
| `memory_store` | `memory_write` | Store key-value memories |

### 84 Bundled Skills (16 categories)

| Category | Skills | Examples |
|----------|--------|----------|
| Filesystem | 6 | `find_files`, `tree_view`, `file_info`, `copy_file`, `move_file`, `tail_file` |
| Version Control | 8 | `git_status`, `git_diff`, `git_log`, `git_commit`, `git_branch`, `git_blame` |
| Code Analysis | 6 | `grep_code`, `count_lines`, `find_definition`, `lint_check` |
| Web & HTTP | 5 | `http_get`, `http_post`, `download_file`, `check_url` |
| Database | 5 | `sqlite_query`, `pg_query`, `db_tables`, `db_schema` |
| Docker | 6 | `docker_ps`, `docker_logs`, `docker_exec`, `docker_build` |
| Kubernetes | 5 | `kubectl_get`, `kubectl_describe`, `kubectl_logs`, `kubectl_apply` |
| System | 6 | `process_list`, `system_info`, `disk_usage`, `uptime_info` |
| Text Processing | 5 | `json_query`, `json_file_query`, `yaml_to_json`, `regex_match` |
| Network | 5 | `ping_host`, `port_check`, `dns_lookup`, `curl_request` |
| Security | 5 | `hash_file`, `scan_secrets`, `generate_password`, `encode_base64` |
| Documentation | 5 | `word_count`, `markdown_toc`, `doc_links_check`, `changelog_entry` |
| Testing | 5 | `run_tests`, `test_coverage`, `run_benchmarks`, `test_single` |
| Package Mgmt | 5 | `npm_list`, `pip_list`, `cargo_deps`, `outdated_check` |
| Cloud | 5 | `aws_s3_ls`, `terraform_plan`, `ssh_command`, `env_check` |
| Media | 5 | `image_info`, `image_resize`, `pdf_text`, `archive_create` |

Skills are TOML manifests that delegate to shell commands. Add custom skills to `~/.config/ferroclaw/skills/`:

```toml
[skill]
name = "my_tool"
description = "Custom skill"
version = "0.1.0"
category = "system"

[skill.tool]
type = "bash"
command_template = "my-command {{arg1}} {{?optional_arg}}"

[skill.security]
required_capabilities = ["process_exec"]
```

AgentSkills.io compatible: `ferroclaw skills export --format agentskills`

### MCP Tools

MCP tools are discovered automatically from configured servers and added to the registry.

## Messaging Channels

| Channel | Protocol | Config Section | Auth |
|---------|----------|---------------|------|
| **Telegram** | Bot API (long-polling) | `[telegram]` | Bot token + chat ID allowlist |
| **Discord** | HTTP API | `[channels.discord]` | Bot token + guild allowlist |
| **Slack** | Web API + Socket Mode | `[channels.slack]` | Bot token + channel allowlist |
| **WhatsApp** | Business Cloud API | `[channels.whatsapp]` | API token + number allowlist |
| **Signal** | signal-cli REST API | `[channels.signal]` | Phone number + allowlist |
| **Email** | SMTP/IMAP | `[channels.email]` | SMTP auth + address allowlist |
| **Home Assistant** | REST API | `[channels.homeassistant]` | Long-lived access token |
| **HTTP Gateway** | REST (127.0.0.1) | `[gateway]` | Optional bearer token |

All channels enforce allowlists for access control. No platform SDKs compiled in — just HTTP calls via reqwest.

---

## Performance

| Operation | Time |
|-----------|------|
| Capability check | 15.5 ns |
| Compact signature (1 tool) | 2.8 µs |
| Skill summary (50 tools) | 226 µs |
| FTS5 search (200 entries) | 119 µs |
| Audit verify (1,000 entries) | 2.97 ms |
| Response format (50 KB, minified) | 492 µs |

Full benchmarks: [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md)

---

## Testing

```bash
# Run full test suite (~584 tests)
cargo test --all

# Library tests (~343 tests)
cargo test --lib

# Integration tests (~241 tests)
cargo test --tests

# Run benchmarks
cargo bench

# Specific benchmark
cargo bench --bench diet_compression
```

---

## Features

Ferroclaw extends Claude Code's capabilities with 10 major features:

### Task Management
- **TaskSystem** - SQLite-backed task tracking with dependencies and status workflow
- **PlanMode** - Structured 4-phase planning (Research, Planning, Implementation, Verification)

### Memory & Editing
- **MemdirSystem** - File-based persistent memory with automatic truncation
- **FileEditTool** - Safe file editing through exact string replacement

### Git Workflow
- **Commit Command** - Automated conventional commit generation
- **Review Command** - Code review with quality scoring and issue detection

### Extensibility
- **AgentTool** - Spawn specialized subagents with isolated context
- **HookSystem** - Event-driven extensibility with 6 lifecycle hooks

**Documentation**: See [`FEATURES.md`](FEATURES.md) for complete feature reference

---

## Documentation

| Document | Description |
|----------|-------------|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | System architecture, module map, data flow |
| [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) | Performance benchmarks with Criterion |
| [`docs/SECURITY.md`](docs/SECURITY.md) | Security model, capabilities, audit log |
| [`docs/COMPARISON.md`](docs/COMPARISON.md) | vs. OpenClaw, Hermes, NanoClaw, Claude Code, Codex |
| [`config.example.toml`](config.example.toml) | Annotated configuration template |

---

## Project Structure

```
src/
├── main.rs              CLI entry point
├── lib.rs               Library root
├── cli.rs               Clap subcommands
├── config.rs            TOML config loading
├── error.rs             Unified error types
├── types.rs             Core types (Message, ToolCall, Capability)
├── provider.rs          LlmProvider trait
├── tool.rs              ToolHandler trait + ToolRegistry
├── gateway.rs           HTTP API (127.0.0.1 only)
├── telegram.rs          Telegram bot
├── providers/
│   ├── anthropic.rs     Anthropic Messages API
│   ├── openai.rs        OpenAI-compatible API
│   ├── zai.rs           Zai GLM API
│   ├── openrouter.rs    OpenRouter API
│   └── streaming.rs     SSE utilities
├── mcp/
│   ├── client.rs        MCP client (stdio transport)
│   ├── diet.rs          DietMCP compression
│   ├── cache.rs         Schema cache (SHA256 + TTL)
│   └── registry.rs      Unified tool registry
├── agent/
│   ├── loop.rs          ReAct agent loop
│   └── context.rs       Token budget + context pruning
├── security/
│   ├── capabilities.rs  8-type capability system
│   └── audit.rs         Hash-chained audit log
├── memory/
│   └── store.rs         SQLite + FTS5
├── tools/
│   └── builtin.rs       7 built-in tools
├── skills/
│   ├── manifest.rs      SkillManifest, categories, builders
│   ├── bundled.rs       84 bundled skill definitions
│   ├── loader.rs        Skill discovery + registration
│   ├── executor.rs      Bash command interpolation + dispatch
│   └── agentskills.rs   AgentSkills.io compatibility
└── channels/
    ├── mod.rs           Channel trait + types
    ├── router.rs        Multi-channel message routing
    ├── discord.rs       Discord HTTP API adapter
    ├── slack.rs         Slack Web API adapter
    ├── whatsapp.rs      WhatsApp Business Cloud API adapter
    ├── signal.rs        Signal (signal-cli REST) adapter
    ├── email.rs         SMTP/IMAP adapter
    └── homeassistant.rs Home Assistant REST API adapter
```

## License

MIT
