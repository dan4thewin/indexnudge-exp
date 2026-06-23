# Feature Request: resolve qualified call targets + struct / transitive dead-code detection

Follow-up to `feature-request-dead-code-detection.md` (the `enum_variant_refs`
view, now shipped, schema v11). Using that view to clear **5 dead enum
variants** in safebash (2026-06-22) exposed three concrete gaps that cap how
far dead-code detection can go. In priority order:

---

## 1. `calls` drops every path-qualified call site (highest leverage)

The indexer records a call only when it's written as a **bare name** or
**`receiver.method()`**. It **drops the call** when written with a path
prefix — `mod::fn()`, `super::…::fn()`, `crate::…::fn()`, `Self::fn()`.

Evidence on safebash (calls that demonstrably exist by grep, with **zero
rows** in `calls`):

| call site (verified)                                            | `calls` rows |
|----------------------------------------------------------------|-------------:|
| `resolve::resolve_commands(…)`  (lib.rs:109)                    | 0 |
| `super::inst_handlers::handle_general_command(…)` (dispatch.rs) | 0 |
| `self.emit_pwd(…)` (dispatch.rs:211) — method, **captured**     | 1 |

`calls.callee_qname` is **always NULL** for Rust (no path resolution).

**Impact.** This codebase calls free functions predominantly through
`super::`/`crate::` module paths, so most free-function call sites are
invisible to `calls`. The obvious dead-function query —

```sql
SELECT name FROM symbols
WHERE kind='function' AND file LIKE 'sash/src/%'
  AND name NOT IN (SELECT DISTINCT callee FROM calls);
```

— returns **68** candidates that are **overwhelmingly live**: `cli_main`,
`compile_to_object`, `resolve_commands`, `handle_general_command`,
`intern_literals`, … Each spot-checked candidate has 0 `calls` rows **and**
a real `::fn(` callsite. So `calls` answers "what does X call" reasonably,
but is **not a usable dead-function oracle** here.

### Proposed

For a `call_expression` whose function is a `scoped_identifier`
(`a::b::foo`) or `Self::foo`:
- record the **final segment** (`foo`) as `callee` (matches how bare /
  method calls are already keyed), and
- populate **`callee_qname`** with the full written path
  (`super::inst_handlers::handle_general_command`) for disambiguation.

Then "symbols never appearing as `calls.callee`" becomes a usable
dead-function candidate generator, with a documented residual-false-positive
list: trait-impl methods invoked through the trait, FFI
(`#[no_mangle]` / `extern "C"`), entry points (`main`, `cli_main`), and
`#[test]` / `#[cfg(test)]` functions. (Cross-checking against rustc helps —
see §4.)

---

## 2. No never-constructed-**struct** detector (the struct analog of `enum_variant_refs`)

`enum_variant_refs` nails dead enum variants. The same idea applied to
**structs** would have been a one-query find instead of an ad-hoc one. I
hand-rolled this over `lines`:

```sql
WITH structs AS (SELECT DISTINCT name FROM symbols
                 WHERE file LIKE 'sash/src/%' AND kind='struct')
SELECT s.name FROM structs s
WHERE NOT EXISTS (
  SELECT 1 FROM lines l
  WHERE (l.content LIKE '%'||s.name||' {%' OR l.content LIKE '%'||s.name||'(%')
    AND l.content NOT LIKE '%struct '||s.name||'%');
```

It found a **real dead struct** — `Coercion` (safebash `emit_source.rs`):
never constructed, only the element type of a `static COERCIONS:
Vec<Coercion>` that is initialized empty and never pushed to (a leftover
from when `--emit-source` was removed). But it also produced a **false
positive**, `Interner`, which *is* built via `Interner::new()` — a form the
`Name {` / `Name(` LIKE patterns can't see.

### Proposed: a `struct_refs` view (or generalize to `type_refs`)

Classify each `TypeName` occurrence by context, exactly like
`string_literals` / `enum_variant_refs`:

| context     | matches                                                        |
|-------------|---------------------------------------------------------------|
| `construct` | `Name { … }`, `Name( … )` (tuple), **and** associated ctors `Name::new(…)` / `Name::default()` / `Name::from(…)` returning `Self` |
| `type`      | type position — field type, fn signature, generic arg (`Vec<Name>`) |

Dead candidate = a struct with `type` refs but **no** `construct` ref. The
`::new`/`::default`/`::from` handling is what removes the `Interner` false
positive.

```sql
SELECT s.name FROM (SELECT DISTINCT name AS sname FROM struct_refs) s
WHERE NOT EXISTS (SELECT 1 FROM struct_refs c
                  WHERE c.name=s.sname AND c.context='construct');
-- safebash: returns Coercion, not Interner.
```

---

## 3. Transitive deadness — the view sees only the first layer

`enum_variant_refs` flagged 5 dead variants but **missed 2 more** that
became dead only *after* the first removals:

- `SsaTest::Paren` — its lone `construct` site was inside the
  `TestExpr::Paren` arm; once `TestExpr::Paren` (itself dead) was removed,
  `SsaTest::Paren` had no constructor.
- `BuiltinCmd::SubshellWait` — constructed only inside `lower_subshell_stmt`,
  which was reachable only from the dead `StmtKind::Subshell`.

A human caught both. The view, which only checks *"has ≥1 construct site"*,
counted the transitively-dead construct site as alive.

### Proposed

Either (a) a **fixpoint** pass — after removing zero-construct variants,
re-evaluate variants whose every construct site sits inside a symbol that is
itself now dead, and iterate; or, more cheaply, (b) **document** that the
detector reports one layer and should be **re-run after each removal** (how
I used it — it went to zero after the 5th removal). (a) needs a
construct-site → enclosing-symbol → is-that-symbol-dead linkage, which
also depends on §1 (a sound dead-symbol signal). A nice property to expose:
`enum_variant_refs.symbol` already names the enclosing function of each ref,
so the join target exists.

---

## 4. Pair the report with rustc to cut noise

`rustc`'s own `dead_code` lint already catches **non-pub** dead functions,
fields, and never-constructed variants — safebash builds **warning-clean**,
so there are none of those. The index's unique value is the **pub-reachable**
blind spot (the whole reason `RedirectOp::HereDoc` / `Coercion` survived).
So any dead-symbol report is higher-signal if it **filters to `pub` /
`pub(crate)` symbols** (the `symbols` table already records signatures;
visibility could be added). That focuses attention exactly where rustc is
blind.

---

## Cost

- §1 is an **indexer** change (call-expression target resolution); the
  highest-leverage item — it's the gate to any dead-*function* detection.
- §2 and §3(b) are **views/queries** over existing data, same family as
  `string_literals` / `enum_variant_refs` — cheap.
- §3(a) and §4 want a little more (enclosing-symbol linkage, a visibility
  column).

## Motivating codebase

safebash (`.indexnudge-rust.db`), 2026-06-22/23. `enum_variant_refs` cleared
5 dead variants (+2 transitive caught by hand); the struct query found
`Coercion`; the `calls` dead-function query produced 68 candidates that are
almost entirely live qualified-call false positives.
