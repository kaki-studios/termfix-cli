# Project revamp
The tty doesn't work (escape codes etc.)
Solution: use pre-existing tools
Basically this except with (named pipes)[https://en.wikipedia.org/wiki/Named_pipe]:

script -q -f raw.log (record session)
ansi2txt raw.log > clean.log (remove ansi escape codes)
col -b < clean.log > extraclean.log (remove control chars etc idk.)

The above works perfectly
