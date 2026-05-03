///This file is basically just a collection of functions for runtime parsing and termfix command
///handling in the pty.
use crate::Commands;
use crate::context::ShellContext;
use crate::parser;
use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use futures_util::StreamExt;
use libghostty_vt::{Terminal, TerminalOptions};
use serde::Serialize;
use std::env;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, timeout};

pub const CUSTOM_COMMAND_BOOTSTRAP: &[u8] =
    b"termfix() { printf '\\033]1337;TERMFIX_CMD=%s\\a' \"$1\"; }\n";
pub const OSC_133_BOOTSTRAP: &[u8] = b"\n\
_termfix_append_prompt_markers() {\n\
  local bmark=$'%{\\e]133;B\\a%}'\n\
  case \"${PROMPT-}\" in\n\
    *$'\\e]133;B'*) ;;\n\
    *) PROMPT=\"${PROMPT}${bmark}\" ;;\n\
  esac\n\
}\n\
_termfix_precmd() {\n\
  printf '\\033]133;D;%s\\a\\033]133;A\\a' \"$?\"\n\
  _termfix_append_prompt_markers\n\
}\n\
_termfix_preexec() { printf '\\033]133;C\\a'; }\n\
if [ -n \"${ZSH_VERSION:-}\" ]; then\n\
  autoload -Uz add-zsh-hook 2>/dev/null\n\
  add-zsh-hook precmd _termfix_precmd\n\
  add-zsh-hook preexec _termfix_preexec\n\
fi\nclear\n";

pub struct RawModeGuard;

impl RawModeGuard {
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[derive(Serialize)]
pub struct CommandOutput {
    pub command: String,
    pub output: String,
}

pub enum CommandExecution {
    Buffered(Vec<u8>),
    Streamed,
}

pub struct ProcessedPtyChunk {
    pub display: Vec<u8>,
    pub capture: Vec<u8>,
}

pub fn normalize_for_tty(bytes: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 16);
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            if i == 0 || bytes[i - 1] != b'\r' {
                out.push(b'\r');
            }
            out.push(b'\n');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

pub fn strip_clear_sequences_stream(pending: &mut Vec<u8>, chunk: &[u8]) -> Vec<u8> {
    pending.extend_from_slice(chunk);
    let mut out = Vec::with_capacity(chunk.len());
    let mut i = 0usize;

    while i < pending.len() {
        if pending[i] == 0x0c {
            // Ctrl+L form feed; keep it out of captured context.
            i += 1;
            continue;
        }

        if pending[i] == 0x1b && i + 1 < pending.len() && pending[i + 1] == b'[' {
            let seq_start = i;
            i += 2;

            while i < pending.len() {
                let b = pending[i];
                if (0x40..=0x7e).contains(&b) {
                    let final_byte = b as char;
                    let params = &pending[seq_start + 2..i];
                    i += 1;

                    let drop_seq = final_byte == 'J'
                        || (final_byte == 'H'
                            && (params.is_empty() || params == b"1;1" || params == b"1"));

                    if !drop_seq {
                        out.extend_from_slice(&pending[seq_start..i]);
                    }
                    break;
                }
                i += 1;
            }

            // Incomplete CSI sequence at the end of the buffer: wait for next chunk.
            if i >= pending.len() {
                break;
            }
            continue;
        }

        out.push(pending[i]);
        i += 1;
    }

    if i > 0 {
        pending.drain(0..i);
    }

    out
}

fn build_payload_from_raw_context(raw_context: Vec<u8>) -> Result<Vec<CommandOutput>> {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 10_000,
    })?;

    let out = parser::parse_vec(raw_context, &mut terminal)?;
    Ok(out
        .into_iter()
        .map(|(command, output)| CommandOutput { command, output })
        .collect())
}

