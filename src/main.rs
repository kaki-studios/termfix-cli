//! Spawn an interactive shell using `portable_pty` and bridge bytes between
//! the local terminal and the child PTY.

use anyhow::Result;
use anyhow::anyhow;
use libghostty_vt::Terminal;
use libghostty_vt::TerminalOptions;
use std::sync::Arc;
use std::sync::Mutex;

use crate::context::ShellContext;

mod context;
mod pty;
mod parser;

fn main() -> Result<()> {
    //TODO the main fn should be responsible for setting up env vars, loading configs and making buffers
    //for command inputs and outputs. these buffers will then be used as context for an LLM when the
    //user asks for it.

    let context = ShellContext::new();
    let context_arc = Arc::new(Mutex::new(context));

    pty::shell(24, 80, &std::env::var("SHELL")?, context_arc.clone())?;

    std::fs::write(
        "./output.raw",
        context_arc
            .lock()
            .map_err(|_| anyhow!("couldn't get the lock"))?
            .get_raw_context(),
    )?;
    let mut terminal = Terminal::new(TerminalOptions {
        cols: 80,
        rows: 24,
        max_scrollback: 10_000,
    })?;



    let out = parser::parse(context_arc
            .lock()
            .map_err(|_| anyhow!("couldn't get the lock"))?
            .get_raw_context(), &mut terminal)?;
    std::println!("{}", out);





    Ok(())
}
