# ferroclaw

Security-first single-binary Rust agent framework.

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
