# Termfix CLI

The cli is a pty that behaves completely like the normal terminal, except it records commands and their outputs.
It then stores that as a buffer that's send to the backend for termfix responses

## Install (macOS/Linux)
Installation from [termfix](https://termfix.dev), run `curl -fsSL https://termfix.dev/install.sh | bash`
Only works with zsh for now

## How it works:
1. Start a pty with portable-pty
2. Capture raw pty output (except with a few tweaks) in ShellContext.raw_context
3. Use libghostty-rs to parse the raw context (ansi escape codes etc.)

## Configuration
Read full docs from [termfix](https://termfix.dev)
Config is in ~/.termfix/config.toml, logs are in ~/.termfix/logs

## Commands
Read full docs from [termfix](https://termfix.dev)
`termfix start` Start the pty and command recording
`termfix fix --count <n> --message <User message>` Send request to https://termfix.dev to help with terminal errors
`termfix context` Print current context that has been captured
`termfix status` Print current status, inactive/active
