# Pinned

- Investigate goose terminal read / SIGTSTP on `goose run -t` — goose does a terminal read despite the -t switch, causing SIGTSTP when run as a subprocess. Workaround: pipe through `tee` to isolate process groups. Root cause undiagnosed — could be end-of-run prompt, provider auth, or session init. The empty logs (0 bytes) suggest it happens early, not at end-of-run. See goose CLI `run` command code for the actual terminal read.
