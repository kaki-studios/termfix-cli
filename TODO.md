- [X] Capture commands and their outputs
    - [X] Document the pipeline
- [X] Implement the termfix command
- [X] Move runtime parsing logic away from pty.rs
- [X] Send output to api and make `termfix fix` work etc.
    - [X] Parse terminal output on command
- [X] Streaming responses, timeout etc. for the api
    - [X] The streamed responses get added to the command, no the output
- [ ] Add config file for api keys, and hardcode termfix url in release builds

QOL
- [ ] Parse terminal output while pty is running, speeds up waiting time 

Late stage improvements
- [ ] Open source the cli but migrate the parsing logic to the api to keep the moat.
