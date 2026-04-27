//! Spawn an interactive shell using `portable_pty` and bridge bytes between
//! the local terminal and the child PTY.

use anyhow::Result;
use std::env;

mod pty;


fn main() -> Result<()> {
    //TODO the main fn should be responsible for setting up env vars, loading configs and making buffers
    //for command inputs and outputs. these buffers will then be used as context for an LLM when the
    //user asks for it.
    //TODO adapt resolution to current terminal: not always 24x80
    return pty::shell(24, 80, &env::var("SHELL")?);
}
