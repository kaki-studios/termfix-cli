//! Spawn an interactive shell using `portable_pty` and bridge bytes between
//! the local terminal and the child PTY.

use anyhow::Result;
use anyhow::anyhow;
use crossterm::terminal::size;
use libghostty_vt::Terminal;
use libghostty_vt::TerminalOptions;
use std::env::args;
use std::os;
use std::process::exit;
use std::sync::Arc;
use std::sync::Mutex;

use crate::context::ShellContext;

mod context;
mod parser;
mod pty;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Start,
    Status,
    Context,
}

fn main() -> Result<()> {
    //TODO the main fn should be responsible for setting up env vars, loading configs
    //and making a buffer for the context for the LLM.
    let cli = Cli::parse();
    match &cli.command {
        Some(Commands::Start {}) => {}
        Some(Commands::Status {}) => {
            println!("inactive");
            println!("Use \"termfix start\" to activate");
            exit(0)
        }
        Some(Commands::Context) => {
            println!("Termfix is inactive, no available context.");
            println!("Use \"termfix start\" to activate");
            exit(0);
        }
        None => {
            eprintln!("Error");
            exit(1);
        }
    }

    let context = ShellContext::new();
    let context_arc = Arc::new(Mutex::new(context));
    let (cols, rows) = size().unwrap_or((80, 24));

    pty::shell(rows, cols, &std::env::var("SHELL")?, context_arc.clone())?;

    //To parse ansi escape sequences, we need to emulate the whole shell session again...
    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 10_000,
    })?;

    let out = parser::parse(
        context_arc
            .lock()
            .map_err(|_| anyhow!("couldn't get the lock"))?
            .get_raw_context(),
        &mut terminal,
    )?;
    std::fs::write("./logs/clean.log", out)?;
    Ok(())
}
