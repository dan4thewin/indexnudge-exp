#!/bin/bash
# Oneshot goose run against TibbitWiki with Go-indexed .indexnudge.db
cd ~/src/TibbitWiki

PATH=~/src/indexnudge/bin-go:$PATH ~/src/indexnudge/goose/target/release/goose run \
  -t "You are exploring the TibbitWiki codebase. Answer these questions by examining the code:
1. What is TibbitWiki? Summarize the project and its architecture.
2. How does the HTTP server route requests? Trace from main to handler dispatch.
3. How does the wiki use channels for concurrency? What goroutines are running and how do they communicate?
4. How is markdown rendered? What libraries and customizations are involved?
Give thorough answers with file and function references." \
  --max-turns 12 \
  2>&1 | tee /tmp/indexnudge_tibbittest.log
