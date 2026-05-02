use std::sync::Arc;

use crate::parser;
use anyhow::Result;
use clap::Parser;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use libghostty_vt::{Terminal, TerminalOptions};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::{env, thread};
use tokio::sync::Mutex;

use crate::Commands;
use crate::context::ShellContext;
struct RawModeGuard;
const CUSTOM_COMMAND_BOOTSTRAP: &[u8] =
    b"termfix() { printf '\\033]1337;TERMFIX_CMD=%s\\a' \"$1\"; }\n";
const OSC_133_BOOTSTRAP: &[u8] = b"\n\
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

#[derive(Deserialize)]
struct Response {
    message: String,
}

#[derive(Serialize)]
struct CommandOutput {
    command: String,
    output: String,
}

fn normalize_for_tty(bytes: Vec<u8>) -> Vec<u8> {
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

impl RawModeGuard {
    fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn strip_clear_sequences_stream(pending: &mut Vec<u8>, chunk: &[u8]) -> Vec<u8> {
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

async fn handle_termfix_command(command: &str, context: &mut ShellContext) -> Result<Vec<u8>> {
    //points to current executable, prob just should do ""
    let mut vec = vec![env::args().into_iter().next().unwrap_or("".to_string())];
    vec.extend(command.split_whitespace().map(|e| e.to_string()));

    // Never abort the PTY on CLI parse errors; surface help/error text in-shell.
    let cli = match crate::Cli::try_parse_from(vec) {
        Ok(cli) => cli,
        Err(e) => {
            return Ok(normalize_for_tty(format!("{e}\n").into_bytes()));
        }
    };
    // return format!("{:?}", command.split_whitespace()).bytes().collect();
    match &cli.command {
        Some(Commands::Start {}) => Ok(b"already active\r\n".to_vec()),
        Some(Commands::Status {}) => Ok(b"active\r\n".to_vec()),
        Some(Commands::Context) => {
            let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
            //To parse ansi escape sequences, we need to emulate the whole shell session again...
            let mut terminal = Terminal::new(TerminalOptions {
                cols,
                rows,
                max_scrollback: 10_000,
            })?;

            // let out = parser::parse(context.get_raw_context(), &mut terminal)?;
            // std::fs::write("./logs/clean.log", out)?;
            let out = parser::parse_vec(context.get_raw_context(), &mut terminal)?;
            let payload: Vec<CommandOutput> = out
                .into_iter()
                .map(|(command, output)| CommandOutput { command, output })
                .collect();
            let res = serde_json::to_string(&payload)?;
            std::fs::write("./logs/clean.json", &res)?;
            // Ok(b"Logs written to logs/clean.json\r\n".to_vec())
            Ok(normalize_for_tty(res.into_bytes()))
        }
        Some(Commands::Fix) => {
            let out = {
                let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
                //To parse ansi escape sequences, we need to emulate the whole shell session again...
                let mut terminal = Terminal::new(TerminalOptions {
                    cols,
                    rows,
                    max_scrollback: 10_000,
                })?;
                parser::parse_vec(context.get_raw_context(), &mut terminal)?
            };
            let payload: Vec<CommandOutput> = out
                .into_iter()
                .map(|(command, output)| CommandOutput { command, output })
                .collect();
            let client = reqwest::Client::new();
            //TODO display some text while processing request, check tokio docs + use tokio timeout
            let resp = client
                //TODO if no url is present, this hangs
                .post(format!("{}/api/fix", std::env::var("TERMFIX_API_URL")?))
                .json(&payload)
                .header("Authorization", std::env::var("KEY")?)
                .send()
                .await?;

            Ok(normalize_for_tty(
                resp.json::<Response>().await?.message.into_bytes(),
            ))
        }
        None => Ok(normalize_for_tty(b"Error\n".to_vec())),
    }
}

async fn process_termfix_commands_stream(
    pending: &mut Vec<u8>,
    chunk: &[u8],
    context: &mut ShellContext,
) -> Result<Vec<u8>> {
    pending.extend_from_slice(chunk);
    let mut out = Vec::with_capacity(chunk.len());
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
                match handle_termfix_command(&command, context).await {
                    Ok(bytes) => out.extend_from_slice(&bytes),
                    Err(e) => {
                        out.extend_from_slice(&normalize_for_tty(
                            format!("termfix error: {e}\n").into_bytes(),
                        ));
                    }
                }
            } else {
                out.extend_from_slice(&pending[seq_start..end_idx + term_len]);
            }
            i = end_idx + term_len;
            continue;
        }

        out.push(pending[i]);
        i += 1;
    }

    if i > 0 {
        pending.drain(0..i);
    }

    Ok(out)
}

pub async fn shell(
    rows: u16,
    cols: u16,
    shell: &str,
    shell_ctx: Arc<Mutex<ShellContext>>,
) -> anyhow::Result<()> {
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(PtySize {
        rows: rows,
        cols: cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new(shell);
    cmd.cwd(std::env::current_dir()?);
    let mut child = pair.slave.spawn_command(cmd)?;

    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;

    // Register built-in termfix commands in the spawned shell session.
    // This keeps them behaving like normal shell commands inside the PTY.
    writer.write_all(CUSTOM_COMMAND_BOOTSTRAP)?;
    writer.write_all(OSC_133_BOOTSTRAP)?;
    writer.flush()?;

    let _raw_mode = RawModeGuard::new()?;
    let copied_ctx = Arc::clone(&shell_ctx);

    let output_thread: tokio::task::JoinHandle<anyhow::Result<()>> = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let mut buffer = [0u8; 4096];
        let mut termfix_pending = Vec::new();
        let mut clear_filter_pending = Vec::new();

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            let mut context = copied_ctx.lock().await;
            let transformed =
                process_termfix_commands_stream(&mut termfix_pending, &buffer[..n], &mut context)
                    .await?;
            let filtered = strip_clear_sequences_stream(&mut clear_filter_pending, &transformed);
            context.push(&filtered);
            stdout.write_all(&transformed)?;
            stdout.flush()?;
        }

        let mut context = copied_ctx.lock().await;
        if !termfix_pending.is_empty() {
            let transformed =
                process_termfix_commands_stream(&mut termfix_pending, &[], &mut context)
                    .await
                    .map_err(|_| std::io::Error::last_os_error())?;
            if !transformed.is_empty() {
                let filtered =
                    strip_clear_sequences_stream(&mut clear_filter_pending, &transformed);
                context.push(&filtered);
            }
        }
        if !clear_filter_pending.is_empty() {
            let filtered = strip_clear_sequences_stream(&mut clear_filter_pending, &[]);
            if !filtered.is_empty() {
                context.push(&filtered);
            }
        }

        Ok(())
    });

    let input_thread = thread::spawn(move || -> io::Result<()> {
        let mut stdin = io::stdin();
        let mut buffer = [0u8; 4096];

        loop {
            let n = stdin.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            writer.write_all(&buffer[..n])?;
            writer.flush()?;
        }

        Ok(())
    });

    let status = child.wait()?;

    match output_thread.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("Error reading from PTY: {e}"),
        Err(_) => eprintln!("Output thread panicked"),
        // Ok(_) => {}
        // Err(e) => eprintln!("Error on thread: {}", e),
    }

    // The input thread can still be blocked on stdin. Detach by dropping it.
    drop(input_thread);

    print!("Shell exited with status: {status:?}\r\n");

    Ok(())
}
