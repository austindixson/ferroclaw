# Quality Policy

## Required gates

All changes should pass:

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --all -q
```

## Lint policy

- Do not add crate-level `#[allow(...)]` for Clippy warnings.
- Avoid function-level `#[allow(...)]` unless there is a strong, documented reason.
- Prefer structural fixes over suppressions.

## Preferred patterns

- Prefer argument structs when a function starts accumulating many parameters.
- Prefer typed conversions (`FromStr`, `Default`, enums) over ad-hoc string handling.
- Prefer clear control flow (`if let ... && ...`) and remove dead/legacy paths only when safe.

## PR checklist

- [ ] Code is formatted.
- [ ] Clippy passes with `-D warnings`.
- [ ] Tests pass.
- [ ] No new broad lint suppressions.
- [ ] Behavior preserved (or explicitly documented if changed).
