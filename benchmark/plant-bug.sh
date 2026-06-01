#!/bin/bash
# Plant a subtle bug for Task 3 (debug benchmark)
#
# In wrap_bare_frontmatter, change the detection from "title: " (with space)
# to "title:" (no space). This means "title:My Task" would be accepted but
# more importantly, the trailing colon also matches "title:\n" as a valid
# start. The real bug: it now wraps the fences around content that starts
# with "title:" but ISN'T bare frontmatter — it could be a markdown line
# like "title: is required" in body text of a fenced document that lost
# its fences somehow.
#
# But the actual visible effect: the header detection now includes
# the blank line in the header block because find("\n\n") splits at the
# blank line, but the header portion includes a trailing \n. When this
# gets wrapped in ---, the YAML has a trailing newline before the closing
# --- which adds an empty key, causing serde_yaml to parse it differently.
#
# Simpler approach: just break it for notes whose title contains a colon.
# Change the starts_with check to split on first colon and validate the
# key, but do it wrong.
set -e
cd "${IWE_WORKDIR:-$HOME/src/iwe}"

# The bug: after wrapping, strip any line in the header that starts with
# a space (meant to "normalize indentation") — this breaks YAML sequences
# like "depends:\n  - foo" by removing the "  - foo" lines.
ed crates/liwe/src/markdown.rs <<'ED'
/Cow::Owned(format!("---\\n{}\\n---\\n\\n{}", header, rest))/c
        let cleaned_header: String = header
            .lines()
            .filter(|l| !l.starts_with(' '))
            .collect::<Vec<_>>()
            .join("\n");
        Cow::Owned(format!("---\n{}\n---\n\n{}", cleaned_header, rest))
.
/Cow::Owned(format!("---\\n{}\\n---\\n", header))/c
        let cleaned_header: String = header
            .lines()
            .filter(|l| !l.starts_with(' '))
            .collect::<Vec<_>>()
            .join("\n");
        Cow::Owned(format!("---\n{}\n---\n", cleaned_header))
.
w
q
ED
echo "Bug planted: bare frontmatter wrapping strips indented lines (breaks YAML sequences)"
echo "Effect: depends: [foo] works but depends:\\n  - foo loses the items"
echo "Result: --meta filtering on sequence values silently fails for bare frontmatter docs"
