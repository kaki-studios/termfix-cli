use std::sync::Arc;

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{self, Read, Write};
use std::thread;
use tokio::sync::Mutex;

use crate::context::ShellContext;
use crate::helpers::*;

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
    writer.write_all(crate::helpers::CUSTOM_COMMAND_BOOTSTRAP)?;
    writer.write_all(crate::helpers::OSC_133_BOOTSTRAP)?;
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

            let transformed =
                process_termfix_commands_stream(
                    &mut termfix_pending,
                    &buffer[..n],
                    &copied_ctx,
                    &mut stdout,
                )
                    .await?;
            let filtered =
                strip_clear_sequences_stream(&mut clear_filter_pending, &transformed.capture);
            copied_ctx.lock().await.push(&filtered);
            if !transformed.display.is_empty() {
                stdout.write_all(&transformed.display)?;
                stdout.flush()?;
            }
        }

        if !termfix_pending.is_empty() {
            let transformed =
                process_termfix_commands_stream(&mut termfix_pending, &[], &copied_ctx, &mut stdout)
                    .await
                    .map_err(|_| std::io::Error::last_os_error())?;
            if !transformed.display.is_empty() || !transformed.capture.is_empty() {
                let filtered =
                    strip_clear_sequences_stream(&mut clear_filter_pending, &transformed.capture);
                copied_ctx.lock().await.push(&filtered);
                if !transformed.display.is_empty() {
                    stdout.write_all(&transformed.display)?;
                    stdout.flush()?;
                }
            }
        }
        if !clear_filter_pending.is_empty() {
            let filtered = strip_clear_sequences_stream(&mut clear_filter_pending, &[]);
            if !filtered.is_empty() {
                copied_ctx.lock().await.push(&filtered);
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
