//! Spawn an interactive shell using `portable_pty` and bridge bytes between
//! the local terminal and the child PTY.

use anyhow::Result;
use crossterm::terminal::size;
use libghostty_vt::Terminal;
use libghostty_vt::TerminalOptions;
use std::process::exit;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::context::ShellContext;

mod context;
mod helpers;
mod parser;
mod pty;
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Serialize)]
struct CommandOutput {
    command: String,
    output: String,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(subcommand_required = false)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Start,
    Status,
    Context,
    Fix,
}

#[tokio::main]
async fn main() -> Result<()> {
    //TODO the main fn should be responsible for setting up env vars, loading configs
    //and making a buffer for the context for the LLM.
    dotenv::dotenv()?;
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
        Some(Commands::Fix) => {
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

    pty::shell(rows, cols, &std::env::var("SHELL")?, context_arc.clone()).await?;

    //To parse ansi escape sequences, we need to emulate the whole shell session again...
    let mut terminal = Terminal::new(TerminalOptions {
        cols,
        rows,
        max_scrollback: 10_000,
    })?;

    // let out = parser::parse(context_arc.lock().await.get_raw_context(), &mut terminal)?;
    // std::fs::write("./logs/clean.log", out)?;
    let out = parser::parse_vec(context_arc.lock().await.get_raw_context(), &mut terminal)?;
    let payload: Vec<CommandOutput> = out
        .into_iter()
        .map(|(command, output)| CommandOutput { command, output })
        .collect();
    let res = serde_json::to_string(&payload)?;
    std::fs::write("./logs/clean.json", res)?;
    Ok(())
}
