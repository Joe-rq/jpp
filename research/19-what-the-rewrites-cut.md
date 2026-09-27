# 19. What is the sixty percent that the rewrites cut?

2026-09-27. Task origin: a cross-check assigned by the coordinating session — `research/18-repetition-in-real-jev-projects.zh-CN.md` used a 12-category busywork keyword method to measure a median busywork-line share of 11.1% in the judgment core (scope=core, v2 block-level scope), while the public rewrite-study report ( https://jpp.towow.net/rewrite-study/ , the public report page comparing field-by-field results for 84 real JEV-project rewrites) used a completely different method (counting total lines before and after the rewrite) to measure a median fold-normalized ratio of 2.5x for the judgment core — meaning the J++ version, on average, uses only about 40% of the original judgment core's lines, with roughly 60% cut away. The two numbers differ by more than fivefold, and document 18 itself admits "this material has no way to separate the two possibilities." This document reads the cut lines directly, to answer: what is that roughly sixty percent of lines actually made of, and which J++ construct or which thing the runtime took over replaced it; and whether the answer to "what's being repeated" is busywork or a way of composing judgments.

Method and judgment criteria: `research/地基/00-定位与方法论-v1.md`, `research/地基/00-目标与动机-v1.md`, `research/地基/00-Nature意图汇编.md` (item 22 requires "give examples, not abstractions"). The definition of "judgment core," the scope of `measure.toml`, and the fold-normalization algorithm are given in https://jpp.towow.net/rewrite-study/ (the public report page comparing field-by-field results for 84 real JEV-project rewrites). The definitions and detection rules for the 12 busywork categories, and the method behind the v1/v2 classification scripts, are given in section 2 of `research/18-repetition-in-real-jev-projects.zh-CN.md`. The list of J++ constructs (`judge`/`cut`/`sieve`/`pair`/`tally`/`first_k`/`iterate`/`order`/`fit`/`walk`/`search`/`verify`/`ground`/`judged_graph`/`compose`/`gen`/`do`, `budget`, the ledger and replay) is given in `rust/GUIDE.md`, in the "built-in function list" and "pairings: element → composition → nesting" sections.

Material: the original source code of 84 projects (all corresponding repositories are public open-source projects, referenced by number) and their block-by-block comparison against the J++ rewrite — this is a private-workspace internal rewrite corpus (each project has its original code, a file registering the judgment-core line range, the J++ rewrite program, a record of line counts and equivalence results, and notes on project selection and pre-registration), not published with this release; this document's quantitative results come from manual review and script-based statistics over this internal material, and the published data tables are in `research/data/2026-09-27-what-got-cut/`.

---

## 1. Classification scheme: A–E

The original code that got cut (the part of the original judgment core with no direct counterpart text in the J++ version) is sorted into the following five categories (mutually exclusive, first matching category wins; the criterion is "what this piece of code is doing," not a keyword):

- **A, busywork**: the categories from document 18's twelve that concern "how to plug in a single judgment call," not stitching multiple judgments together — category 2, thresholds (a threshold constant plus a comparison); category 3, handling uncertainty (an else branch, a tag, a discard); category 4, retry and failure; category 5, caching; category 6, cost and call counting; category 7, concurrency limits; category 12, calibration and labeling. These map to J++'s `cut` declared line, the `unsure` three-way exit, judgment-absence handling (B32/K-039), the ledger, the `budget` block, and certified lines — see document 18's section 5 mechanism-mapping table.
- **B, composition**: document 18's categories 1 (call loops and batching), 10 (result feedback), 11 (combining multiple judgments), plus any control flow — a loop, an item-by-item filter, pairwise comparison, multi-round iteration, voting, or a cascade — that stitches multiple judgments together, even without a dedicated busywork keyword, as long as the function is "organizing how several judgments are chained." This maps to J++'s `sieve` (item-by-item filtering), `pair` (pairwise matching), `iterate`/`walk`/`search` (multi-round iteration, feeding one round's result into the next), `first_k` (taking the top k), `tally` (vote/aggregate counting), and `order`/`fit` (ranking, fitted combination).
- **C, data reshaping**: document 18's categories 8 (question assembly) and 9 (material trimming), plus code that falls outside those 12 keyword-matched categories but is genuinely "assembling material into a question payload, parsing the judge's return value, moving fields around" — building a request body, pulling fields out of a response object, flattening nested structures into parameters for the judgment interface.
- **D, pure syntactic difference**: code with a one-to-one functional counterpart, but where the host language (Python/JS/TS/Go/Rust, etc.) is simply more verbose than J++ — class definitions, `import`, type annotations, `@dataclass`, logger setup, docstrings, explicit `return`/variable declarations, CLI argument parsing (if it falls within the registered core line range). Deleting this code does not change the judgment logic itself; it's only there because the host language requires it.
- **E, present in the original but not in J++**: functionality that was dropped or is non-equivalent — places `result.json`/`rerun.json`/an internal note on the topic mark "partially equivalent," or places where reading the code shows the J++ version genuinely has no implementation of the original project's functionality.

Residual: the part of the original code that maps onto the core semantic content of the J++ judgment statement itself (the direct source of `criteria`/`instructions` text, state-field definitions, etc.) — this part **was not cut**, it was just re-expressed in different syntax; it is not counted in the A–E numerator, and serves only as a denominator reference.

## 2. Predictions (written before the numbers)

A bet on how the cut lines distribute across A–E (the numbers are guesses; section 3, "The numbers," below reports the actual measurements — the predictions are not adjusted after the fact to fit them):

| Category | Predicted share | Rationale |
|---|---|---|
| A, busywork | 12% | Document 18 already measured this: the 12-category busywork keyword method gives a scope=core median of only 11.1% — that is the share of "busywork out of all core lines," not "busywork's share of the cut lines," but most busywork (thresholds, uncertainty handling, retry, caching, cost, concurrency, calibration) should be exactly the kind of thing that gets cut, with only a small residue of busywork remaining in the J++ version (e.g. a threshold constant becoming a parameter on `cut`'s declared line — still a literal number left behind). The prediction is that A's share of cut lines is close to document 18's measured total busywork share, since busywork lines are almost entirely replaced. |
| B, composition | 20% | Document 18's section 5 mechanism-mapping table lists constructs like sieve/pair/iterate/search/tally/order/fit as corresponding to three busywork categories — "call loops," "result feedback," "combining multiple judgments" — whose occurrence rates in document 18 are 75.3%, 22.2%, and 6.2% respectively. High occurrence, but many projects only have a simple one- or two-line for loop, not complex multi-round orchestration — so the guess is a middling, not the largest, share of lines. |
| C, data reshaping | 30% | Guessed to be the category most underestimated: document 18's 12-category scheme puts "question assembly" and "material trimming" at only 50.6%/54.3% occurrence, but string concatenation, f-strings, and pulling fields off a response object are the kind of code that happens on nearly every line, and are hard to pin precisely with a keyword regex (an f-string with three nested layers of formatting, a chain of `.get()` calls — hard for a fixed regex to catch in full); predicted to be the category document 18 systematically under-detects the most, and one of the main sources of the gap between 11.1% and 60%. |
| D, pure syntactic difference | 30% | Guessed as the second-largest source: Python's `class`/`@dataclass`/type annotations/`import`/logger setup, as long as it falls within the core line range registered in `measure.toml`, gets counted in "core line count," but has nothing to do with "judgment logic" — J++ has no class system, no import statements, no logging configuration, so this kind of line is 100% cut and would never be caught by document 18's 12-category busywork regex at all (that regex is built around "how to plug into JEV," and doesn't recognize a class definition). |
| E, functionality gaps | 8% | The rewrite report's own tally: 72/84 (85.7%) fully equivalent, 11 partially consistent, most gaps small (an individual branch, an individual field) — predicted to be a small but non-zero share; known gaps like B155/B156 (cross-candidate splitting) and K-199/K-200 (fission, the scheduling pass — not built) would show up here. |

The key call in this prediction: C and D together are about 60%, and are the main destination of the gap — this is also the direction the task brief itself hinted at ("is the gap mainly in B and C?"). This document's prediction on B is more conservative than the brief seemed to imply (20% rather than higher), putting more share into D instead — on the intuition that the redundancy inherent to Python/JS's type system and object-oriented syntax is more common, and more evenly spread across nearly every project, than "multi-round orchestration code." Only after this prediction was written did the code-reading classification begin; section 3, "The numbers," below is updated from the actual measurements and this table is not revised after the fact.

---

## 3. A method correction made along the way: counting by "net lines cut," not by the whole original block

Early in the manual review, a problem the pre-registration hadn't anticipated came up: if the A–E classification is scored only by "how many lines this block of original code has," it will systematically get the direction wrong — because many blocks of original code also need a few lines in J++ to replace them, sometimes more lines than the original (for example, J-05's "an undecided result must be consumed" requires every judgment to explicitly exhaust the act/ignore/unsure branches; a one-line Python `if p < 0.5: return "review"` often has to expand into several lines of `handle(cut(...), {...})` in J++). Counting only the original line count conflates "what this block of original code is about" with "how many lines this block of original code actually saved."

Corrected scope: **net lines cut = original line count − corresponding J++ line count**. Each block of original code gets two numbers — the original line count, and the corresponding J++ line count (0 if there is no counterpart) — and the A–E classification only tallies the net difference. This introduces two new sub-categories, counted separately and not folded into A–E:

- **Moved elsewhere**: a block of original content (usually question text, a candidate list, or a threshold value) cannot be found in `prog.jpp`, but is confirmed to have moved to another file — fixture data, a host script like `adapter.py`/`build_jpp_input.py`, a real-run `--profile` — the content still exists, it just lives somewhere else now; this doesn't count as replaced by a language construct, and doesn't count as genuinely cut either.
- **New (J++-only)**: content present in `prog.jpp` with no counterpart anywhere in the original code — a `budget{}` declaration, J-05's `unsure`/`pending` scaffolding, the extra boilerplate from splitting into multiple independent `judge()` calls because the candidate set differs. This is overhead the language/runtime itself requires, and it explains why "net lines cut" is often smaller than the surface-level line count that would be attributed to A–E from the original code alone.

Classifying exit dispatch (an if/elif branching on a judgment result) follows one consistent rule: dispatching on the result of **one** judgment counts as **A** (mapping to `cut`/`handle`'s three-way exit); dispatching on a combination of **two or more** judgment results counts as **B** (mapping to combining/cascading multiple judgments).

This correction was added after three manual-review sub-agents had already started work (the supplementary instructions are the two paragraphs above; the actual results are documented in the method notes at the start of each of the three `research/data/2026-09-27-what-got-cut/manual-*.md` files); the low-ratio group, the mid-ratio group, and B4-06 (the first item in the high-ratio group) fully adopted the net-lines-cut scope. The other four items in the high-ratio group (B5-10, B4-12, B4-01, B4-09), constrained by time, still counted by "the original code's raw line range," without deducting the corresponding J++ lines block by block — the sub-agents for these four noted this scope difference themselves in the text; this document does not paper over it when aggregating, calls it out specifically below the summary table, and does not add these four items' numbers directly into the net-lines-cut items as if they were the same thing.

## 4. Fifteen projects, manual comparison

Per the pre-registration, five projects each were chosen at the lowest, median, and highest fold-normalized ratios (covering both ends and the middle of the distribution among the 77 of 84 projects that could be classified), with block-by-block manual reading of the original code, reading of `prog.jpp`, and cross-checking against `result.json`/`rerun.json`/the internal selection notes/the internal pre-registration. The complete block-by-block tables are at:

- Low-ratio group (0.68–1.03x): `research/data/2026-09-27-what-got-cut/manual-低倍率组.md` (B5-11b, B1-07, B5-06b, Q-04, B5-02)
- Mid-ratio group (2.64–2.96x, near the median of the 84-project distribution): `research/data/2026-09-27-what-got-cut/manual-中倍率组.md` (B6-03, Q-07, B3-08, B6-06, B3-09)
- High-ratio group (7.44–10.95x): `research/data/2026-09-27-what-got-cut/manual-高倍率组.md` (B4-06, B5-10, B4-12, B4-01, B4-09)

### 4.1 Net lines cut, totaled per group

| Group | A busywork | B composition | C reshaping | D syntax | E gaps | Net cut total | Scope |
|---|---|---|---|---|---|---|---|
| Low ratio (5 projects) | 13 | **−23** | 57 | 69 | 39 | 155 | Net cut (original − J++) |
| Mid ratio (5 projects) | 51 | 2 | 123 | 64 | 46 | 286 | Net cut (original − J++) |
| High ratio (5 projects, mixed scope) | 409 | 80 | 457 | 309 | 220 | 1475 | B4-06 net cut; the other 4, original code's raw line count (J++ lines not deducted) |
| **15-project total** | **473** | **59** | **637** | **442** | **305** | **1916** | Mixed scope, see the note below |

Share (15-project total, mixed scope, see the dedicated scope warning below):

| Category | Total net lines cut | Share |
|---|---|---|
| A, busywork | 473 | 24.7% |
| B, composition | 59 | 3.1% |
| C, data reshaping | 637 | 33.2% |
| D, syntactic difference | 442 | 23.1% |
| E, functionality gaps | 305 | 15.9% |

**Scope warning (do not skip past this and quote the table above directly)**: only 1 of the 5 items in the high-ratio group (B4-06) used the net-lines-cut scope; the other 4 (B5-10, B4-12, B4-01, B4-09) used "the original code's raw line range," without deducting the corresponding J++ lines block by block — this systematically biases these 4 items, and by extension the 15-project total table, toward "looks like more was cut" (because many blocks, though assigned a category, actually still need a few lines written on the J++ side, just not subtracted). The three groups looked at separately are more trustworthy than this combined table: the **low-ratio and mid-ratio groups (10 projects, full net-lines-cut scope)** are this document's hardest data, and the conclusions below rely mainly on them; the high-ratio group's numbers serve as a directional reference, not a basis for a precise share.

### 4.2 The low-ratio and mid-ratio groups (10 projects, full net-lines-cut scope) on their own

| Category | Total net lines cut | Share |
|---|---|---|
| A, busywork | 64 | 14.5% |
| B, composition | −21 | −4.8% (negative) |
| C, data reshaping | 180 | 40.8% |
| D, syntactic difference | 133 | 30.2% |
| E, functionality gaps | 85 | 19.3% |

(Total net cut: 441; the A+B+C+D+E denominator here is still handled by "each category's value, including negative ones, added into the numerator as-is" — consistent with the arithmetic in the two group subtotal tables above — i.e., 64−21+180+133+85=441.)

The single most important thing to remember from this table: **the net value for B (composition) is negative**. In these 10 projects, wherever the original code used a compact, built-in feature of the host language to handle "how to stitch multiple judgments together" — Python's `max(key=(a,b,c))` tuple comparison, TypeScript's `match`/chained `??`, a batch for-loop hand-building a dictionary — expanding that into J++'s `fit` closures, `order()`, nested `handle(cut(...))`, or `map`+`filter`+`fold` actually ends up longer than the original. Broken down per project: only 1 contributes a positive value (B6-06's hand-written batching, +3); 4 are negative (B1-07 −14, Q-04 −6, B5-02 −3, Q-07 −1); the remaining 5 are 0 (B5-11b, B5-06b, B6-03, B3-08, B3-09).

### 4.2a From net-lines-cut to core − jpp: a complete accounting

The A–E net-lines-cut figures above are only the "cut" side, not the whole of `core_lines − jpp_lines` — the complete accounting also has to add "net retained" (the same piece of verdict text getting longer or shorter net of switching containers) and "moved elsewhere" (content that still exists, just in a different file), and subtract "new" (J++-only language/runtime scaffolding):

| Group | A–E net cut | Net retained | Moved elsewhere | New | Estimated core−jpp | Measured core−jpp (Σcore−Σjpp) |
|---|---|---|---|---|---|---|
| Low ratio (5 projects) | 155 | −67 | 0 | 80 | 155−67+0−80=**8** | 362−389=**−27** |
| Mid ratio (5 projects) | 286 | 63 | 17 | ≈36 (B6-03: 2 + Q-07: 18 + B3-08: 11 + B6-06: 5 + B3-09: 0; B3-09 additionally has 26 lines of "code outside any registered core range" not counted in any item, honestly noted here as a scope gap) | 286+63+17−36=**330** | 519−212=**307** |
| 10-project total | 441 | −4 | 17 | ≈116 | 8+330=**338** (includes B3-09's 26-line scope gap; the two figures disagree slightly) | −27+307=**280** |

The gap between the two columns (estimated vs. measured) is 35 lines in the low-ratio group and 23 lines in the mid-ratio group — both within the "per-item estimate rounding/residual" range each project's own section notes; this isn't a calculation error being hidden, it's the necessary noise of a sub-agent using estimates under time pressure rather than a compiler-level exact reconciliation — and the two columns cross-confirm each other at the order of magnitude (same magnitude, same sign).

**The low-ratio group's "net cut" looks like 155 lines on the surface, but is almost entirely canceled out by "new language scaffolding" (80) and "the retained part itself also got longer"** (net retained −67; a negative number means expansion — to be unambiguous, since the word "expansion" alone could be read either way: −67 means this verdict text is longer in J++ than in the original) — both the estimated and measured figures land in the single digits to tens of lines. This is the exact arithmetic reason these 5 projects' fold-normalized ratios are stuck around 0.7–1.0 and can't be pushed down further.

**The mid-ratio group is the most suitable one for answering "what exactly is the sixty percent net-cut in a typical project"** (sorted by ratio among the 84 projects, this group's 2.64–2.96x happens to sit right near the median — not cherry-picked to make the numbers work). Its complete accounting is: net cut total 286 (C 123 + D 64 + A 51 + E 46 + B 2) + net retained 63 (the question/criteria text itself was also compressed, not just the control flow — e.g. Q-07's QUESTIONS bank went from 84 lines down to 37) + moved elsewhere 17 (state-projection logic carried over verbatim, now executed in `adapter.py`) − new, about 36 (budget declarations, J-05 scaffolding) = **core−jpp of about 330 (estimated) / 307 (measured)**, which is **59.2%** of this group's core-lines total (519). This 59.2% should not be read as "J++ can inherently cut a typical project by six-tenths" — it is a direct consequence of "having chosen 5 projects near the median of the 84-project ratio distribution, whose fold-normalized ratio (2.64–2.96) already corresponds to about a 62–66% reduction" — to some extent this is circular. Its value isn't in "proving a six-tenths cut is possible," it's in **answering what that sixty percent is actually made of**: C (question assembly + field-moving, 123 lines) and net-retained (the criteria text itself being compressed, 63 lines) together account for most of that six-tenths — not D (syntactic difference, 64 lines), and not the busywork (A, 51 lines) that document 18 emphasizes. As a cross-check, recomputing with the official line counts from the rewrite-data CSV: Q-07's official core line count is 145 (not the 173 raw lines used in this table), and B6-06's official jpp line count is 20 (excluding the 20 lines of data constants this table's 40 includes) — under that scope, the mid-ratio group comes out to 318/491 = 64.8%, the same order of magnitude as 59.2% but not an identical number — both numbers are stated here, without hiding whichever one is more favorable.

### 4.2b Category A by sub-category, category B by construct, with medians

The task brief asks for "A broken down by sub-category; B by construct," and for "both the median and the total" to be given. The sub-categories/constructs noted in the remarks column of the 10-project tables have been re-tallied after checking each net value line by line (this corrects two miscalculations from an earlier draft: B3-08's threshold line in physics.ts had an original line count of 5 and a J++ line count also of 5, for a net value of 0, not the previously mis-stated +5; B3-08's retry sub-category net value is the sum of two lines, `judge.ts:39-40` and `judge.ts:41-53`, totaling 2+6=8, not the previously mis-cited raw line count of 13):

| A's sub-categories (following document 18's 12 categories) | Per-item net value | Character |
|---|---|---|
| 4, Retry and failure | B5-06b +17 (timeout/cancellation infrastructure, the only full positive contribution), B3-08 +8 (judge.ts response-completeness validation, two lines combined), B6-03 +1 (a try wrapping the judgment call), Q-04 +4 (a try/except wrapping response parsing) | The one busywork sub-category in this set where "what should be cut really was cut cleanly" — all 4 items are positive |
| 2, Thresholds | B1-07 −5, B6-03 −3 (two threshold lines combined: route.ts:81's −5 + lib.ts:16-23's +2), B3-08 0 (physics.ts, 5→5), B6-06 −2, Q-07 +5 (constant definition +6, comparison check −1, two lines combined), B5-02 +2, B5-06b −5 | 4 of 7 items have a negative or zero net value — the cost of J-05's explicit three-way exit often outweighs the one line of threshold comparison it saves; positive and negative split roughly evenly, so it can't be said flatly that "this category can never be cut" or "always can be" |
| 3, Handling uncertainty | No clear positive-contribution case appeared on its own among the 10 projects | — |
| 6, Cost and call counting | B3-08 (judge.ts:55-59, 64-67, two cost/metadata lines, net value +9, no corresponding J++ line) | Taken over automatically by the ledger/budget, but hand-written cost tracking was already rare in this batch of projects |
| 5/7/12 (caching/concurrency/calibration) | None appeared in any of the 10 projects | Consistent with document 18's finding that these three categories already have the lowest occurrence rates (12.3%/11.1%/9.9%) |

| B's constructs | Project it appears in | Net value |
|---|---|---|
| Hand-written batching → `judge()` auto-batches by material hash | B6-06 (mood.js's single `postToTypeSafe` call) | Net cut +2 (the only positive B contribution in this batch) |
| Two-hop result feedback → explicit rewrite with `map`/`filter`/`fold` | B1-07 (two-hop triage, −14), B5-02 (intent decides the app branch, −3) | Both negative |
| Implicit tuple/lexicographic comparison → `fit` closures + `order()` (**correction**: this is a form of combining multiple judgments, not result feedback — an earlier draft misclassified this; corrected here) | Q-04 (Python's `max(key=tuple)` → two `fit` closures plus `order`, taking the lexicographic max across "hits a wall," "hits itself," and "gets closer to food," −6) | Negative |
| Combining multiple judgments (two judgments combined to decide a label) | Q-07 (`mismatch = choice if confidence clears the gate else None`, −1) | Net cut −1 (exit dispatch on a combination of two judgments, barely changes line count) |

**A/B net-value medians (10 projects, absolute line counts, not percentages — for projects with a small total net cut, a percentage would distort things; e.g. B1-07's whole-project net cut totals only 10 lines, and category C alone accounts for 130% of it; the median is more stable using absolute line counts)**: A's median is 3.5 lines (total 51+13=64, the figure from section 4.1); B's median is 0 lines (total −21; 6 of 10 projects are 0, 4 are negative, not one item's net contribution exceeds +3); C's median is 12.5 lines (total 180); D's median is 14.5 lines (total 133); E's median is 6 lines (total 85). The medians and the totals say the same thing: C and D are the most stable sources of positive contribution (not one project has a negative C or D net value among the 10), A and E swing widely (some projects at 0 or negative, some at twenty or thirty lines), and B's median of 0 directly confirms that "composition contributes essentially nothing to net compression for a typical project" is not being skewed by a handful of extreme outliers.

None of the heavier composition constructs — `sieve`/`pair`/`search`/`walk`/`judged_graph` — appear across the 10 projects; this batch's judgment-orchestration complexity simply isn't high enough to need them. The high-ratio group's B4-12 (`order`+`fold`+`slice` replacing sort-and-truncate) and B4-01 (intent → motor result feedback, but sidestepped by "two independent rounds of questions" rather than genuinely implemented) are the only two projects in this investigation showing any sign of a heavier composition construct — but neither is in the 10 net-lines-cut-scope projects, and their numbers cannot be folded directly into the table above.

### 4.3 The high-ratio group (5 projects, mixed scope, directional reference only)

| Category | Total lines | Share (of A+B+C+D+E) |
|---|---|---|
| A, busywork | 409 | 27.7% |
| B, composition | 80 | 5.4% |
| C, data reshaping | 457 | 31.0% |
| D, syntactic difference | 309 | 21.0% |
| E, functionality gaps | 220 | 14.9% |

This batch of projects (routing, retrieval filtering, a robot arm, a snake game, command suggestions) has the highest compression ratios, driven mainly by two categories: **C (question assembly + peeling fields out of nested response objects, 31.0%) and A (busywork, 27.7% — this batch generally has long stretches of hand-written HTTP retry, response-structure validation, concurrency scheduling, and confidence-gate-plus-fallback logic, making it the batch with the most solid A-category share in this whole investigation)**. B still accounts for only 5.4% — these projects are basically "one or a few independent judgments plus complex question assembly," not "a chain of judgments driving a decision," confirming document 18's own finding that "combining multiple judgments" and "concurrency limits" are the two rarest busywork categories among the 84 projects. E (14.9%) is concentrated in two projects: B5-10's 95-line multi-tick autoplay loop (outside the scope this project's selection was meant to cover in the first place, not something the language can't do) and B4-09's skip-gating and effort-clamp table (untested by this rewrite, or moved into the host glue code).

**Part of this batch's high ratios is compression disguised as "moved elsewhere," and that has to be called out on its own.** B4-01's "retained" category has 138 lines (over a third of its own A–E-plus-retained total) — this is natural-language rule text for the judgment questions themselves — `RULES`/`axis_criteria`/`INTENTS`/`MOTOR_RULES` — which, by this document's own section-3 definition, is neither in `prog.jpp` nor replaced by any construct, but was pulled verbatim from a call log by `adapter.py` and moved into `jpp_input.json`. **By this document's own rule, this should be recorded as "moved elsewhere," not "retained"** — the high-ratio group's sub-agent recorded it as "retained" (reasoning that "the content has a direct counterpart," but the counterpart lives in a data file, not in the program, which doesn't hold up). This is a known classification error in this document that was not corrected in time, and it is stated honestly here: if B4-01's fold-normalized ratio (9.21) were recalculated under the rule "moved elsewhere doesn't count as compression," it would come out more conservative than the surface number — part of this batch's high-ratio projects' "ratio" itself comes from moving the judgment question text out of source-code strings and into a data file, not entirely from the credit of a language construct.

### 4.4 A counterintuitive but solid finding: category B's net value is near zero, even negative

Across the 15 projects, B (composition) totals only 3.1% (mixed scope) or −4.8% (10-project, pure net-lines-cut scope). This directly overturns the pre-registration's guess that "B should be about 20%," and doesn't match the impression given by document 18's section-5 mechanism table, which maps constructs like sieve/pair/iterate/search onto call loops, result feedback, and combining multiple judgments — not because these constructs don't exist or don't work, but because **they require explicitly exhausting the three-way exit (act/ignore/unsure), and require turning an implicit tuple comparison or a chained conditional into a declarative `fit`/`order` call — this cost of "making things explicit" often cancels out the loop/dictionary-building code it saves, pushing the net value toward zero or negative**. The place where a B-category construct genuinely shows its power and clearly saves lines is where the original code already had to hand-write complex orchestration — B4-12's thread-pool/semaphore concurrency scheduling (22 lines → 1 line of `map`) is the cleanest example of category B found in this investigation; B4-01's genuine result feedback ("the intent judgment's result decides the motor judgment's candidate set") is, in the J++ version, actually sidestepped with two independent rounds of questions rather than genuinely implemented — because the language doesn't yet have a way to compress "the previous round's result rewrites the next round's candidate set" into a single statement (gap B155/B156).

## 5. The remaining 62 projects: script-based rule inference, explicitly downgraded to a lower-bound reference, not extrapolated to an A–E share

For the remaining 62 projects (84 minus the 15 manually reviewed, minus 7 with no source copy or ruled not comparable), `research/scripts/cut_lines_classify.py` was written, reusing the block-level attribution and context-proximity logic from `busywork_classify.py` to re-bucket the 12 busywork categories into A/B/C, then running a set of "syntactic boilerplate" regexes over the residual lines to identify D. This is **rule-inferred, not manually reviewed**, and the script has one fatal limitation that has to be stated up front, before any numbers:

**The script can only recognize lines that "look like" busywork/composition/data-reshaping/syntactic-boilerplate by surface features — it cannot tell "whether this content has any counterpart in J++ at all." It cannot give a net-lines-cut number, only "what category this line of text resembles," and a large amount of content that really was cut — generic if/elif business routing, chains of variable assignment, moving fields around — has no dedicated keyword at all, so the script will mis-classify it as "retained."**

This is verified using the 62 projects' own line-count data: the "actual share cut," computed as `(core_lines − jpp_lines) / core_lines`, has a median of **64.2%** across the 62 projects; the share of core lines the script can directly bucket into A/B/C/D has a median of only **13.6%** — the script sees only **about a fifth to a quarter** of the lines actually cut, a median gap of 45.3 percentage points. The aggregate scope agrees: the script identified 2,276 "suspected cut" lines across the 62 projects in total, 22.7% of the total core-line count (10,045) — far below the true compression measured by the line-count ratio.

This huge gap is itself part of the answer to the question this document is trying to answer: **the "busywork + composition + data-reshaping + syntactic boilerplate" that keyword/syntax regexes can catch is only a small part of the content that gets cut; the bulk of it has no dedicated vocabulary at all and can only be confirmed by understanding context — what this piece of code is actually trying to do in business terms, and which lines of J++ it corresponds to** — this is exactly why the 15-project manual review took so much time reading code block by block, rather than just running a regex once and getting a number.

For this reason, this document **does not use this script to compute an A–E share for the 62 projects**, and only uses its output for two things:

1. **A lower-bound reference**: the 2,276 lines the script identified in total, broken down by category (A 32.2%, B 43.2%, C 3.1%, D 21.5%, aggregate scope), represent the composition of "the part that's most conservative and easiest to recognize with keywords" — within this, A (busywork, including retry/caching/thresholds/cost counting) and B (call loops etc., mainly for-loops and batching with a clear syntactic marker like `Promise.all`) are easy to recognize, while C (question assembly, much of it three layers of nested f-strings or chained `.get()` calls, with no stable regex signature) is almost impossible to recognize (only 70 lines total across the 62 projects, 3.1%) — this runs in the opposite direction from the 15-project manual review's finding that category C has the largest share (33–41%), which is exactly what confirms "category C is the one the keyword method misses most systematically" — this is one prediction in this document that turned out to be right.
2. **Checking the boundary of document 18's own method**: this script's pattern of missed detections (category A relatively easy to recognize, category C almost impossible) is the same kind of problem document 18 itself found in its own calibration check in section 3.3 (missed multi-variable-unpacking for-loops, the structural pattern `abs(p-0.5)` going undetected, an implicit combination expressed via `reduce` going unrecognized) — keyword/regex methods have a stable ceiling that doesn't go away just by improving the word list.

Data files: `research/data/2026-09-27-what-got-cut/规则推-62项目.csv` (the 62 projects), `规则推-15个人工项目对照.csv` (the script's output for the 15 manually reviewed projects, for cross-checking in the next section). Script: `research/scripts/cut_lines_classify.py`.

## 6. Reconciling with document 18's 11.1%: where the gap lies

Document 18's 11.1% measures "across the judgment core of 84 projects, the share of lines the 12-category busywork keywords can hit, as a share of total core lines" — using a **raw line count** scope (no J++-corresponding lines deducted). Mapped onto this document's categories, the 12 busywork categories roughly correspond to the union of A (categories 2/3/4/5/6/7/12), B (categories 1/10/11), and C (categories 8/9); D (syntactic difference) and E (functionality gaps) were never covered by that 12-category definition at all — document 18's method never intended to measure these two.

**Comparing on the same batch, the same scope (raw line counts, no J++-corresponding lines deducted), using only the 10 projects under the pure net-lines-cut scope (the low- and mid-ratio groups), without mixing in the high-ratio group's mixed-scope numbers:**

- **Document 18's v2 keyword method** (raw line counts, all 12 categories combined — document 18's native scope): total core lines across the 10 projects, 863 (document 18's script's own recount; this differs by 18 lines from the 881 this document counted by manual reading — a counting-scope difference between the two tools on the same batch of files, not a contradiction); hits 207 lines, a share of **24.0%**.
- **This document's manually reviewed A+B+C, switched to a raw line-count scope**: low-ratio group A=34, B=52, C=91 (total 177); mid-ratio group A=87, B=20, C=138 (total 245); 10-project total A=121, B=72, C=229, A+B+C=**422**, which is **47.9%** of the total core line count (881).

**These two numbers (47.9% vs. 24.0%) cannot simply be subtracted to get "how much the keyword method missed" — because this document's own A/B/C definitions are broader than document 18's 12 categories.** Document 18's category 8 (question assembly) refers only to "concatenating strings/templates to assemble a question"; category 9 (material trimming) refers only to "truncation/chunking." This document's C category additionally includes "pulling fields off the judge's return value, moving fields around" — content like extracting `answer.choice`/`response.probabilities` from a response object and folding it into the return value — none of which falls within any of document 18's twelve category definitions; it isn't that document 18 "failed to recognize it," it's that document 18 never intended to measure it at all. This document's A category is similarly broader than document 18's: it folds "progress callbacks," "timing logs," and "exit dispatch after any single judgment call" into A, while document 18's 12 categories only recognize the 7 specific sub-categories of retry/threshold/uncertainty/caching/cost/concurrency/calibration. Splitting apart, line by line, how much of this document's A/B/C falls inside versus outside document 18's 12 sub-category definitions — to precisely separate "missed detection" from "a broader definition" — would require going back through roughly 140 rows of tables and judging each one individually; that has not been finished this time, and this is a piece of work this document explicitly leaves undone rather than pretending it's already been done. **The responsible thing that can be said is only that "a broader definition and genuine missed detection are mixed together, and this document cannot separate them precisely" — it cannot give a precise percentage-point figure for "how much was missed."**

**And that's only the accounting for A+B+C.** D (syntactic difference) and E (functionality gaps) are two categories document 18's 12-category busywork definition never intended to cover from the start — this part has none of the "definition breadth" ambiguity above; document 18's 12 busywork categories simply do not include "type annotations," "class definitions," or "unimplemented functionality" at all — there is no gray area here. Looking at these two on their own: D's net cut (10-project total) is 133 lines, 15.1% of the total core line count (881); E's net cut is 85 lines, 9.6% of the total core line count (both percentages here use the total core line count as the denominator, not the net-cut total — the conclusions section below uses the same denominator, to avoid confusion).

So, on the task brief's question of whether "the gap is mainly in B and C," the responsible partial answer is: **not in B (B's net value is near zero, even negative — see section 4.4); C really is the largest single piece (the highest share of the 10 projects' net-cut total — see section 4.2), but "C explains the gap better than B" and "the keyword method systematically misses C" are two different claims — this document can only confirm the former; the latter is limited by the "definitions not separated" method limitation above and cannot be precisely quantified**. D (syntactic difference) and E (functionality gaps) together account for about 24.7 percentage points of the total core line count — a part of the gap document 18 never intended to measure by design, and the least contentious part of the gap. What exactly makes up "a typical project's net cut of about sixty percent" is answered in the next section using the mid-ratio group's (the group closest to the median of the 84-project distribution) complete accounting, not by adding up a questionable total here for a single number.

## 7. Seven side-by-side examples

Each example below gives the original code and the J++ code (no more than 15 lines each), with one sentence on what the original was doing and what J++ replaced it with. Full context is in the corresponding sections of `research/data/2026-09-27-what-got-cut/manual-*.md`.

**One qualification stated up front**: the "equivalence" in Examples 1, 2, 3, and 5 was verified under a fixed observation (a fixture replaying a recorded reading table), not by asking the real judge again — `prog.jpp` no longer containing validation/retry/concurrency code shows that "this kind of code doesn't need to be hand-written by the author," which is not the same as "the contract guarantees of `judge()`, the transport-layer retries, and the concurrency scheduling have already been verified on a real run under every scenario." Document 18's section 5 states this plainly: the judgment-absence mechanism (B32/K-039) is "built, with unit tests, but has no dedicated real-run comparison against this batch of open-source projects' 'what to do when a call fails' scenarios" — Examples 2 and 5 below show exactly the scenario this mechanism takes over; readers should not equate "the code no longer needs to be written" with "the runtime behavior has been tested to the same rigor."

### Example 1: the judgment result validates its own compliance, 35 lines → 0 lines (B4-06, Go, client.go:142-176)

```go
p, ok := a.Probabilities[a.Choice]
if !ok {
    return errors.New("choice missing from probabilities")
}
if p != maxP {
    return fmt.Errorf("choice %q (%.2f) is not the argmax (%.2f)", a.Choice, p, maxP)
}
// plus the type/range/non-empty validation above it, the whole function is 35 lines
```

```jpp
let r = judge(st, intended_q);   // no extra code needed
```

One sentence: the original project makes its own raw HTTP call, with nothing to guarantee the other side answers according to protocol, so it has to validate for itself that "the chosen candidate is really in the probability table, and the probability really is the max"; `judge()`'s reading is itself a contractually guaranteed legal structure — this kind of self-validation has no reason to exist at all.

### Example 2: retrying on HTTP 429/500-529, plus retry-after backoff, 36 lines → 0 lines (B4-01, Python, policy.py:273-308)

```python
for attempt in range(self.config.api_retries + 1):
    try:
        response = self.client.post(self.config.api_url, json=exchange["request"], ...)
    except httpx.TransportError:
        if attempt == self.config.api_retries:
            raise PolicyError("...") from None
        self.sleep(min(2**attempt, 5)); continue
    if response.status_code in {429, 500, 502, 503, 504, 529} and attempt < self.config.api_retries:
        delay = float(response.headers.get("retry-after", 2**attempt))
        self.sleep(min(max(delay, .1), 10)); continue
    if response.status_code != 200:
        raise PolicyError(f"...HTTP {response.status_code}...")
    break
```

```jpp
let r = judge(stx, xq);   // sending the request, retrying, and getting the reading all happen in this one line
```

One sentence: network-error retries, retrying 5xx/429 by the `retry-after` header or exponential backoff, and only raising an error once the retry count is exhausted — this entire transport-layer policy is a built-in effect of `judge()`; the program never needs the word "retry" to appear in it at all.

### Example 3: thread-pool/semaphore concurrency scheduling, 22 lines → 1 line (B4-12, Python, strategies.py:161-183)

```python
def score(self, query, documents):
    with ThreadPoolExecutor(max_workers=self.config.max_concurrency) as executor:
        futures = [executor.submit(self._score_one, query, d, key) for d in documents]
        scores = [f.result() for f in futures]
    return scores
# plus an async version with asyncio.Semaphore + asyncio.gather, two nearly duplicate sets of code
```

```jpp
let rows = map(c.docs, fn(d) !{judge} { score_one(c.query, d) });
```

One sentence: the original project manages its own concurrency cap, and has to write it twice — once synchronous, once async; J++'s `map` combined with a `!{judge}` effect marker issues a judgment for each piece of material, leaving whether calls overlap to the runtime — the program never needs to say "thread pool" or "semaphore." **Classification note (stated honestly, without glossing over it)**: in the manual table, this block is classified as **A (concurrency-limit busywork)**, not this document's B (composition) — "whether to run concurrently, and how many at once" is itself busywork, not "stitching multiple judgment results together"; but it genuinely is implemented via the `map` composition primitive, which is an example of busywork and a composition primitive overlapping in how they're written. The original project's own notes acknowledge this is the main reason its fold-normalized ratio came out about 50% higher than the pre-registration predicted.

### Example 4: sorting plus truncating to the top K, 7 lines → `order`+`fold`+`slice` (B4-12, Python, retriever.py:152-158; the only example in this document classified as a positive instance of B)

```python
retained.sort(
    key=lambda document: float(document.metadata.get(self.config.metadata_relevance_key, 0.0)),
    reverse=True,
)
return retained[: self.config.top_k] if self.config.top_k is not None else retained
```

```jpp
let tiers = order(map(hit_rows, fn(row) { row.reading }));
let ordered = fold(tiers, [], fn(acc, tier) { concat(acc, map(tier, fn(i) { hit_rows[i] })) });
let sliced = if c.top_k != unit { slice(ordered, 0, c.top_k) } else { ordered };
```

One sentence: the original project has to write the judgment result back into the document's metadata field first, then sort by that field, then truncate to the top K; `order()` sorts directly on the judgment's raw reading, with no need to write it back into the material and read it out again to compare — the line count here doesn't drop noticeably (J++ is actually a bit longer), but the logic that sorting itself requires — the comparison function, stability, reversing — really is absorbed into `order()`'s built-in behavior. This is the one clear case, among all 15 projects in this investigation, where "this is a B-category construct genuinely doing work" — and it also shows that "doing work" doesn't necessarily mean "saving lines."

### Example 5: a fallback for matching candidates, 30 lines → 0 lines (B4-09, Python, grid.py:123-152)

```python
def resolve(criteria_map, choice, grid):
    if choice is None: return None
    key = str(choice).strip()
    by_position = {str(i): e for i, e in enumerate(grid, start=1)}
    if key in by_position: return by_position[key]
    head = key.split(":", 1)[0].strip()
    if head in by_position: return by_position[head]
    for candidate in (key, head):
        for entry in grid:
            if entry.model_id == candidate: return entry
    for position, text in (criteria_map or {}).items():
        if text == key: return by_position.get(str(position))
    return None
```

```jpp
(no corresponding code on the J++ side — 0 lines; select()'s answer type is structurally a candidate index already)
```

One sentence: the original project calls a generic HTTP endpoint and has to guard against the other side echoing back a string it doesn't recognize — trying four possible forms (a position number, a prefix, the raw `model_id`, the raw candidate description); J++'s `select()` answer is itself an index into the candidate set, so there's no such thing as "the answer doesn't match any candidate" — this isn't compressing 30 lines down to a few, this kind of code simply has no reason to exist in J++ at all.

### Example 6: question assembly, 19 lines → 2 lines (B3-09, JavaScript, actions.js:172-190)

```javascript
questions: { action: { type: "choice",
  instructions: { task: "Choose the next concrete browser operation...",
    rules: ["Choose only from the supplied actions...", /* 8 rules total */] },
  criteria: Object.fromEntries(actions.map((a) => [a.id, a.label])) } }
```

```jpp
let ACTION_TEXT = "Choose the next concrete browser operation... [the 8 rules concatenated into one block of text]";
let action_q = select(ACTION_TEXT, "action-pick");
```

One sentence: the original code rebuilds a structured object on every call; J++ hardcodes this text as a string constant ahead of time and passes it straight to `select` at runtime — the cost is that each candidate's own explanatory text is no longer passed alongside the question to the judgment itself, and only serves as a host-side index table (this is a simplification, not a free equivalent substitution). **A note on the line-count scope**: the "19 lines → 2 lines" here counts raw lines — the 8 rules are packed into one long string squeezed into 2 lines; the rewrite report's "fold-normalization" scope (normalizing every 100 characters into one standard line) would fold this one long line into several standard lines by character count, and the true ratio after fold-normalization would be smaller than the literal "19→2" comparison here — this document does not report only the most flattering scope for the sake of a good-looking example.

### Counterexample: J-05's undecided-consumption discipline makes a one-line threshold check longer, not shorter (B1-07, Python, classifier.py:141)

```python
"needs_review": min(area[1], category[1], tone[1]) < CONFIDENCE_THRESHOLD,
```

```jpp
fn picked(r, argmax_key) {
    handle(cut(r, argmax_key), {
        pick: fn(k) {
            handle(cut(r, {stat: "confidence", declare: {hi: 0.5}}), {
                act: fn() { {k: k, ok: true, exit: unit} },
                ignore: fn() { {k: k, ok: false, exit: unit} },
                unsure: fn(u) { {k: k, ok: false, exit: u} }
            })
        },
        unsure: fn(u) { {k: -1, ok: false, exit: u} }
    })
}
```

One sentence: the original code is a 1-line threshold comparison; because J++ has to explicitly exhaust the pick/act/ignore/unsure exits, the net cut is **−5** (longer, not shorter) — this isn't a one-off; it's the same reason 4 of the 10 net-lines-cut-scope projects in this investigation have a negative B-category net value: the cost of the explicitness the language requires is, sometimes, larger than the lines it saves.

## 8. Conclusions

**The predictions were half right.** (The percentages below are all against a denominator of "10-project net-cut total, 441 lines"; D and E have another set of numbers in section 6 against a denominator of "total core lines, 881" — the two denominators are different, already noted separately in section 6 and section 4.2b; only the former is used here.) The pre-registration bet C+D together at about 60% (30%+30%); the measured C+D together is 71.0% (40.8%+30.2%) — right direction, somewhat underestimated in magnitude, but the same order of magnitude. The prediction for A was 12%, measured at 14.5% — close. The prediction for E was 8%, measured at 19.3% — a clear underestimate: functionality gaps are more common than the pre-registration expected. **The prediction that missed by the widest margin is B**: predicted at 20%, measured at **−4.8% (negative)**. The pre-registration's reasoning was, "document 18 lists a mapping table from busywork categories to J++ constructs; more constructs, higher occurrence, so the line count should follow" — that reasoning wasn't wrong on its own, but it missed one thing: these constructs require explicitly exhausting the three-way exit, and the cost of this "making things explicit" often cancels out the orchestration code it saves, pushing the net value toward zero or negative (the medians in section 4.2b confirm this isn't an illusion created by a few extreme outliers).

**The answer to "what's being repeated" is not busywork, and not composition — it is mainly data reshaping (C) and syntactic difference (D), plus a sizable chunk where the retained part itself was also compressed.** C and D together account for 71.0% of the 10 projects' net-cut total, the bulk of what got cut; A (busywork, 14.5%) can be significantly higher in projects with long stretches of hand-written HTTP infrastructure (up to 27.7% combined across the four high-ratio-group items); B (composition) contributes almost no net compression, with a median of zero. In addition, the mid-ratio group's complete accounting (section 4.2a) shows that "net retained" (the criteria/question text itself being compressed, not the control flow getting shorter) is 63 lines on its own — almost as large as D (64 lines) — and this content doesn't appear in any of the A–E categories at all, because by definition it doesn't count as "cut busywork/composition/data-reshaping/syntax/functionality"; it's "the same judgment intent, expressed in a more compact container" — this too is a real part of what's repeated, and this document cannot pretend it doesn't exist just because it doesn't fit the A–E scheme. This is inconsistent with the impression given by document 18's section 5, which maps the 12 busywork categories onto constructs like sieve/pair/iterate/search — not because that mapping table is wrong (the constructs genuinely exist, and genuinely correspond) but because "a corresponding construct exists" does not mean "using this construct necessarily saves lines."

**Reconciling with document 18's 11.1%: the gap comes from at least two different kinds of cause, and this document could not precisely measure a third (exactly how much the keyword method itself missed).** Across the same 10 projects, in the same raw-line-count scope, the manually reviewed A+B+C comes to 47.9%, while the keyword method measures only 24.0% — but these two numbers cannot simply be subtracted to get "how much was missed," because this document's own A/C definitions are broader than document 18's 12 categories (details and the acknowledged limitation are in section 6). The two gaps that can be confirmed are D (syntactic difference) and E (functionality gaps) — two categories document 18's 12-category busywork definition never intended to cover from the start — accounting for 15.1% and 9.6% of the total core line count respectively, about 24.7 percentage points combined, and there's no dispute that this gap exists. Adding in the broader A/C definitions plus some genuine missed detection (about 23 percentage points combined, i.e. 47.9%−24.0%, though this document cannot separate the two causes within that number) roughly explains the gap between 11.1% and "a typical project's (mid-ratio group) net cut of about sixty percent" — but exactly how much the keyword method missed is a sub-question this document leaves as unfinished work, not something it pretends to have precisely measured.

**"Fully equivalent" does not mean "no functionality gap" — this has to be stated separately, or it will mislead.** Among the 15 manually reviewed projects, three — B5-02 (yapp), B4-06 (jevyoumean), B6-06 (JevMood) — are marked "fully equivalent" in the rewrite records (a field-by-field comparison under a fixture's fixed reading table matches exactly), but after reading the code block by block, this document found genuine category-E functionality gaps in each of them: B5-02's entire learning mechanism for "dynamically generating candidate descriptions from learned positive/negative examples" (30 lines; `adapter.py` only feeds this to the original project's Python side, never moved anywhere J++ would read it); B4-06's candidate description text being simplified in transit (`build_jpp_input.py` only takes the candidate key, not the value — 7 lines of description text genuinely lost); B6-06's continuous-value volume scaling and sorting by volume (6 lines, a known non-equivalence). **These three gaps are not the same kind of thing, and have to be discussed separately**: for B5-02 and B6-06, the gap is already honestly recorded in each project's `gap_titles` field ("the learning mechanism is not equivalent," "the continuous volume value and the activeSounds sort order are not comparable") — the overall "fully equivalent" label is still given because the equivalence judgment follows whatever fields the fixture can cover; the known partial gap is recorded but doesn't pull down the overall label — this is a question of "accounting scope," not "the gap being hidden." Only B4-06's lost candidate-description text is confirmed to appear in no version of the rewrite record at all — a gap genuinely never recorded anywhere, found for the first time during this document's review. **"Fully equivalent" measures "the compared fields behave consistently under this fixture's readings," not "functional completeness is identical" — both known and unrecorded gaps can hide underneath the "fully equivalent" label, and 3 of the 15 projects reviewed in this document have one, to different degrees.**

**What was genuinely saved, what is just syntactic difference, and what is a cut feature — stated honestly and separately:**

- **Genuinely saved (the language/runtime took over something that used to be hand-written)**: the built-in structural-compliance guarantee on `judge()`/`select()`'s reading (no response validation to write, Examples 1 and 5), transport-layer retry/backoff (Example 2), concurrency scheduling (Example 3), sorting/truncation built into `order` (Example 4), cost counting and cache-key management that the ledger/budget makes unnecessary to write — this is the most solid contribution within category A, able to reach over a quarter of the net cut in projects with a fully written-out HTTP infrastructure; but as stated at the start of section 7, this equivalence has so far only been verified under a fixed observation, with no dedicated real-run comparison for "a call failing/a concurrency error" scenarios specifically.
- **Syntactic difference (not the language being smarter — the host language simply requires more words)**: type annotations, class/struct definitions, imports, docstrings in Python/TS/Go — this part (D, 30.2% of the 10 projects' net-cut total, or 15.1% if the denominator is the total core-line count of 881 — the two percentages refer to the same 133 lines, just with different denominators) has nothing to do with "J++'s judgment ability" — it's purely a matter of whether the host language has a type system, whether it needs import statements; the gap would shrink with a more concise host language.
- **Cut functionality (glossing over this would be dishonest and misleading)**: category E (19.3% of the 10 projects' net-cut total, 9.6% of the total core-line count of 881 — the same 85 lines under two denominators) contains genuine functionality gaps (unmerged two-round calls from a differing candidate set, cross-dimension ranking, chunking beyond 254 candidates, multi-tick environment loops, secondary sort keys, continuous-value scales), plus what was described above as "recorded but not pulled down from the overall label in fully-equivalent projects" (B5-02, B6-06) and "confirmed unrecorded" (B4-06) gaps, plus "moved elsewhere" (content carried over verbatim into a fixture/host file, not genuinely gone, counted separately here and not counted as cut). These are not the same kind of thing — some are things the language genuinely cannot do yet, some are a question of the accounting scope for equivalence judgments, some are just a choice of how to keep the books — the degree and the nature differ.

**A one-sentence summary for developers**: take a real project and plug it into JEV — what's genuinely repeated, what every project has to write over again, is not "how to chain multiple judgments together" (J++'s constructs for this exist, but the lines they save are often canceled out by the cost of making things explicit) — it's "how to assemble material and candidates into a question, and how to dig fields back out of the judgment result," plus the host language's own syntax tax. These two things together are more worth the language taking over than busywork itself (retry, caching, thresholds, concurrency) — and they are, in fact, the ones taken over most thoroughly.

---

**Editor's note (2026-09-27, in response to Nature's question: "could it be that they simply never used any composition at all?"):** Yes. Among the 10 net-scope projects, category B is 0 in 6 of them and negative in 4 — the 6 that are 0 simply had no block in the original code that could be counted as composition at all, so there was nothing to save; the 4 that are negative used a compact host-language idiom (a tuple comparison, a chained `??`) to wire up the judgment, and J++ expanding that into an explicit three-way exit made it longer. So this document can only say two things: (1) this batch of real projects is, essentially, a single round of judgment — it can't measure the value of composition, because the sample itself doesn't contain what would need to be measured (the rewrite report also recorded this: only 4 projects do any cross-candidate comparison at all); (2) once a composition construct is actually used, the explicit exit carries a fixed cost — and that cost is a design question (the default destination for "not sure"), not a sampling problem. The value of composition has to be measured against the target scenarios (the "class B" scenarios in an internal planning document, not published with this release), not against this batch of projects.
