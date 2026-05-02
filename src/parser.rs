use anyhow::Result;
use libghostty_vt::Terminal;
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
use libghostty_vt::TerminalOptions;

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

fn parse_osc_133_event(body: &[u8]) -> Option<u8> {
    let payload = body.strip_prefix(b"133;")?;
    payload.first().copied()
}

fn render_segment(segment: &[u8], terminal: &mut Terminal) -> Result<String> {
    if segment.is_empty() {
        return Ok(String::new());
    }
    parse(segment.to_vec(), terminal)
}

pub fn parse_vec(input: Vec<u8>, _terminal: &mut Terminal) -> Result<Vec<(String, String)>> {
    let mut command_region = Vec::new();
    let mut output_region = Vec::new();
    let mut in_command_region = false;
    let mut in_output_region = false;
    let mut res = Vec::new();
    let mut i = 0usize;

    while i < input.len() {
        if input[i] == 0x1b && i + 1 < input.len() && input[i + 1] == b']' {
            let seq_start = i;
            i += 2;

            let mut term_end: Option<(usize, usize)> = None;
            while i < input.len() {
                if input[i] == 0x07 {
                    term_end = Some((i, 1));
                    break;
                }
                if input[i] == 0x1b && i + 1 < input.len() && input[i + 1] == b'\\' {
                    term_end = Some((i, 2));
                    break;
                }
                i += 1;
            }

            let Some((end_idx, term_len)) = term_end else {
                break;
            };

            let body = &input[seq_start + 2..end_idx];
            if let Some(event) = parse_osc_133_event(body) {
                match event {
                    b'B' => {
                        command_region.clear();
                        in_command_region = true;
                        in_output_region = false;
                    }
                    b'C' => {
                        in_command_region = false;
                        in_output_region = true;
                    }
                    b'D' => {
                        if !command_region.is_empty() || !output_region.is_empty() {
                            let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
                            let mut command_terminal = Terminal::new(TerminalOptions {
                                cols,
                                rows,
                                max_scrollback: 1_000,
                            })?;
                            let mut output_terminal = Terminal::new(TerminalOptions {
                                cols,
                                rows,
                                max_scrollback: 10_000,
                            })?;

                            let command = render_segment(&command_region, &mut command_terminal)?
                                .trim()
                                .to_string();
                            let output = render_segment(&output_region, &mut output_terminal)?
                                .trim_start_matches(['\r', '\n'])
                                .trim_end()
                                .to_string();
                            if !command.is_empty() {
                                res.push((command, output));
                            }
                        }
                        command_region.clear();
                        output_region.clear();
                        in_command_region = false;
                        in_output_region = false;
                    }
                    _ => {}
                }
            }

            i = end_idx + term_len;
            continue;
        }

        if in_command_region {
            command_region.push(input[i]);
        } else if in_output_region {
            output_region.push(input[i]);
        }

        i += 1;
    }

    Ok(res)
}
