#!/bin/bash
# Benchmark task prompts

PROMPT_EXPLORE='How does `iwe find --meta` filter documents? Trace the data flow from CLI argument parsing through to the filtered result set. List every function involved and explain the filtering logic. Do not modify any files.'

PROMPT_IMPLEMENT='Add a `--count` flag to `iwe find` that prints only the number of matching documents instead of the full list. The flag should work with all existing filters (--meta, --roots, --refs-to, etc.). Ensure cargo build and cargo test pass.'

PROMPT_DEBUG='Users report that `iwe find --meta depends=foo` returns no results for bare frontmatter documents, even though those documents have a `depends:` field with `- foo` in their front matter. The same query works for documents with fenced YAML (---) front matter. All tests pass. Diagnose and fix. Ensure cargo test still passes after the fix.'
