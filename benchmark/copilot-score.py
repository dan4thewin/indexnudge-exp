#!/usr/bin/env python3
"""Score a Copilot CLI benchmark session from --output-format json JSONL.

Handles Copilot's native tool set (bash, view, edit, create, grep, glob,
web_fetch, web_search, sql, etc.) plus shell-command classification matching
pi-score.py's logic so results are directly comparable.
"""
import json
import sys
import re
from collections import Counter


def classify_command(cmd):
    """Classify a shell command into a navigation category. Matches pi-score."""
    cmd = cmd.strip()

    if re.search(r'sqlite3.*\bFROM\s+(symbols|calls|imports|defs|directories|files|blocks|match_arms|refs)\b', cmd, re.I):
        return 'direct_db_structural'
    if re.search(r'sqlite3.*\bFROM\s+(lines|lines_fts)\b', cmd, re.I):
        return 'direct_db_lines'
    if re.search(r'sqlite3\s+.*\.indexnudge', cmd):
        return 'direct_db_other'
    if re.search(r'/usr/bin/(grep|sed|cat|head|tail)\b', cmd):
        return 'shim_evasion'
    if re.search(r'\bcommand\s+(grep|sed|cat|head|tail)\b', cmd):
        return 'shim_evasion'
    if re.search(r'\bawk\b.*NR\s*[><=]', cmd):
        return 'shim_evasion'
    if re.search(r'\b(cargo|go|mk)\s+(build|test|run|check)\b', cmd):
        return 'build_test'
    if re.search(r'\bgit\b', cmd):
        return 'git'
    if re.search(r'\bed\b.*<<', cmd):
        return 'mutation'
    if re.search(r'\b(cat|head|tail|grep|sed)\b', cmd) and \
       not re.search(r'/usr/bin/', cmd) and \
       not re.search(r'\bcommand\b', cmd):
        return 'shim_candidate'
    return 'other'


