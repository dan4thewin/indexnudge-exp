#!/usr/bin/env python3
"""Score a goose benchmark session from exported JSON."""
import json
import sys
import re
from collections import Counter


def classify_command(cmd):
    """Classify a shell command into a navigation category."""
    cmd = cmd.strip()

    # Direct DB structural queries
    if re.search(r'sqlite3.*\bFROM\s+(symbols|calls|imports|defs|directories|files)\b', cmd, re.I):
        return 'direct_db_structural'

    # Direct DB line queries
    if re.search(r'sqlite3.*\bFROM\s+(lines|lines_fts)\b', cmd, re.I):
        return 'direct_db_lines'

    # Direct DB other (e.g., .schema, .tables)
    if re.search(r'sqlite3\s+.*\.indexnudge\.db', cmd):
        return 'direct_db_other'

    # Shim evasion — explicit path to real command
    if re.search(r'/usr/bin/(grep|sed|cat|head|tail)\b', cmd):
        return 'shim_evasion'

    # Shim evasion — 'command' prefix bypasses shim
    if re.search(r'\bcommand\s+(grep|sed|cat|head|tail)\b', cmd):
        return 'shim_evasion'

    # awk used for file reading (evasion)
    if re.search(r'\bawk\b.*NR\s*[><=]', cmd):
        return 'shim_evasion'

    # Build/test commands
    if re.search(r'\b(cargo|go|mk)\s+(build|test|run|check)\b', cmd):
        return 'build_test'

    # Git commands
    if re.search(r'\bgit\b', cmd):
        return 'git'

    # File mutation (ed, write)
    if re.search(r'\bed\b.*<<', cmd):
        return 'mutation'

    # Navigation commands that would hit shims (need response to determine hit/miss)
    if re.search(r'\b(cat|head|tail|grep|sed)\b', cmd) and \
       not re.search(r'/usr/bin/', cmd) and \
       not re.search(r'\bcommand\b', cmd):
        return 'shim_candidate'

    return 'other'


def get_response_text(conv, req_id):
    """Find tool response text matching a request ID."""
    for msg in conv:
        for c in msg.get('content', []):
            if c.get('type') == 'toolResponse' and c.get('id') == req_id:
                result = c.get('toolResult', {})
                if isinstance(result, dict):
                    # Nested: {status, value: {content: [{type: text, text: ...}]}}
                    value = result.get('value', result)
                    if isinstance(value, dict):
                        content = value.get('content', [])
                        if isinstance(content, list):
                            return ' '.join(
                                item.get('text', '')
                                for item in content
                                if isinstance(item, dict) and item.get('type') == 'text'
                            )
                    return str(result)
                return str(result)
    return ''


def score_session(data):
    """Score a session export."""
    conv = data.get('conversation', [])

    total_tokens = data.get('total_tokens', 0)
    input_tokens = data.get('input_tokens', 0)
    output_tokens = data.get('output_tokens', 0)

    commands = []
    categories = Counter()
    tool_calls = 0

    for msg in conv:
        for c in msg.get('content', []):
            if c.get('type') == 'toolRequest':
                tool_calls += 1
                tc = c.get('toolCall', {})
                val = tc.get('value', {})
                name = val.get('name', '')
                args = val.get('arguments', {})
                req_id = c.get('id')

                if name in ('shell', 'developer__shell'):
                    cmd = args.get('command', '')
                    cat = classify_command(cmd)

                    # Resolve shim_candidate by checking response
                    if cat == 'shim_candidate' and req_id:
                        resp = get_response_text(conv, req_id)
                        if 'sqlite3' in resp or 'indexnudge.db' in resp:
                            cat = 'shim_hit'
                        else:
                            cat = 'shim_fallthrough'

                    categories[cat] += 1
                    commands.append((cat, cmd[:120]))
                else:
                    categories['non_shell_tool'] += 1

    # Compute metrics
    nav_calls = sum(v for k, v in categories.items()
                    if k in ('direct_db_structural', 'direct_db_lines', 'direct_db_other',
                             'shim_hit', 'shim_fallthrough', 'shim_evasion'))
    if nav_calls == 0:
        nav_calls = 1

    db_structural = categories.get('direct_db_structural', 0)
    db_lines = categories.get('direct_db_lines', 0)
    db_other = categories.get('direct_db_other', 0)
    shim_hit = categories.get('shim_hit', 0)
    shim_fall = categories.get('shim_fallthrough', 0)
    evasion = categories.get('shim_evasion', 0)

    db_total = db_structural + db_lines + db_other + shim_hit
    db_ratio = db_total / nav_calls
    structural_ratio = db_structural / (db_structural + db_lines) if (db_structural + db_lines) else 0
    evasion_ratio = evasion / nav_calls
    line_ratio = db_lines / nav_calls
    nav_ratio = nav_calls / tool_calls if tool_calls else 0

    return {
        'total_tokens': total_tokens,
        'input_tokens': input_tokens,
        'output_tokens': output_tokens,
        'tool_calls': tool_calls,
        'nav_calls': nav_calls,
        'categories': dict(categories),
        'db_structural': db_structural,
        'db_lines': db_lines,
        'db_other': db_other,
        'shim_hit': shim_hit,
        'shim_fallthrough': shim_fall,
        'evasion': evasion,
        'db_ratio': round(db_ratio, 3),
        'structural_ratio': round(structural_ratio, 3),
        'evasion_ratio': round(evasion_ratio, 3),
        'line_ratio': round(line_ratio, 3),
        'nav_ratio': round(nav_ratio, 3),
        'commands': commands,
    }


def main():
    if len(sys.argv) < 2:
        print("Usage: score.py <session.json> [--verbose]", file=sys.stderr)
        sys.exit(1)

    verbose = '--verbose' in sys.argv

    with open(sys.argv[1]) as f:
        data = json.load(f)

    r = score_session(data)

    print(f"Tokens: {r['total_tokens']} (in={r['input_tokens']}, out={r['output_tokens']})")
    print(f"Tool calls: {r['tool_calls']} | Navigation: {r['nav_calls']}")
    print(f"  DB structural: {r['db_structural']}")
    print(f"  DB lines:      {r['db_lines']}")
    print(f"  DB other:      {r['db_other']}")
    print(f"  Shim hit:      {r['shim_hit']}")
    print(f"  Shim fallthru: {r['shim_fallthrough']}")
    print(f"  Shim evasion:  {r['evasion']}")
    print(f"Scores:")
    print(f"  db_ratio:         {r['db_ratio']}")
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
