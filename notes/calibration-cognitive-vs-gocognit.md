1. # Calibration: Go `symbols.cognitive` vs `gocognit`
2. 
3. **Status: shipped (Go schema v1).** Sibling of
4. `calibration-cognitive-vs-mozilla.md` (the Rust side). Different
5. calibration target, same protocol.
6. 
7. ## Where the reference comes from
8. 
9. [`uudashr/gocognit`](https://github.com/uudashr/gocognit) is the
10. Go-ecosystem standard implementation of the Sonar Cognitive
11. Complexity spec (Campbell 2018). `golangci-lint` ships it as the
12. `gocognit` linter, so it's what most Go teams already enforce.
13. There's no Mozilla `rust-code-analysis`-equivalent for Go — Sonar's
14. own sonar-go analyzer is closed-source — so `gocognit` is the de
15. facto reference.
16. 
17. Install:
18. 
19. ```bash
20. go install github.com/uudashr/gocognit/cmd/gocognit@latest
21. ```
22. 
23. ## Calibration on `go/main.go` (this repo, 1,749 lines)
24. 
25. ```
26. functions in both:  38
27. exact matches:      32/38  (84%)
28. sum gocognit:       673
29. sum indexnudge:     672
30. mean delta:         −0.03   (indexnudge − gocognit)
31. ```
32. 
33. All non-zero deltas are single-digit and rank-preserving:
34. 
35. | function          | gocognit | indexnudge | Δ |
36. |-------------------|---------:|-----------:|--:|
37. | `walkCognitive`   |   40 |   33 | **−7** |
38. | `exprName`        |    9 |    3 | **−6** |
39. | `fullIndex`       |   31 |   35 | **+4** |
40. | `doFreshen`       |  107 |  111 | **+4** |
41. | `extractSymbols`  |   69 |   72 | **+3** |
42. | `main`            |    7 |    8 | **+1** |
43. 
44. The other 32 functions match exactly.
45. 
46. ## Two systematic divergence sources
47. 
48. ### 1. Recursion (−Δ): we don't count, gocognit does
49. 
50. Sonar's spec says each direct recursive call adds +1. `gocognit`
51. implements this; we don't. Mozilla `rust-code-analysis` also
52. doesn't (a TODO comment in `src/metrics/cognitive.rs` explicitly
53. defers it), so the Rust side of indexnudge skips it too — the
54. behaviour is consistent across both indexnudge languages and
55. matches Mozilla, but diverges from gocognit by exactly the
56. recursive-call count.
57. 
58. Demonstrations:
59. 
60. - `walkCognitive` makes 7 direct recursive calls to itself
61.   (inside the IfStmt case for Init, Cond, Body, and the four
62.   else-chain calls). gocognit: +40. indexnudge: 33. **Δ = −7.**
63. - `exprName` makes 6 recursive calls (`StarExpr`, `ArrayType`,
64.   `MapType` ×2, `ChanType`, `Ellipsis`). gocognit: 9. indexnudge:
65.   3. **Δ = −6.**
66. 
67. Adding recursion counting would close every negative delta in
68. our calibration table. The trade-off is choosing whose convention
69. to match — Mozilla (current) or Sonar/gocognit (would diverge
70. from Mozilla and from the Rust column). The two indexnudge
71. languages should remain consistent with each other; if we ever
72. change this, change both at once.
73. 
74. ### 2. `else { if ... }` collapsed to `else if` (+Δ): gocognit treats them the same; we don't
75. 
76. Go programmers write both:
77. 
78. ```go
79. if x > 0 { ... } else if x < 0 { ... }   // AST: IfStmt.Else = IfStmt
80. if x > 0 { ... } else { if x < 0 { ... } }   // AST: IfStmt.Else = BlockStmt{IfStmt}
81. ```
82. 
83. The two compile identically. gocognit treats both as a flat
84. else-if chain (+1 for the else, no extra nesting on the inner
85. if). We follow the AST literally: the inner if inside an
86. `else { ... }` block sees the bumped nesting from the outer if,
87. so it pays +1 more than the chained-`else if` form.
88. 
89. Demonstration:
90. 
91. ```go
92. func elseif(x int) int {
93.     if x > 0 { return 1 } else if x < 0 { return -1 }
94.     return 0
95. }
96. // gocognit: 2,   indexnudge: 2  ✓
97. 
98. func elseBlockWithIf(x int) int {
99.     if x > 0 { return 1 } else { if x < 0 { return -1 } }
100.     return 0
101. }
102. // gocognit: 3,   indexnudge: 4  (+1 — inner if at nesting=1)
103. ```
104. 
105. The +4 deltas on `doFreshen` / `fullIndex` and the +3 on
106. `extractSymbols` all come from this pattern appearing inside the
107. large IIFE closures those functions use.
108. 
109. This one is more clearly a bug-or-feature judgement call. Sonar's
110. spec PDF is silent on which interpretation is correct; both are
111. defensible. The literal-AST reading is what tree-sitter-based
112. tools tend to do (including Mozilla for Rust's `if let Else` /
113. `else { if let ... }` distinction), so the Rust side of
114. indexnudge has the same shape. Keeping the two languages aligned
115. wins over matching gocognit on this point.
116. 
117. ## Reproducing the calibration
118. 
119. ```bash
120. # Build + index
121. cd go && go build -o indexnudge . && rm -f .indexnudge-go.db && ./indexnudge
122. 
123. # Pull both views into TSVs
124. gocognit -json main.go | python3 -c "
125. import json, sys
126. for d in json.load(sys.stdin):
127.     name = d['FuncName'].split('.')[-1]
128.     print(f\"{d['Complexity']}\t{name}\t{d['Pos']['Line']}\")
129. " > /tmp/gocognit.tsv
130. 
131. sqlite3 .indexnudge-go.db "
132.   SELECT cognitive || char(9) || name || char(9) || start_line
133.   FROM symbols
134.   WHERE kind IN ('function','method') AND file='main.go'
135.   ORDER BY start_line;" > /tmp/indexnudge.tsv
136. 
137. # Diff
138. paste <(sort /tmp/gocognit.tsv) <(sort /tmp/indexnudge.tsv) | column -t
139. ```
140. 
141. ## Out of scope (intentional)
142. 
143. - **Implementing recursion counting.** Would require detecting
144.   self-calls during the walk; would diverge from Mozilla
145.   `rust-code-analysis` (which we calibrate Rust against), so
146.   adding it on the Go side alone breaks cross-language parity.
147.   File a separate task if needed.
148. - **Collapsing `else { if }` into `else if`.** Same parity
149.   concern; both languages currently do the literal-AST thing.
150. - **Per-file or per-package totals.** gocognit reports sums; our
151.   schema reports per-function values that downstream queries can
152.   sum if needed.
153. 
154. ## Related
155. 
156. - `calibration-cognitive-vs-mozilla.md` — the Rust side, same
157.   protocol against Mozilla `rust-code-analysis`.
158. - `feature-request-add-cognitive-complexity-column.md` — the
159.   original FR.
160. 