def score_copilot_session(lines):
    """Score a Copilot CLI session from JSONL events."""
    events = []
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            continue

    # Pair tool.execution_start with tool.execution_complete by toolCallId
    pending = {}  # toolCallId -> (toolName, args)
    tool_calls = []

    for ev in events:
        et = ev.get('type', '')
        d = ev.get('data', {})

        if et == 'tool.execution_start':
            tc_id = d.get('toolCallId', '')
            pending[tc_id] = (d.get('toolName', ''), d.get('arguments', {}))
        elif et == 'tool.execution_complete':
            tc_id = d.get('toolCallId', '')
            name, args = pending.pop(tc_id, ('', {}))
            result = d.get('result', {})
            text = ''
            if isinstance(result, dict):
                content = result.get('content', '') or result.get('detailedContent', '')
                if isinstance(content, str):
                    text = content
                elif isinstance(content, list):
                    text = ' '.join(
                        item.get('text', '')
                        for item in content
                        if isinstance(item, dict) and item.get('type') == 'text'
                    )
            elif isinstance(result, str):
                text = result
            tool_calls.append({
                'name': name,
                'args': args,
                'result': text,
                'success': d.get('success', None),
            })

    # Tokens — Copilot reports outputTokens on assistant.message events.
    # Input tokens are not exposed in the JSONL stream (only request IDs);
    # we report what we can.
    output_tokens = 0
    for ev in events:
        if ev.get('type') == 'assistant.message':
            output_tokens += ev.get('data', {}).get('outputTokens', 0) or 0

    categories = Counter()
    commands = []

    for tc in tool_calls:
        name = tc['name']
        args = tc['args']
        result = tc['result']

        if name == 'bash':
            cmd = args.get('command', '')
            cat = classify_command(cmd)

            if cat == 'shim_candidate':
                if 'indexnudge' in result or 'sqlite3' in result:
                    cat = 'shim_hit'
                else:
                    cat = 'shim_fallthrough'

            categories[cat] += 1
            commands.append((cat, cmd[:120]))
        elif name == 'view':
            path = args.get('path', '')
            categories['native_view'] += 1
            commands.append(('native_view', f'view {path}'))
        elif name == 'grep':
            pattern = args.get('pattern', '')
            categories['native_grep'] += 1
            commands.append(('native_grep', f'grep {pattern!r}'))
        elif name == 'glob':
            pattern = args.get('pattern', '')
            categories['native_glob'] += 1
            commands.append(('native_glob', f'glob {pattern!r}'))
        elif name in ('edit', 'create', 'write'):
            categories['mutation'] += 1
            commands.append(('mutation', f'{name} {args.get("path", "")}'))
        elif name in ('report_intent',):
            categories['meta_tool'] += 1
            commands.append(('meta_tool', name))
        elif name in ('ask_user',):
            categories['meta_tool'] += 1
            commands.append(('meta_tool', f'ask_user {str(args)[:80]}'))
        elif name == 'sql':
            categories['session_sql'] += 1
            commands.append(('session_sql', f'sql {str(args)[:80]}'))
        elif name in ('web_fetch', 'web_search'):
            categories['web'] += 1
            commands.append(('web', f'{name}'))
        elif name == 'task':
            categories['subagent'] += 1
            commands.append(('subagent', f'task {args.get("name","")}'))
        else:
            categories['other_tool'] += 1
            commands.append(('other_tool', f'{name} {json.dumps(args)[:80]}'))

    # Navigation calls — every tool that's used to look at code.
    nav_keys = (
        'direct_db_structural', 'direct_db_lines', 'direct_db_other',
        'shim_hit', 'shim_fallthrough', 'shim_evasion',
        'native_view', 'native_grep', 'native_glob',
    )
    nav_calls = sum(v for k, v in categories.items() if k in nav_keys)
    nav_denom = nav_calls if nav_calls else 1

    db_structural = categories.get('direct_db_structural', 0)
    db_lines = categories.get('direct_db_lines', 0)
    db_other = categories.get('direct_db_other', 0)
    shim_hit = categories.get('shim_hit', 0)
    shim_fall = categories.get('shim_fallthrough', 0)
    evasion = categories.get('shim_evasion', 0)
    native_view = categories.get('native_view', 0)
    native_grep = categories.get('native_grep', 0)
    native_glob = categories.get('native_glob', 0)

    db_total = db_structural + db_lines + db_other + shim_hit
    native_nav = native_view + native_grep + native_glob

    db_ratio = db_total / nav_denom
    native_ratio = native_nav / nav_denom
    structural_denom = db_structural + db_lines + native_view
    structural_ratio = db_structural / structural_denom if structural_denom else 0
    evasion_ratio = evasion / nav_denom
    line_ratio = db_lines / nav_denom
    tool_call_count = len(tool_calls)
    nav_ratio = nav_calls / tool_call_count if tool_call_count else 0

    return {
        'output_tokens': output_tokens,
        'tool_calls': tool_call_count,
        'nav_calls': nav_calls,
        'categories': dict(categories),
        'db_structural': db_structural,
        'db_lines': db_lines,
        'db_other': db_other,
        'shim_hit': shim_hit,
        'shim_fallthrough': shim_fall,
        'evasion': evasion,
        'native_view': native_view,
        'native_grep': native_grep,
        'native_glob': native_glob,
        'db_ratio': round(db_ratio, 3),
        'native_ratio': round(native_ratio, 3),
        'structural_ratio': round(structural_ratio, 3),
        'evasion_ratio': round(evasion_ratio, 3),
        'line_ratio': round(line_ratio, 3),
        'nav_ratio': round(nav_ratio, 3),
        'commands': commands,
    }


def main():
    if len(sys.argv) < 2:
        print("Usage: copilot-score.py <session.jsonl> [--verbose] [--json]", file=sys.stderr)
        sys.exit(1)

    verbose = '--verbose' in sys.argv
    json_out = '--json' in sys.argv

    with open(sys.argv[1]) as f:
        lines = f.readlines()

    r = score_copilot_session(lines)

    if json_out:
        out = {k: v for k, v in r.items() if k != 'commands'}
        print(json.dumps(out))
        return

    print(f"Output tokens: {r['output_tokens']}")
    print(f"Tool calls: {r['tool_calls']} | Navigation: {r['nav_calls']}")
    print(f"  DB structural: {r['db_structural']}")
    print(f"  DB lines:      {r['db_lines']}")
    print(f"  DB other:      {r['db_other']}")
    print(f"  Shim hit:      {r['shim_hit']}")
    print(f"  Shim fallthru: {r['shim_fallthrough']}")
    print(f"  Shim evasion:  {r['evasion']}")
    print(f"  Native view:   {r['native_view']}")
    print(f"  Native grep:   {r['native_grep']}")
    print(f"  Native glob:   {r['native_glob']}")
    print(f"Scores:")
    print(f"  db_ratio:         {r['db_ratio']}")
    print(f"  native_ratio:     {r['native_ratio']}")
    print(f"  structural_ratio: {r['structural_ratio']}")
    print(f"  evasion_ratio:    {r['evasion_ratio']}")
    print(f"  line_ratio:       {r['line_ratio']}")
    print(f"  nav_ratio:        {r['nav_ratio']}")

    if verbose:
        print(f"\nCategories: {r['categories']}")
        print("\nCommands:")
        for cat, cmd in r['commands']:
            print(f"  [{cat:22s}] {cmd}")


if __name__ == '__main__':
    main()
