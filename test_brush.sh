#!/bin/bash
# Run patched goose against brush repo to test indexnudge integration.
# Uses | tee to isolate process groups (avoids SIGTSTP propagation).
cd ~/src/brush

# Freshen the index (renamed from .codeindex.db)
# The brush repo still has .codeindex.db from before the rename.
# Rebuild it as .indexnudge.db for the patched goose tree.rs to find.
~/src/indexnudge/bin/indexnudge .

# Patched goose + shims on PATH
PATH=~/src/indexnudge/bin:$PATH ~/src/indexnudge/goose/target/release/goose run \
  -t "You are exploring the brush codebase. Answer these questions by examining the code:
1. What is brush? Summarize the project.
2. How is the shell's command dispatch implemented? What function handles it?
3. What parsing library or approach does brush-parser use?
4. How does brush handle variable expansion?
Give thorough answers with file and function references." \
  --max-turns 12 \
  2>&1 | tee /tmp/indexnudge_brush_run.log
