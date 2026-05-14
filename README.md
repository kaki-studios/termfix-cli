# Termfix CLI

The cli is a pty that behaves completely like the normal terminal, except it records commands and their outputs.
It then stores that as a buffer that's send to the backend for termfix responses

## Install (macOS/Linux)
Installation from [termfix](https://termfix.dev)

## How it works:
1. Start a pty with portable-pty
2. Capture raw pty output (except with a few tweaks) in ShellContext.raw_context
3. Use libghostty-rs to parse the raw context (ansi escape codes etc.)
