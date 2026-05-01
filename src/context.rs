use anyhow::Result;
use anyhow::anyhow;
use std::string;

///A struct that holds context from the shell (their outputs, can't get input, see pty.rs)
pub struct ShellContext {
    raw_context: Vec<u8>,
}

impl ShellContext {
    pub fn new() -> ShellContext {
        ShellContext {
            raw_context: Vec::new(),
        }
    }

    pub fn push(&mut self, s: &[u8]) {
        self.raw_context.extend_from_slice(s);
    }

    pub fn get_raw_context(&self) -> Vec<u8> {
        self.raw_context.clone()
    }
}
