use std::string;
use anyhow::anyhow;
use anyhow::Result;



///A struct that holds context from the shell (their outputs, can't get input, see pty.rs)
pub struct ShellContext {
    raw_context: Vec<String>,
}

impl ShellContext {
    pub fn new() -> ShellContext {
        ShellContext {
            raw_context: Vec::new(),
        }
    }

    pub fn push_output(&mut self, output: &str) {
        self.raw_context.push(output.into());
    }

    pub fn get_context(&self) -> String {
        std::println!("length: {}", self.raw_context.len());
        std::println!("vec: {:#?}", self.raw_context);
        let mut ctx = String::new();
        for out in &self.raw_context {
            ctx.push_str(&std::format!("OUTPUT: {}\n", out));
        }
        ctx
    }

}
