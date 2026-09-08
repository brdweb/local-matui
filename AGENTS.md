# Matui development

Matui is a terminal user interface for Music Assistant. This repository is the
source of truth for the application.

## Architecture and verification

Rust + Ratatui; controller and embedded local playback in the initial scope.
Pin sendspin-rs exactly to 0.3.7 (`sendspin = "=0.3.7"`). Music Assistant
2.10.2 is the target, not a claim of live-tested compatibility. Architecture
and source references live in `docs/architecture.md`.

Use `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
`cargo test --all-targets` with Cargo.lock. Keep hardware-free protocol fixtures
clearly separate from live audio validation. Consult versioned official server
and client sources when changing the integration; do not guess API envelopes.

## Working conventions

- Keep work scoped to this repository. Preserve unrelated changes.
- Record architecture decisions and actual setup/test commands as they emerge.
- Keep credentials, server tokens, local configuration, and personal media data
  out of Git. Provide sanitized examples when configuration is introduced.
- Test relevant behavior before proposing a merge; do not claim checks passed
  unless they ran.
- Use feature branches and reviewable pull requests for application changes.
- Obtain explicit authorization before changing a live Music Assistant server,
  controlling playback, publishing releases, or deploying services.
