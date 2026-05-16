# Termfix CLI

**Your terminal, with a safety net.** Termfix silently records every command you run and every output you see — then uses AI to help you fix errors when things go wrong.

---

## What is Termfix?

Termfix is a lightweight terminal wrapper that sits between your shell and the actual terminal emulator. It acts just like your normal terminal — you type commands, you see output, everything feels native — but behind the scenes it captures a complete, structured record of your session.

When you hit an error and don't know why, instead of copy-pasting terminal output into a chatbot or scouring Stack Overflow, you just run:

```
termfix fix --message "why did this fail?"
```

Termfix sends your last commands and their outputs to the [termfix.dev](https://termfix.dev) API, which returns a context-aware AI diagnosis and fix suggestion — streamed directly into your terminal.

---

## Features

- **Transparent recording** — works as a drop-in shell wrapper; there's no new UI, no separate pane, no distraction
- **ANSI-aware parsing** — uses the Ghostty terminal engine (`libghostty-vt`) under the hood to faithfully render ANSI escape sequences, colors, progress bars, and complex TUI output into clean, readable text
- **Streaming AI responses** — fix suggestions stream in real time with a 15-second idle timeout, so you never hang forever
- **Granular context control** — send all commands or only the last `n` to the API; keep sensitive output out of requests
- **Custom instructions** — pre-configure system info (distro, shell, personal preferences) so the AI always has the right context
- **Config-file based** — API keys, custom instructions, and logs all live in `~/.termfix/`; no environment variable sprawl
- **Runs locally** — everything happens on your machine; only the commands you choose get sent to the API

---

## Installation

### Quick install (macOS / Linux)

```bash
curl -fsSL https://termfix.dev/install.sh | bash
```

The installer downloads the prebuilt binary and drops it into your `PATH`.

**Currently only Zsh is supported out of the box.** Bash and other shells are on the roadmap.

### From GitHub Releases

Download the `.tar.gz` for your platform from the [releases page](https://github.com/termfix/termfix-cli/releases), extract it, and place the `termfix` binary somewhere on your `PATH`.

### Build from source

```bash
git clone https://github.com/termfix/termfix-cli.git
cd termfix-cli
cargo build --release
# Binary is at ./target/release/termfix
```

Requires a recent Rust toolchain (edition 2024).

---

## Commands

### `termfix start`

Launches a new shell session wrapped inside a PTY (pseudo-terminal). From this point on, all commands and their outputs are recorded into a session buffer.

```bash
termfix start
# You're now in a recorded session. Use your shell exactly like normal.
```

The session ends when you `exit` the shell.

### `termfix status`

Checks whether termfix is currently recording. Prints `active` or `inactive`.

```bash
termfix status
```

Available both from outside a session (prints `inactive`) and from within a recorded session (prints `active`).

### `termfix context`

Prints the entire captured session context as JSON. Each entry is a command-output pair with ANSI codes fully rendered to plain text.

```bash
termfix context
```

Example output structure:

```json
[
  {
    "command": "ls -la",
    "output": "total 48\ndrwxr-xr-x ..."
  },
  {
    "command": "cargo build",
    "output": "Compiling termfix v0.1.3 ...\n    Finished dev [unoptimized + debuginfo] ..."
  }
]
```

### `termfix fix`

Sends captured commands to the termfix.dev API for AI-powered diagnosis.

```bash
# Fix the last command that errored
termfix fix --message "why is this failing?"

# Send only the last 3 commands
termfix fix --count 3 --message "which package provides this header?"

# Send ALL recorded commands
termfix fix --all --message "summarize what I've been doing"
```

| Flag | Description |
|------|-------------|
| `--message`, `-m` | Your question or problem description |
| `--count`, `-c` | Number of recent commands to include (default: 1) |
| `--all` | Include every recorded command in the session |

`--count` and `--all` are mutually exclusive. If neither is specified, only your last command is sent.

The API response streams directly into your terminal as if it were command output — no separate window, no copy-paste.

---

## Configuration

Termfix looks for its config at `~/.termfix/config.toml`. This file is not created automatically; you'll need to set it up after installing.

### Minimal config

```toml
api_key = "your-api-key-here"
```

### With custom instructions

```toml
api_key = "your-api-key-here"
custom_instructions = """
I'm on Arch Linux with Hyprland. I use zsh with starship prompt.
My terminal is Kitty. I prefer solutions that don't involve Docker.
"""
```

Custom instructions are prepended to every API request so the AI always knows your setup.

### Getting an API key

Sign up at [termfix.dev](https://termfix.dev) to get your API key.

---

## How it Works

Termfix runs on a three-stage pipeline:

### 1. PTY Capture

When you run `termfix start`, the CLI spawns your default shell (via `$SHELL`) inside a pseudo-terminal managed by `portable-pty`. Every byte that flows between the shell and the terminal display is duplicated into `ShellContext`, an in-memory buffer.

Termfix injects two shims into the child shell to make this work:

- **OSC 133 hooks** — `precmd` and `preexec` Zsh hooks emit [OSC 133](https://iterm2.com/documentation-shell-integration.html) escape sequences (`;C` for command start, `;D` for command end) so the parser knows where each command begins and ends
- **Custom command interceptor** — a shell function `termfix()` wraps the CLI so you can run `termfix fix`, `termfix context`, etc. inside a recorded session; it uses OSC 1337 with a custom payload to route commands without leaking them into captured output

### 2. ANSI Parsing

Raw terminal bytes are full of escape sequences — colors, cursor movements, progress bars, clear-screen codes. Termfix replays the entire recorded byte stream through `libghostty-vt`, the same terminal engine that powers the [Ghostty terminal emulator](https://ghostty.org). This produces clean, plain-text output that preserves the visual structure of the original display.

The parser builds a `Vec<(command: String, output: String)>` — an ordered list of every command you ran and exactly what it printed, with all formatting stripped.

### 3. API Integration

When you run `termfix fix`, the CLI:
1. Reads your API key and custom instructions from `~/.termfix/config.toml`
2. Selects the requested number of command-output pairs from the session buffer
3. Sends them to `https://termfix.dev/api/fix` as JSON
4. Streams the response back into the terminal using the same PTY write path, so the output appears in your terminal exactly where you'd expect it

Session logs are also persisted to `~/.termfix/logs/<timestamp>.json` for later reference.

---

## Architecture

```
┌─────────────────────────────────────────┐
│  Your Terminal Emulator                 │
│  (Kitty, Alacritty, iTerm2, etc.)       │
└──────────────┬──────────────────────────┘
               │ stdin / stdout
               ▼
┌─────────────────────────────────────────┐
│  termfix (PTY bridge)                   │
│                                          │
│  ┌─────────┐  ┌──────────┐  ┌────────┐ │
│  │ pty.rs  │  │context.rs│  │parser.rs│ │
│  │ spawn   │──│ capture  │──│ render  │ │
│  │ shell   │  │ bytes    │  │ ANSI    │ │
│  └─────────┘  └──────────┘  └────────┘ │
│                      │                  │
│                      ▼                  │
│               ┌──────────┐              │
│               │ fix.rs   │              │
│               │ API call │              │
│               └──────────┘              │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  libghostty-vt (terminal emulator)      │
│  Renders ANSI → plain text              │
└─────────────────────────────────────────┘
```

| Module | Role |
|--------|------|
| `src/main.rs` | Entrypoint, CLI argument parsing, session orchestration |
| `src/pty.rs` | PTY lifecycle, raw mode, input/output forwarding between terminal and child shell |
| `src/context.rs` | `ShellContext` — thread-safe in-memory buffer for captured terminal output |
| `src/parser.rs` | ANSI parsing via `libghostty-vt`, OSC 133 command boundary detection, segment rendering |
| `src/fix.rs` | API communication, config file reading, streaming response handling |
| `src/helpers.rs` | Shared utilities: raw mode guard, custom command bootstraps, line-ending normalization, stream processing |

---

## Logging and Debugging

Session captures are written to `~/.termfix/logs/` as timestamped JSON files. Each file contains the full parsed command-output history for that session — useful if you want to revisit something after the session ends, or for debugging the parser itself.

In debug builds (`cargo build` / `cargo run`), the API endpoint defaults to `http://localhost:3000`. In release builds, it's hardcoded to `https://termfix.dev`.

---

## Shell Support

| Shell | Status |
|-------|--------|
| Zsh   | ✅ Supported |
| Bash  | 🔜 Planned |
| Fish  | 🔜 Planned |

Shell support requires `precmd`/`preexec`-style hooks to emit OSC 133 command boundaries. Zsh hooks are injected via the bootstrap when the PTY starts. Bash and Fish support will require analogous bootstrap scripts.

---

## Development

```bash
# Check for compilation errors quickly
cargo check

# Build the debug binary
cargo build

# Run locally
cargo run -- start

# Run tests
cargo test

# Format and lint
cargo fmt
cargo clippy -- -D warnings
```

---

## License

Termfix CLI is licensed under the [GNU General Public License v3.0](LICENSE). It is free software: you can redistribute it and/or modify it under the terms of the GPL as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.

---

## Links

- [termfix.dev](https://termfix.dev) — Web app, API docs, and API key signup
- [GitHub Releases](https://github.com/termfix/termfix-cli/releases) — Prebuilt binaries
- [Ghostty Terminal](https://ghostty.org) — The terminal engine used for ANSI parsing
- [OSC 133 Shell Integration](https://iterm2.com/documentation-shell-integration.html) — The escape sequence protocol termfix builds on
