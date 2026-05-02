- [X] Capture commands and their outputs
    - [X] Document the pipeline
- [X] Implement the termfix command
- [ ] Move runtime parsing logic away from pty.rs
- [X] Send output to api and make `termfix fix` work etc.
    - [X] Parse terminal output on command
- [ ] Streaming responses, timeout etc. for the api

QOL
- [ ] Parse terminal output while pty is running, speeds up waiting time 

Late stage improvements
- [ ] Open source the cli but migrate the parsing logic to the api to keep the moat.
