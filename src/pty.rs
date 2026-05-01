use std::sync::Arc;

use crate::parser;
use anyhow::Result;
use clap::Parser;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use libghostty_vt::{Terminal, TerminalOptions};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{self, Read, Write};
use std::sync::Mutex;
use std::{env, thread};

use crate::Commands;
use crate::context::ShellContext;
struct RawModeGuard;
const PROMPT_START_SEQ: &[u8] = b"\x1b]7;";
const COMMAND_END_MARKER: &[u8] = b"\n-----COMMAND END------\n";
const CUSTOM_COMMAND_BOOTSTRAP: &[u8] =
    b"termfix() { printf '\\033]1337;TERMFIX_CMD=%s\\a' \"$1\"; }\nclear\n";

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

fn push_with_command_markers(
    context: &mut ShellContext,
    pending: &mut Vec<u8>,
    chunk: &[u8],
    seen_prompt_start: &mut bool,
) {
    pending.extend_from_slice(chunk);
    let mut out = Vec::with_capacity(chunk.len() + 64);
    let mut i = 0usize;

    while i + PROMPT_START_SEQ.len() <= pending.len() {
        if &pending[i..i + PROMPT_START_SEQ.len()] == PROMPT_START_SEQ {
            if *seen_prompt_start {
                out.extend_from_slice(COMMAND_END_MARKER);
            } else {
                *seen_prompt_start = true;
            }
            out.extend_from_slice(PROMPT_START_SEQ);
            i += PROMPT_START_SEQ.len();
        } else {
            out.push(pending[i]);
            i += 1;
        }
    }

    if i > 0 {
        context.push(&out);
        pending.drain(0..i);
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

fn handle_termfix_command(command: &str, context: &mut ShellContext) -> Result<Vec<u8>> {
    //points to current executable, prob just should do ""
    let mut vec = vec![env::args().into_iter().next().unwrap_or("".to_string())];
    vec.extend(command.split_whitespace().map(|e| e.to_string()));

    //TODO don't exit if arg not present, otherwise termfix exits the pty, check clap docs
    let cli = crate::Cli::parse_from(vec);
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

            let out = parser::parse(context.get_raw_context(), &mut terminal)?;
            std::fs::write("./logs/clean.log", out)?;
            Ok(b"Logs written to logs/clean.log\r\n".to_vec())
        }
        None => Ok(b"Error\r\n".to_vec()),
    }
}

fn process_termfix_commands_stream(
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
                out.extend_from_slice(&handle_termfix_command(&command, context)?);
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

pub fn shell(
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
    writer.flush()?;

    let _raw_mode = RawModeGuard::new()?;
    let copied_ctx = Arc::clone(&shell_ctx);

    let output_thread = thread::spawn(move || -> io::Result<()> {
        let mut stdout = io::stdout();
        let mut buffer = [0u8; 4096];
        let mut termfix_pending = Vec::new();
        let mut clear_filter_pending = Vec::new();
        let mut pending = Vec::new();
        let mut seen_prompt_start = false;

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            if let Ok(mut context) = copied_ctx.lock() {
                let transformed = process_termfix_commands_stream(
                    &mut termfix_pending,
                    &buffer[..n],
                    &mut context,
                )
                .map_err(|_| std::io::Error::last_os_error())?;
                let filtered =
                    strip_clear_sequences_stream(&mut clear_filter_pending, &transformed);
                push_with_command_markers(
                    &mut context,
                    &mut pending,
                    &filtered,
                    &mut seen_prompt_start,
                );
                stdout.write_all(&transformed)?;
                stdout.flush()?;
            } else {
                stdout.write_all(&buffer[..n])?;
                stdout.flush()?;
            }
        }

        if let Ok(mut context) = copied_ctx.lock() {
            if !termfix_pending.is_empty() {
                let transformed =
                    process_termfix_commands_stream(&mut termfix_pending, &[], &mut context)
                        .map_err(|_| std::io::Error::last_os_error())?;
                if !transformed.is_empty() {
                    let filtered =
                        strip_clear_sequences_stream(&mut clear_filter_pending, &transformed);
                    push_with_command_markers(
                        &mut context,
                        &mut pending,
                        &filtered,
                        &mut seen_prompt_start,
                    );
                }
            }
            if !clear_filter_pending.is_empty() {
                let filtered = strip_clear_sequences_stream(&mut clear_filter_pending, &[]);
                if !filtered.is_empty() {
                    push_with_command_markers(
                        &mut context,
                        &mut pending,
                        &filtered,
                        &mut seen_prompt_start,
                    );
                }
            }
            if !pending.is_empty() {
                context.push(&pending);
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

    match output_thread.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => eprintln!("Error reading from PTY: {e}"),
        Err(_) => eprintln!("Output thread panicked"),
    }

    // The input thread can still be blocked on stdin. Detach by dropping it.
    drop(input_thread);

    print!("Shell exited with status: {status:?}\r\n");

    Ok(())
}