pub async fn handle_termfix_command(
    command: &str,
    shell_ctx: &Arc<Mutex<ShellContext>>,
    stdout: &mut io::Stdout,
) -> Result<CommandExecution> {
    //points to current executable, prob just should do ""
    let mut vec = vec![env::args().into_iter().next().unwrap_or("".to_string())];
    vec.extend(command.split_whitespace().map(|e| e.to_string()));

    // Never abort the PTY on CLI parse errors; surface help/error text in-shell.
    let cli = match crate::Cli::try_parse_from(vec) {
        Ok(cli) => cli,
        Err(e) => {
            return Ok(CommandExecution::Buffered(normalize_for_tty(
                format!("{e}\n").into_bytes(),
            )));
        }
    };
    // return format!("{:?}", command.split_whitespace()).bytes().collect();
    match &cli.command {
        Some(Commands::Start {}) => Ok(CommandExecution::Buffered(b"already active\r\n".to_vec())),
        Some(Commands::Status {}) => Ok(CommandExecution::Buffered(b"active\r\n".to_vec())),
        Some(Commands::Context) => {
            let raw = shell_ctx.lock().await.get_raw_context();
            let payload = build_payload_from_raw_context(raw)?;
            let res = serde_json::to_string(&payload)?;
            std::fs::write("./logs/clean.json", &res)?;
            Ok(CommandExecution::Buffered(normalize_for_tty(
                format!("{}\r\n", res).into_bytes(),
            )))
        }
        Some(Commands::Fix) => {
            let raw = shell_ctx.lock().await.get_raw_context();
            let payload = build_payload_from_raw_context(raw)?;
            let client = reqwest::Client::new();
            let resp = client
                .post(format!("{}/api/fix", std::env::var("TERMFIX_API_URL")?))
                .json(&payload)
                .header("Authorization", format!("Bearer {}", std::env::var("KEY")?))
                .send()
                .await?;

            let mut stream = resp.bytes_stream();
            loop {
                let next_chunk = timeout(Duration::from_secs(15), stream.next()).await;
                let chunk = match next_chunk {
                    Ok(Some(Ok(chunk))) => chunk,
                    Ok(Some(Err(e))) => return Err(anyhow!("stream read error: {e}")),
                    Ok(None) => break,
                    Err(_) => {
                        let msg = normalize_for_tty(
                            b"\ntermfix error: stream timed out after 15s idle\n".to_vec(),
                        );
                        stdout.write_all(&msg)?;
                        stdout.flush()?;
                        shell_ctx.lock().await.push(&msg);
                        return Ok(CommandExecution::Streamed);
                    }
                };

                let normalized = normalize_for_tty(chunk.to_vec());
                stdout.write_all(&normalized)?;
                stdout.flush()?;
                shell_ctx.lock().await.push(&normalized);
            }

            Ok(CommandExecution::Streamed)
        }
        None => Ok(CommandExecution::Buffered(normalize_for_tty(
            b"Error\n".to_vec(),
        ))),
    }
}

pub async fn process_termfix_commands_stream(
    pending: &mut Vec<u8>,
    chunk: &[u8],
    shell_ctx: &Arc<Mutex<ShellContext>>,
    stdout: &mut io::Stdout,
) -> Result<ProcessedPtyChunk> {
    pending.extend_from_slice(chunk);
    let mut display = Vec::with_capacity(chunk.len());
    let mut capture = Vec::with_capacity(chunk.len());
    let mut i = 0usize;

    while i < pending.len() {
        if pending[i] == 0x1b && i + 1 < pending.len() && pending[i + 1] == b']' {
            let seq_start = i;
            i += 2;

            let mut term_end: Option<(usize, usize)> = None;
            while i < pending.len() {
                if pending[i] == 0x07 {
                    term_end = Some((i, 1));
                    break;
                }
                if pending[i] == 0x1b && i + 1 < pending.len() && pending[i + 1] == b'\\' {
                    term_end = Some((i, 2));
                    break;
                }
                i += 1;
            }

            let Some((end_idx, term_len)) = term_end else {
                break;
            };

            let body = &pending[seq_start + 2..end_idx];
            if let Some(command) = body.strip_prefix(b"1337;TERMFIX_CMD=") {
                let command = String::from_utf8_lossy(command);
                match handle_termfix_command(&command, shell_ctx, stdout).await {
                    Ok(CommandExecution::Buffered(bytes)) => {
                        display.extend_from_slice(&bytes);
                        if command.trim() == "context" {
                            capture.extend_from_slice(&normalize_for_tty(
                                b"<User session context>\n".to_vec(),
                            ));
                        } else {
                            capture.extend_from_slice(&bytes);
                        }
                    }
                    Ok(CommandExecution::Streamed) => {}
                    Err(e) => {
                        let err = normalize_for_tty(format!("termfix error: {e}\n").into_bytes());
                        display.extend_from_slice(&err);
                        capture.extend_from_slice(&err);
                    }
                }
            } else {
                display.extend_from_slice(&pending[seq_start..end_idx + term_len]);
                capture.extend_from_slice(&pending[seq_start..end_idx + term_len]);
            }
            i = end_idx + term_len;
            continue;
        }

        display.push(pending[i]);
        capture.push(pending[i]);
        i += 1;
    }

    if i > 0 {
        pending.drain(0..i);
    }

    Ok(ProcessedPtyChunk { display, capture })
}
