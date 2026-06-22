1. # Calibration: `symbols.cognitive` vs Mozilla `rust-code-analysis`
2. 
3. **Status: shipped (schema v10).** Sibling of
4. `bug-cyclomatic-undercounts-vs-mozilla.md`. Same protocol, different
5. metric: Cognitive Complexity (Campbell / SonarSource 2018) instead of
6. McCabe.
7. 
8. ## Where the algorithm comes from
9. 
10. Mozilla's
11. [`src/metrics/cognitive.rs`](https://github.com/mozilla/rust-code-analysis/blob/main/src/metrics/cognitive.rs)
12. is the only widely-used open-source implementation of the Sonar
13. Cognitive Complexity spec over the same tree-sitter-rust grammar
14. indexnudge uses. Its per-Rust-function `cognitive.sum` is the
15. calibration target, exactly the way `cyclomatic.sum` is for the
16. cyclomatic column.
17. 
18. The Mozilla Rust impl is short:
19. 
20. ```rust
21. impl Cognitive for RustCode {
22.     fn compute(node, stats, nesting_map) {
23.         let (mut nesting, mut depth, mut lambda) = get_nesting_from_map(node, nesting_map);
24.         match node.kind_id().into() {
25.             IfExpression => if !is_else_if(node) { increase_nesting(...) }
26.             ForExpression | WhileExpression | MatchExpression => increase_nesting(...)
27.             Else /* and else-if */ => increment_by_one(stats)
28.             BreakExpression | ContinueExpression => if has Label child { +1 }
29.             UnaryExpression => bool_seq.set(NOT marker)
30.             BinaryExpression => bool_seq evaluates && and || immediate children
31.             FunctionItem => { nesting = 0; depth bumps if nested in another fn }
32.             ClosureExpression => { lambda += 1 }
33.             _ => {}
34.         }
35.         nesting_map.insert(node.id(), (nesting, depth, lambda));
36.     }
37. }
38. ```
39. 
40. Where `increase_nesting` does `structural += nesting + depth + lambda + 1`,
41. then `nesting += 1`, then `bool_seq.reset()`.
42. 
43. Mozilla treats every `FunctionItem` and `ClosureExpression` as a
44. separate function "space" with its own `Stats`. Per-space sums merge
45. into the enclosing space via `Stats::merge`, so the per-function
46. `cognitive.sum` exposed via JSON includes contributions from every
47. closure and nested fn declared inside.
48. 
49. ## What `rust/src/main.rs::count_cognitive_complexity` does
50. 
51. A single recursive walk over the tree-sitter-rust body, threading
52. `(nesting, depth, lambda, prev_bool)` through the recursion:
53. 
54. - `if_expression`: skip if parent is `else_clause` (that's the chain's
55.   inner if, paid for by the `else_clause` +1). Otherwise
56.   `count += nesting + lambda + depth + 1`, reset `prev_bool`, bump
57.   `nesting` for the subtree.
58. - `for_expression` / `while_expression` / `loop_expression` /
59.   `match_expression`: same `+ nesting + lambda + depth + 1`, bump
60.   nesting.
61. - `else_clause`: `count += 1`, no nesting bump, no `prev_bool` reset.
62. - `break_expression` / `continue_expression`: `+1` iff a `loop_label`
63.   child exists.
64. - `binary_expression`: if operator is `&&` or `||`, follow the Sonar
65.   BoolSequence rule — `+1` when the sequence starts or when the
66.   operator kind switches (`&&` → `||` or vice versa); `+0` when the
67.   same operator continues.
68. - `unary_expression`: set `prev_bool = NOT` so the next boolean
69.   operator counts as a switch (matches Mozilla's behavior where any
70.   `UnaryExpression` resets the marker).
71. - `closure_expression`: `lambda += 1` inside the body; save/restore
72.   `lambda` and `prev_bool` around the recursion so outer state isn't
73.   leaked.
74. - `function_item` (nested): reset `nesting = 0`, bump `depth`,
75.   save/restore all state around the recursion. The nested fn's
76.   contributions accumulate into the enclosing fn's cognitive (matching
77.   Mozilla's space-merge behavior) AND the nested fn gets its own
78.   `symbols` row with cognitive computed standalone (`nesting = 0`,
79.   `depth = 0`, no closure ancestors). This last bit slightly diverges
80.   from Mozilla for inner fn_items inside outer fn_items, where Mozilla
81.   would report the inner fn at `depth = 1` — but item-level nested fns
82.   are rare in Rust, and the divergence is a per-fn +1/+2 at most.
83. 
84. ## What we explicitly don't count
85. 
86. Mirroring Mozilla, NOT counted as cognitive contributions:
87. 
88. - `try_expression` (`?` operator) — cyclomatic charges it, cognitive
89.   doesn't.
90. - early `return` — cyclomatic doesn't charge it either, but
91.   downstream cyclomatic-like metrics sometimes do; cognitive
92.   explicitly doesn't.
93. - `match_arm`s — only the `match_expression` itself charges +1+nesting;
94.   arms contribute via whatever decisions live inside their bodies.
95. - `|`-alternated patterns in match arms — one decision regardless of
96.   alternative count.
97. - match arm guards (`pat if cond => ...`) — Mozilla doesn't visit
98.   them as `IfExpression` nodes.
99. - `matches!` macro — opaque `macro_invocation`, not expanded.
100. - `let-else` — no separate node kind handler in Mozilla.
101. 
102. ## Validation
103. 
104. Eight unit tests in `cognitive_tests` cover the FR's stated behavioral
105. cases:
106. 
107. - 20-arm `match` with single-call arms → cognitive **1** (the
108.   "dispatcher discount" that was the original motivation).
109. - 4-level nested `if` → cognitive **10** (+1+2+3+4).
110. - 6-case `if`/`else if` ladder → cognitive **6** (no nesting bonus).
111. - `if` inside closure inside fn → cognitive **3** (closure adds +1
112.   nesting level; if = +2, else = +1).
113. - Outer `match` with one nested `match` arm body → cognitive **3**
114.   (outer +1, inner +2 = 1 base + 1 nesting).
115. - Function with `?` and early `return` and one `if` → cognitive **1**
116.   (only the `if` contributes; `?` and `return` add 0).
117. - Verbatim Mozilla `rust_1_level_nesting` fixture → cognitive
118.   **11** — calibration anchor against the upstream test.
119. - Struct/enum rows → cognitive **NULL**.
120. 
121. ## Spot-checking against safebash (and others)
122. 
123. Reproduce the same protocol as the cyclomatic calibration in
124. `bug-cyclomatic-undercounts-vs-mozilla.md`:
125. 
126. ```bash
127. cd ~/src/safebash
128. # Mozilla cognitive
129. rust-code-analysis-cli -m -p sash/src -O json \
130.   | jq '..|objects|select(.metrics?)|{name, file: input_filename,
131.         line: .start_line, cognitive: .metrics.cognitive.sum}' \
132.   | jq -s '.|sort_by(-.cognitive)|.[:20]'
133. 
134. # indexnudge cognitive
135. sqlite3 .indexnudge-rust.db "
136.   SELECT name, file, start_line, cyclomatic, cognitive
137.   FROM symbols
138.   WHERE kind IN ('function','method')
139.     AND file LIKE 'sash/src/%'
140.     AND cognitive IS NOT NULL
141.   ORDER BY cognitive DESC LIMIT 20;"
142. ```
143. 
144. Expected: same ranking as Mozilla, matching numbers on most
145. functions, residual divergence on the same kind of constructs that
146. drove cyclomatic's `lower_for`/`lower_case` residual (`match`
147. variants the new grammar unified) plus the inner-fn-item depth
148. divergence noted above.
149. 
150. ## Schema
151. 
152. Schema version bumped 9 → 10:
153. 
154. ```sql
155. ALTER TABLE symbols ADD COLUMN cognitive INTEGER;
156. CREATE INDEX idx_symbols_cognitive ON symbols(cognitive);
157. ```
158. 
159. `cognitive` is NULL on exactly the same rows where `cyclomatic` is
160. NULL (struct / enum / trait / impl / const / static / type). Auto-
161. rebuild on next `--freshen` follows the existing
162. `schema_is_current()` path.
163. 
164. ## Downstream
165. 
166. The point of having both columns: divergence is itself a signal.
167. 
168. - High CC + low Cognitive: dispatcher pattern (the `match` over enum
169.   variants the FR is targeting). Safe; the `cyclo-allow` culture
170.   downstream can stop curating these by hand.
171. - Low CC + high Cognitive: deep nesting hidden by few decisions per
172.   construct. Rare, but a real refactor signal. (Spotted in
173.   indexnudge's own `walk_cognitive` at 22/29 — the function that
174.   computes cognitive is itself deeply nested. Self-flagged.)
175. - High both: real refactor target.
176. 
177. The built-in complexity-hotspots snippet in
178. `AGENTS.md.rust.snippet` should grow a `cognitive` column alongside
179. `cyclomatic` in a follow-up; same change applies to the safebash
180. pre-commit report.
181. 
182. ## Go indexer
183. 
184. Not yet implemented. Tracked in
185. `notes/tasks/go-cognitive-complexity.md`.
186. 