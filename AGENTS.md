# Repository Guidelines

## Project Structure & Module Organization
This is a Rust CLI project focused on PTY capture and terminal parsing.
- `src/main.rs`: entrypoint; wires shell execution, context capture, and parsing.
- `src/pty.rs`: PTY lifecycle, terminal raw mode handling, input/output forwarding.
- `src/context.rs`: shared shell-output buffer (`ShellContext`).
- `src/parser.rs`: converts raw terminal bytes into rendered text using `libghostty-vt`.
- `README.md`: project overview.
- `TODO.md`: active task notes.
- `output.*` and `*.log`: runtime/debug artifacts; treat as generated output.

Keep new modules in `src/` and declare them from `main.rs` or `lib.rs` if introduced.

## Build, Test, and Development Commands
Use Cargo for all development workflows:
- `cargo check`: fast compile validation without producing a binary.
- `cargo build`: build debug binary.
- `cargo run`: run the CLI locally.
- `cargo test`: run unit/integration tests.
- `cargo fmt`: apply Rust formatting.
- `cargo clippy -- -D warnings`: lint and fail on warnings.

Run `cargo fmt && cargo clippy -- -D warnings && cargo test` before opening a PR.

## Coding Style & Naming Conventions
- Follow standard Rust formatting (`rustfmt` defaults, 4-space indentation).
- Use `snake_case` for functions/modules/variables and `CamelCase` for structs/enums.
- Keep modules focused: PTY logic in `pty.rs`, parsing logic in `parser.rs`, state in `context.rs`.
- Prefer `anyhow::Result` in top-level orchestration paths; use explicit error messages for lock/thread/IO failures.

## Testing Guidelines
There is currently no dedicated `tests/` directory; add tests as features stabilize.
- Unit tests: colocate in each module with `#[cfg(test)]`.
- Integration tests: place under `tests/` for end-to-end shell/parsing behavior.
- Test names should describe behavior, e.g. `parse_renders_rows_from_vt_bytes`.

Aim to cover parsing output correctness and PTY thread/exit behavior.

## Commit & Pull Request Guidelines
Current history uses short, informal commit messages. For new work, use clear imperative messages:
- `feat: capture PTY output into context buffer`
- `fix: avoid lock poisoning panic in parser path`

PRs should include:
- concise summary of behavior changes,
- test/lint command results,
- linked issue or task (if applicable),
- terminal output snippets when runtime behavior changes.
