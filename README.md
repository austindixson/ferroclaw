# ferroclaw

Security-first single-binary Rust agent framework.

## Presentation Framework (Proven README Pattern)

### TL;DR
Security-first Rust agent framework with native MCP support and robust test coverage.

### Why this project
- Solves a concrete workflow problem with reproducible command paths.
- Prioritizes operator reliability over demo-only output.
- Structured for practical use, not just conceptual documentation.

### Quick Start
```bash
cargo run --release
```

### Installation
```bash
rustup default stable
cargo build --release
```

### Usage Examples
```bash
cargo run --release -- --help
cargo test --all
```

### Architecture at a glance
- src/main.rs — CLI entrypoint
- src/lib.rs — shared core modules
- tests/ + benches/ — correctness and performance validation

### Troubleshooting
- If build fails after updates: `cargo clean && cargo build --release`.
- If provider calls fail, validate required API keys and provider config.

### Project status
Hardened reliability + security posture with CI-gated quality checks.


## Installation

```bash
rustup default stable
cargo build --release
```

## Quick Start

```bash
cargo run --release -- --help
cargo run --release
```

## Usage Examples

- Run full test suite
```bash
cargo test --all
```

- Run one test module with output
```bash
cargo test memory_store -- --nocapture
```

- Build optimized binary
```bash
cargo build --release
```

## Implementation Overview

- `src/main.rs` is the primary CLI entrypoint.
- `src/lib.rs` exposes shared internal modules used across command paths.
- `tests/` and `benches/` cover correctness and performance-critical behavior.
- `.github/workflows/ci.yml` defines CI validation gates.

## Troubleshooting

- If build fails after dependency updates, run `cargo clean && cargo build --release`.
- If model/provider commands fail, verify required API keys in your environment before launch.
- If behavior diverges between local and CI, run `cargo test --all` on a clean tree.

## Visual Overview

![ferroclaw visual overview](docs/assets/visual-overview-ferroclaw.svg)


## Problem
Production agents need auditable control loops and constrained side effects.

## Reproducibility
```bash
cargo build --release
cargo test --all
```

## Limits
Provider/tool parity remains an ongoing engineering target.

## Contributing

Contributions are welcome. Open an issue first for significant changes, then submit a focused PR with reproducible validation steps.

## License

See `LICENSE` for terms.
