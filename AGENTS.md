# Repository Guidelines

## Project Goal & Structure

Port `python-snap7` to an idiomatic, maintainable Rust library. Use the local Python implementation, documentation, and tests as behavioral references. Track migrated features, compatibility differences, and unsupported functionality. Preserve applicable upstream licenses and attribution when reusing code.

`src/lib.rs` is the library entry point. Introduce focused modules for protocol encoding, transport, clients, data types, and errors as features are implemented. Keep protocol logic separate from network I/O. Place integration tests in `tests/` and runnable examples in `examples/` when needed.

`python-snap7/` contains the local upstream reference and is excluded from this repository's Git tracking. Read its own `AGENTS.md` before editing that directory.

## Build, Test & Development Commands

Run from the repository root:

- `cargo build`: compile the library.
- `cargo fmt --all -- --check`: check formatting; use `cargo fmt --all` to fix it.
- `cargo clippy --all-targets --all-features -- -D warnings`: run lint checks; check mutually exclusive features separately if introduced.
- `cargo test --all-targets`: run unit and integration tests.
- `cargo test --doc`: verify documentation examples.

Run applicable checks before submission and report failures or skipped checks.

## Coding Style & Documentation

Follow `rustfmt` with four-space indentation. Use `snake_case` for files, modules, functions, and variables; `PascalCase` for types and traits; and `SCREAMING_SNAKE_CASE` for constants.

Write comments and `AGENTS.md` in English. Keep `README.md` in English and maintain `README.zh-CN.md` as its Chinese counterpart, with reciprocal links and synchronized content.

Document each source file with `//!` and types, traits, functions, and methods with `///`. Explain purpose, input constraints, return values, and errors where applicable. Comment critical or complex logic, especially protocol fields, byte order, length validation, and state transitions. Update comments alongside behavior changes.

Use readable multiline expressions, clear control flow, and focused functions. Return explicit errors for invalid external input. Explain safety invariants whenever `unsafe` is necessary.

## Testing Guidelines

Use Rust's built-in test framework. Cover normal, boundary, and error cases for every migrated feature, including malformed packets, timeouts, disconnections, and reconnections. Add regression tests for bug fixes; name tests after conditions and expected outcomes.

Default tests must run without PLC hardware, using fixed packets, mock transports, or local servers. Derive compatibility cases from Python tests. Hardware tests require explicit opt-in and documented environments. No numerical coverage threshold is configured; protocol parsing and error branches require assertions.

## Commits & Pull Requests

No commit convention has been established yet. Use focused commits with `feat:`, `fix:`, `test:`, `docs:`, or `refactor:` prefixes.

PRs should describe migration scope, upstream behavior, interface changes, compatibility impact, related issues, test results, and remaining limitations. Update documentation and examples with interface changes.
