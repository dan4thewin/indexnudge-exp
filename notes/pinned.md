# Pinned

- Investigate goose terminal read / SIGTSTP on `goose run -t` — goose does a terminal read despite the -t switch, causing SIGTSTP when run as a subprocess. Workaround: pipe through `tee` to isolate process groups.
