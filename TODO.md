- [X] Capture commands and their outputs
    - [X] Document the pipeline
- [X] Implement the termfix command
- [X] Move runtime parsing logic away from pty.rs
- [X] Send output to api and make `termfix fix` work etc.
    - [X] Parse terminal output on command
- [X] Streaming responses, timeout etc. for the api
    - [X] The streamed responses get added to the command, no the output
- [X] Add config file for api keys, and hardcode termfix url in release builds
- [ ] gh actions/gh releases
- [ ] move away from the bootstrap, users should add it to their .zshrc/.bashrc or do it in the install script
- [X] custom instructions (personal info, eg distro etc.)
- [X] installer (in the termfix project, not here)

QOL
- [ ] Parse terminal output while pty is running, speeds up waiting time 
