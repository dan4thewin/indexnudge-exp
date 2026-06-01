#!/usr/bin/env python3
"""Score a pi benchmark session from JSON event stream."""
import json
import sys
import re
from collections import Counter


def classify_command(cmd):
    """Classify a shell command into a navigation category."""
    cmd = cmd.strip()

    if re.search(r'sqlite3.*\bFROM\s+(symbols|calls|imports|defs|directories|files)\b', cmd, re.I):
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


def score_pi_session(lines):
    """Score a pi session from JSON lines."""
    events = []
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            events.append(json.loads(line))
        except json.JSONDecodeError:
            continue

    # Extract tool calls and their results by pairing start/end events
    tool_calls = []
    pending_args = {}  # toolCallId -> args from start event

    for ev in events:
        if ev.get('type') == 'tool_execution_start':
            pending_args[ev.get('toolCallId', '')] = ev.get('args', {})
        elif ev.get('type') == 'tool_execution_end':
            result = ev.get('result', {})
            text = ''
            if isinstance(result, dict):
                content = result.get('content', [])
                if isinstance(content, list):
                    text = ' '.join(
                        item.get('text', '')
                        for item in content
                        if isinstance(item, dict) and item.get('type') == 'text'
                    )
                elif isinstance(content, str):
                    text = content
            elif isinstance(result, str):
                text = result
            call_id = ev.get('toolCallId', '')
            tool_calls.append({
                'name': ev.get('toolName', ''),
                'args': pending_args.pop(call_id, {}),
                'result': text,
                'id': call_id,
            })

    # Compute tokens from agent_end or message_end events
    total_tokens = 0
    input_tokens = 0
    output_tokens = 0
    for ev in events:
        if ev.get('type') == 'message_end':
            msg = ev.get('message', {})
            usage = msg.get('usage', {})
            if usage:
                input_tokens += usage.get('input', 0) + usage.get('cacheRead', 0)
                output_tokens += usage.get('output', 0)
                total_tokens += usage.get('totalTokens', 0)

    categories = Counter()
    commands = []

    for tc in tool_calls:
        name = tc['name']
        args = tc['args']

        if name == 'bash':
            cmd = args.get('command', '')
            cat = classify_command(cmd)

            if cat == 'shim_candidate':
                result = tc['result']
                if 'indexnudge' in result or 'sqlite3' in result:
                    cat = 'shim_hit'
                else:
                    cat = 'shim_fallthrough'

            categories[cat] += 1
            commands.append((cat, cmd[:120]))
        elif name == 'read':
            # pi's read tool is direct file access — equivalent to cat
            path = args.get('path', '')
            if re.search(r'\.(rs|go|ts|js|py|rb|c|h|cpp|hpp|java)$', path):
                categories['file_read'] += 1
                commands.append(('file_read', f'read {path}'))
            else:
                categories['other_read'] += 1
                commands.append(('other_read', f'read {path}'))
        elif name == 'edit' or name == 'write':
            categories['mutation'] += 1
            commands.append(('mutation', f'{name} {args.get("path", "")}'))
        else:
            categories['other_tool'] += 1
            commands.append(('other_tool', f'{name} {json.dumps(args)[:80]}'))

    # Compute metrics
    nav_calls = sum(v for k, v in categories.items()
                    if k in ('direct_db_structural', 'direct_db_lines', 'direct_db_other',
                             'shim_hit', 'shim_fallthrough', 'shim_evasion', 'file_read'))
    if nav_calls == 0:
        nav_calls = 1

    db_structural = categories.get('direct_db_structural', 0)
    db_lines = categories.get('direct_db_lines', 0)
    db_other = categories.get('direct_db_other', 0)
    shim_hit = categories.get('shim_hit', 0)
    shim_fall = categories.get('shim_fallthrough', 0)
    evasion = categories.get('shim_evasion', 0)
    file_read = categories.get('file_read', 0)

    db_total = db_structural + db_lines + db_other + shim_hit
    db_ratio = db_total / nav_calls
    structural_ratio = db_structural / (db_structural + db_lines + file_read) if (db_structural + db_lines + file_read) else 0
    evasion_ratio = evasion / nav_calls
    file_read_ratio = file_read / nav_calls
    line_ratio = db_lines / nav_calls
    tool_call_count = len(tool_calls)
    nav_ratio = nav_calls / tool_call_count if tool_call_count else 0

    return {
        'total_tokens': total_tokens,
        'input_tokens': input_tokens,
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
        'file_read': file_read,
        'db_ratio': round(db_ratio, 3),
        'structural_ratio': round(structural_ratio, 3),
        'evasion_ratio': round(evasion_ratio, 3),
        'file_read_ratio': round(file_read_ratio, 3),
        'line_ratio': round(line_ratio, 3),
        'nav_ratio': round(nav_ratio, 3),
        'commands': commands,
    }


def main():
    if len(sys.argv) < 2:
        print("Usage: pi-score.py <session.jsonl> [--verbose]", file=sys.stderr)
        sys.exit(1)

    verbose = '--verbose' in sys.argv

    with open(sys.argv[1]) as f:
        lines = f.readlines()

    r = score_pi_session(lines)

    print(f"Tokens: {r['total_tokens']} (in={r['input_tokens']}, out={r['output_tokens']})")
    print(f"Tool calls: {r['tool_calls']} | Navigation: {r['nav_calls']}")
    print(f"  DB structural: {r['db_structural']}")
    print(f"  DB lines:      {r['db_lines']}")
    print(f"  DB other:      {r['db_other']}")
    print(f"  Shim hit:      {r['shim_hit']}")
    print(f"  Shim fallthru: {r['shim_fallthrough']}")
    print(f"  Shim evasion:  {r['evasion']}")
    print(f"  File read:     {r['file_read']}")
    print(f"Scores:")
    print(f"  db_ratio:         {r['db_ratio']}")
    print(f"  structural_ratio: {r['structural_ratio']}")
    print(f"  evasion_ratio:    {r['evasion_ratio']}")
    print(f"  file_read_ratio:  {r['file_read_ratio']}")
    print(f"  line_ratio:       {r['line_ratio']}")
    print(f"  nav_ratio:        {r['nav_ratio']}")

    if verbose:
        print(f"\nCategories: {r['categories']}")
        print("\nCommands:")
        for cat, cmd in r['commands']:
            print(f"  [{cat:22s}] {cmd}")


if __name__ == '__main__':
    main()
