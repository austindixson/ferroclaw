# ferroclaw

Security-first single-binary Rust agent framework.

## Problem
Production agents need auditable control loops and constrained side effects.

## Reproducibility
```bash
cargo build --release
cargo test --all
```

## Limits
Provider/tool parity remains an ongoing engineering target.

## Visual Overview

![Install flow](docs/assets/install-flow.png)

![Setup flow](docs/assets/setup-flow.png)

To regenerate these visuals:

```bash
cd docs/remotion
npm install
npm run render:all
```

