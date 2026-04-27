use anyhow::Result;


use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{self, Read, Write};
use std::thread;
struct RawModeGuard;

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

pub fn shell(rows: u16, cols: u16, shell: &str) -> anyhow::Result<()> {

    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(PtySize {
        rows: rows,
        cols: cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let cmd = CommandBuilder::new(shell);
    let mut child = pair.slave.spawn_command(cmd)?;

    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;

    let _raw_mode = RawModeGuard::new()?;

    let output_thread = thread::spawn(move || -> io::Result<()> {
        let mut stdout = io::stdout();
        let mut buffer = [0u8; 4096];

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            stdout.write_all(&buffer[..n])?;
            stdout.flush()?;
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

    println!("\r\nShell exited with status: {status:?}");

    Ok(())
}
