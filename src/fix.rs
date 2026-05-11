///This file is just for functionality related to the `termfix fix` command
use anyhow::anyhow;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::io::{self, Write};
use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::Duration;
use tokio::time::timeout;

use anyhow::Result;
use tokio::sync::Mutex;

use crate::context::ShellContext;
use crate::helpers::*;

#[derive(Deserialize)]
struct Config {
    api_key: String,
    custom_instructions: Option<String>,
}

#[cfg(debug_assertions)]
const TERMFIX_API_URL: &str = "http://localhost:3000";
#[cfg(not(debug_assertions))]
const TERMFIX_API_URL: &str = "https://termfix.kaki.foo";

#[derive(Serialize, Debug)]
struct FixPayload {
    custom_instructions: Option<String>,
    commands: Vec<CommandOutput>,
    message: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Count {
    All,
    Number(u32),
}

impl Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Count::All => f.pad("all"),
            Count::Number(n) => f.pad(&n.to_string()),
        }
    }
}

impl FromStr for Count {
    type Err = ParseIntError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "all" => Ok(Count::All),
            other => u32::from_str(other).map(Count::Number),
        }
    }
}

pub async fn fix(
    shell_ctx: &Arc<Mutex<ShellContext>>,
    stdout: &mut io::Stdout,
    message: Option<String>,
    count: crate::Count,
) -> Result<()> {
    let raw = shell_ctx.lock().await.get_raw_context();
    let all_commands = build_payload_from_raw_context(raw)?;
    let commands = match count {
        Count::All => all_commands,
        Count::Number(n) => {
            let start = all_commands.len().saturating_sub(n as usize);
            all_commands[start..].to_vec()
        }
    };
    let config_home =
        std::env::var("XDG_CONFIG_HOME").unwrap_or(format!("{}/.config", std::env::var("HOME")?));
    let file = std::fs::read_to_string(format!("{}/termfix/config.toml", config_home))?;
    let config: Config = toml::from_str(&file)?;

    let payload = FixPayload {
        custom_instructions: config.custom_instructions,
        commands,
        message,
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/api/fix", TERMFIX_API_URL))
        .json(&payload)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .send()
        .await?;

    // Ensure streamed bytes are categorized as command output by OSC 133 parser.
    let force_output_region = b"\x1b]133;C\x07".to_vec();
    stdout.write_all(&force_output_region)?;
    stdout.flush()?;
    shell_ctx.lock().await.push(&force_output_region);

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
                return Ok(());
            }
        };

        let normalized = normalize_for_tty(chunk.to_vec());
        stdout.write_all(&normalized)?;
        stdout.flush()?;
        shell_ctx.lock().await.push(&normalized);
    }
    Ok(())
}
