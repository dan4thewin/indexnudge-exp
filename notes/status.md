title: Status

## Current state

All audit remediation complete (fix-now + strengthen tiers). 16 tasks in notes/tasks/ — all ready. No remote configured for indexnudge.

## Session 2025-04-30

Continued from 04-28 session. Major progress on multiple fronts:

### indexnudge
- Completed all strengthen tier items (incremental FTS, freshen tests, symlink blocking, shared enumerator, callee_qname + types.Info)
- 6 new shim improvement tasks from observing autonomous agent use shims
- Benchmark harness task designed (3 tasks × 2 models × 3 runs, scoring formula)
- Total: 16 task notes, all ready status

### iwe enhancements
- Autonomous session (Opus 4.6) implemented Phases 0-3 (bare frontmatter, --meta filter, virtual dirs)
- Second autonomous session implemented merged retrieve envelope
- 5-model adversarial audit: 2 HIGH fixed, 1 HIGH acknowledged (design limitation), 6 MEDIUM/LOW
- All 900+ tests passing
- Committed and pushed to dan4thewin/iwe branch indexnudge-enhancements
- iwe installed from fork — bare_frontmatter and --meta filtering active on indexnudge notes

### Task management via iwe
- notes/tasks/ with bare front matter (title, status, depends, gates)
- virtual directory auto-generates tasks index
- `iwe find --meta status=ready` for readiness queries
- frontmatter_document_title = "title" in config

## Observations

- ed with line addresses is the right mutation tool when shims annotate output
- edit tool's exact-match requirement breaks the shim loop
- Autonomous agent bypassed shims early (grep on .md file returned empty) and never recovered — shim fallthrough fix needed
- Agent saw "try symbols table" hints but never used them — nudges need to be more structural
- Symbol-grouped shim output proposed as primary nudge mechanism
- iwe --meta filtering works well for task management at current scale
- Multi-model adversarial audit: all 5 models converge on "iwe needs transactional mutation API" as biggest gap
- PR acceptance: poor as single PR, good when split; virtual dir side-effectful reads flagged by all models

## Key decisions

- synchronous=OFF safe under rename protocol (rename is durability boundary)
- File size limits unnecessary — .gitignore support is the right lever
- Bare frontmatter is fork-only feature; --meta filtering is clean upstream PR candidate
- Benchmark harness needed before implementing shim changes (measure what moves the needle)
