use anyhow::Result;
use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};

fn strip_legacy_title_sequences(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        // Strip legacy title sequences emitted by some zsh prompt setups:
        // ESC k ... ESC \
        if input[i] == 0x1b && i + 1 < input.len() && input[i + 1] == b'k' {
            i += 2;
            while i + 1 < input.len() {
                if input[i] == 0x1b && input[i + 1] == b'\\' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }

        out.push(input[i]);
        i += 1;
    }

    out
}

pub fn parse(input: Vec<u8>, terminal: &mut Terminal) -> Result<String> {
    let cleaned = strip_legacy_title_sequences(&input);
    terminal.vt_write(&cleaned);

    let mut formatter = Formatter::new(
        terminal,
        FormatterOptions {
            format: Format::Plain,
            trim: true,
            unwrap: true,
            selection: None,
        },
    )?;
    let out = formatter.format_alloc(None)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}
