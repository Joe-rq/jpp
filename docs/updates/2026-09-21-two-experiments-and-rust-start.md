# 2026-09-21 update: one falsified scenario, one calibration reading, and the start of the Rust kernel / 一个被证伪的场景、一次校准读数、Rust 内核开工

Dated update following the practice in `progress.md`. This round produced no new
runtime capability. It produced two experiment results — one of which is a
failure — and moved the formal implementation to Rust. The two pytest rows in
"Verification in this repository" were re-run here on the date above against the
committed files. **Every experiment number below is a workspace measurement and
cannot be reproduced from this repository**; the raw records, scripts and
pre-registrations stay in the research workspace, and the write-ups are synced to
`research/地基/foundation/experiments/前提结论.md`.

本次没有新的运行能力。两个实验结果（其中一个是失败）和正式实现转向 Rust 是本轮的
全部内容。下面「本仓库验证」一节的两行测试是在本仓库当天重跑的；**实验数字全部来自
研究工作区，本仓库复现不了**，原始记录与预注册留在工作区，结论文本同步到
`research/地基/foundation/experiments/前提结论.md`。

## 1. E9f-2b′ falsified: predicting an author's change requests is not decidable in one literal hop / E9f-2b′ 证伪：预测作者的修改要求，在一跳字面下不可判

**Result: FAIL. The pre-registered falsification criterion fired and this scenario
leaves the "what Jev can do" table.**

The question was whether a J++ program could read a document before its author
reviews it and mark the paragraphs the author will ask to change — turning an
expensive step (one "ask for a change → AI rewrites → look again" round trip,
measured at a median of ≈ 29 minutes from the session timestamps) into a cheap
one (one judgment, ≈ 5.5 s). Ground truth came from a real log: every change
request the author made about the same document in a session, and the patch hunks
that followed it.

n = **1,564 paragraphs** across 9 documents and 19 "just before a request"
checkpoints; **139 positives (8.9 %)**.

| Arm / 臂 | AUC | Skipped at recall 0.95 / 固定召回 0.95 下省掉 | Pre-registered bet / 赌值 | Result |
|---|---|---|---|---|
| Jev (`noul`, one literal hop) | **0.541** | **4.3 %** (68 / 1564) | AUC ≥ 0.70, skip ≥ 30 % | **FAIL** |
| Heuristic (length + digits + tables) | 0.638 | 12 % | AUC ≤ 0.60 | FAIL, in reverse — the heuristic beats Jev |
| Random | 0.483 | 6.4 % | recall ≈ fraction sent | holds |
| `haiku` judge (same 400-item subsample) | **0.492** | — | comparable to Jev but slower and dearer | "comparable" holds, but both sit on the random line |

On the same 400-item subsample Jev scored 0.561 and `haiku` 0.492. Median latency
was 21.0 s for `haiku` against 5.5 s for Jev; at list prices those 400 `haiku`
calls would be worth about **$2.72** while all 1,564 Jev calls cost **$0.0027**.
That $2.72 is a list-price estimate, not money spent — the `haiku` arm ran through
the author's own Claude subscription. The comparison that survives is the shape:
roughly **1000× cheaper, 4× faster, and lower AUC**.

**Calibration does not hold at all: ECE 0.5423.** The readings pile up between
0.5 and 0.8; the median bucket at 0.738 has an actual positive rate of 0.113,
against a base rate of 0.089. Mean reading on positives 0.642, on negatives
0.630 — a **0.012** gap. The model is not pointing the wrong way; it is not
discriminating. It read the question as "is there anything improvable here",
whose answer is almost always yes.

**The gain formula was not falsified; the scenario was.** Substituting into
ledger entry #38 with C_E ≈ 29 min and C_S ≈ 5.5 s gives a cost ratio of 319 and a
paper gain of +1,830 minutes. All of that comes from the cost ratio and none from
discrimination — the random arm skips 6.4 %, more than Jev's 4.3 %. The
"steps saved at fixed recall" factor is **zero** in this scenario. The formula is
a product; one factor went to zero.

**Why the bet was wrong, and what would make it right.**

1. The question asked "does this paragraph have anything improvable" when it
   should have asked "will *this person* ask to change it". The fix is to anchor
   the judgment on the author: give their earlier requests as `ref` ("this author
   has previously asked for X, Y, Z; would they ask something similar here?").
2. Ground truth is very noisy. The positive rate ranges from 0.5 % (1 of 79
   paragraphs) to 75 % (36 of 48) across documents, because "go on then" makes a
   patch cover the whole file while "fix section 3" touches two paragraphs, and
   the two are equivalent in the labels. The fix is to stratify by request type
   and measure only on targeted edits.
3. Paragraph granularity and patch granularity do not line up: a hunk drags in
   neighbouring paragraphs as context. The fix is to define truth as
   added/deleted lines falling inside the paragraph, and to hand-check 50 items.
4. One literal hop is not enough. Whether an author will change a paragraph
   depends on their intent for the whole document, which is not in that
   paragraph's text. The fix is to `transform` the document structure first and
   then allocate judgment budget by position — which is exactly the "allocate
   budget by uncertainty" strength this run never used.

**What this adds to the domain table**: a new row, *human as executor /
predicting an author's change requests* = **cannot do** (one literal hop, AUC
0.54, ECE 0.54). Set against the E9f row for code correctness (AUC 0.855,
ECE 0.085–0.10): **the same `noul` primitive is calibrated on "does this text
have a checkable defect" and completely uncalibrated on "will this person ask for
a change".** That is a real boundary of the domain, not an implementation flaw.

**Deviations from pre-registration**: (a) ≤ 30 checkpoints were pre-registered,
19 were used (11 dropped for having fewer than 5 paragraphs or zero positives);
(b) the `haiku` arm's bet ("AUC comparable to Jev") turned out to carry no
information, since both arms are on the random line; (c) the agent that ran the
experiment died having written a `report.json` whose `haiku` subsample statistics
were joined wrongly (n = 85, all positive). That file is not used. Every number
in this section was recomputed from the raw JSONL records.

**中文摘要。** 段级「作者会要求改这一段吗」在一跳字面判断下不可判，预注册的证伪判据
触发，2b 场景退出能做域表。n = 1,564 段（9 份文档、19 个「提要求之前」检查点），
阳性 139（8.9 %）。Jev AUC 0.541、固定召回 0.95 下只省掉 4.3 %，赌的是 AUC ≥ 0.70、
省 ≥ 30 %；启发式基线 AUC 0.638 反而更高；随机臂 0.483 能省 6.4 %，比 Jev 还多。
haiku 裁判在同一 400 条子样本上 AUC 0.492，同样在随机线上——两者都没有判别力，所以
「haiku 与 Jev 相当」这个赌值没有信息量。校准彻底不成立：ECE 0.5423，阳性均值 0.642
对阴性 0.630，差 0.012；模型把题读成了「这段有没有可改之处」，答案几乎恒为「有」。
按账本 #38 公式代入，C_E ≈ 29 分钟、C_S ≈ 5.5 秒、成本比 319，纸面收益 +1,830 分钟，
但这个正值全部来自成本比，「固定召回下省掉的步骤」这一项在本场景取零——公式没被
证伪，是场景不成立。四条「怎么才能对」：把判断锚在人身上（作者历史要求作 ref）、
按请求类型分层、真值改用落在段内的增删行、先 transform 出全文结构再按位置分配预算。
能做域表新增一行「人作为执行器 / 预测作者的修改要求 = 不能做」；与 E9f 的代码正确性
行（AUC 0.855、ECE 0.085–0.10）对照，同一个 noul 原语在「文本有无可核查缺陷」上校准，
在「某个人会不会提要求」上完全不校准。

## 2. E-CAL final run: Chinese readings are usable as probabilities, but discrimination came in under every bet / E-CAL 正式版：中文读数可当概率用，但判别力全线低于赌值

**Result: none of the three falsification criteria fired; roughly half the bets
were met.** The headline numbers are good and the discrimination numbers are not,
and both belong in the same summary.

Ground truth is dual model labelling (Fable and Opus agree, at least one with
high confidence). The "all" column therefore only contains entries that have
ground truth: **noul 73 / choice 74 / score 55**. Entries where both models agreed
but neither was confident are reported separately as a check arm
(noul 21 / choice 20 / score 36). The run made **286 calls** for **$0.0096**
(≈ 400 were pre-registered; same-state questions fused).

| Question type | Metric | Bet | Measured (all) | Result |
|---|---|---|---|---|
| `noul` | ECE | ≤ 0.08 | **0.057** | PASS |
| `noul` | Brier | 0.15 | 0.194 | high |
| `noul` | AUC | 0.85 | 0.748 | FAIL |
| `choice` | ECE (p_max vs argmax correct) | 0.15 | 0.199 | marginal FAIL |
| `choice` | argmax accuracy | 0.85 | 0.757 | FAIL |
| `choice` | permutation consistency | 0.85 | **1.000** | PASS |
| `score` | adjacent-band hit | 0.85 | **0.964** | PASS |
| `score` | expected-band MAE | 0.5 | 0.553 | marginal FAIL |

**The `noul` reliability curve is monotone and close to the diagonal** (n = 73):
0.15 → 0.00, 0.25 → 0.00, 0.55 → 0.54, 0.66 → 0.59, 0.74 → 0.74, 0.83 → 1.00. The
falsification criterion — "ECE > 0.10 means the reading is ordinal only" — does
not fire, so **`noul` readings on Chinese material can be used as probabilities**,
consistent with the code-correctness reading in E9f (ECE 0.085–0.10).

**`choice` showed no first-position bias on this material.** The first candidate
was chosen 8.1 % of the time while it is correct 12.2 % of the time — the model
picks the first option *less* often than ground truth. Forward and reversed
candidate orders produced the same argmax in **74 of 74** cases. The ledger's
premise that "first-position bias requires a compiler pass that permutes and takes
the majority" does not hold for Chinese `select` with K = 4 and whole-paragraph
candidates; there, that pass is pure overhead. This does not overturn H8, which
was measured on other keys. It narrows it: **the bias is a property of the key,
not of the question type.**

**Effective n is much smaller than the item count.** Two annotating agents
independently noticed that the 300 items were recombined from only about
**45 independent paragraphs** — 49 distinct segments for `noul`, 45 for `choice`,
44 for `score`, with the most-reused segment appearing 9 times. Keeping only the
first occurrence of each segment leaves **noul n = 26** (AUC 0.855, ECE 0.192),
**choice n = 25** (argmax 0.88), **score n = 17** (adjacent 1.00). So the honest
range is **effective n between 17 and 26, not 100**. Deduplication *raised*
discrimination (AUC 0.748 → 0.855, argmax 0.757 → 0.88), which says the "all"
column was dragged down by the hard repeats — but at those n both columns are
directional only. The next item set has to draw segments from fresh material.

**The unconfident arm is genuinely harder.** On the entries where both models
agreed but neither was confident, `noul` AUC falls to 0.45 (below the random line)
and `choice` argmax to 0.40. An annotator's low confidence correlates strongly
with Jev getting it wrong, which is a free difficulty predictor: you can tell
which items Jev will misjudge without running Jev. That deserves its own
hypothesis and its own experiment.

**Every exit was `unsure`** (noul 100/100, choice 97/97, score 100/100), because
these keys have no entry in `calib.json` yet, and by J-03 a threshold may only
come from a calibration record. Producing those records is what this run was for;
they are stamped `label_source: 模型双标+人抽检`.

**Why discrimination came in low, and what would make it right.**

1. `noul` AUC 0.748 rather than 0.85: the bet extrapolated from E9f code
   correctness, where truth is objective and literally visible. Here truth is
   itself model-produced, and on the 21 entries where both models were unconfident
   AUC is 0.45 — **truth noise and task difficulty sit on the same items.** Those
   21 have to be labelled by hand before anyone can say whether Jev is wrong or
   the labels are.
2. `choice` argmax 0.757: accuracy is dominated by "none of these" (60.8 % of
   truth, 63.5 % of predictions); only 29 items have an actual answer among the
   four. The next item set should hold "none of these" near 25 % and grade
   candidate difficulty.
3. `score` MAE 0.553 with exact hits at only 0.49 but adjacent hits at 0.964: the
   band is right and the offset is systematic. Truth is distributed 1:17 / 2:2 /
   3:25 against predictions 1:13 / 2:14 / 3:19 — the model pushes bands 1 and 3
   into band 2. That is anchor density (only bands 1, 3 and 5 had anchors); adding
   anchors for 2 and 4 fixes it without touching the language.

**Deviations from pre-registration**: (a) ≈ 400 calls pre-registered, 286 made;
(b) the pre-registration did not say "only entries that have ground truth", which
is what the "all" column turned out to be; (c) **a material bug of our own making
was caught just before the run** — the regex that strips 【证据】 blocks when
building the v2 item set also swallowed candidate lines that did not begin with
「【」, so 28 of the 97 `choice` items had lost a candidate. The experiment
script's candidate-completeness assertion stopped it; the set was rebuilt and
diffed line by line against v1 to confirm that everything removed came from
evidence blocks. Without that assertion, those 28 items would have gone to Jev
with stems that no longer matched their labels.

**中文摘要。** 三条证伪判据一条都没触发，赌值只对了一半。真值是模型双标（Fable 与
Opus 一致且至少一方高置信），所以「全体」栏只含有真值的条目：noul 73 / choice 74 /
score 55；两模型一致但都非高置信的另列核对臂（21 / 20 / 36）。286 次调用、$0.0096。
过的：noul ECE 0.057（可靠性曲线单调贴对角线，中文 noul 读数可以当概率用）、choice
置换一致 1.000（74/74）、score 相邻档 0.964。没过的：noul AUC 0.748（赌 0.85）、
Brier 0.194（赌 0.15）、choice ECE 0.199（赌 0.15）与 argmax 0.757（赌 0.85）、
score MAE 0.553（赌 0.5）。choice 在这批材料上**没有首位偏置**——首位被选 8.1 %，而真值里
首位正确占 12.2 %；账本里「首位偏置需要编译器置换取众数」的前提在中文 select、K=4、
候选为整段文本时不成立，偏置是键的性质不是题型的性质。**有效 n 远小于条目数**：300 条
题面只由约 45 个独立段落重组而成（noul 49 段、choice 45 段、score 44 段，最多的一段
出现 9 次），按段落去重后 noul n=26（AUC 0.855）、choice n=25（argmax 0.88）、
score n=17（相邻 1.00）——**有效 n 在 17–26 之间，不是 100**；去重后判别力反而更高，
说明全体栏被重复段落里的难例拉低，但 n 太小只能当方向看。待抽检臂上 noul AUC 掉到
0.45（随机线下）、choice argmax 0.40，说明标注者的低置信与 Jev 判不准高度相关，这是
一个不用跑 Jev 就能得到的难度预测器。三种题的出口全是 unsure，因为这些键在
`calib.json` 里还没有记录，按 J-03 线只能来自校准记录——本次读数正是为写这些记录而跑。
偏差记录三条，其中第三条是我们自己造的材料 bug：v2 构造时删【证据】块的正则把不以
「【」开头的候选行也吃掉了，97 条 choice 里 28 条丢了一个候选，被实验脚本的候选完整性
断言当场拦下，重建并逐条比对 v1 后才跑。

## 3. The formal kernel moves to Rust; no Rust source is public yet / 正式内核转 Rust；Rust 源码尚未公开

The implementation direction changed this round: the formal J++ kernel is being
built in Rust, so that a user writes `.jpp` source which is parsed, type- and
effect-checked, and executed by a Rust runtime, instead of writing host-language
builder code. The decision and its first acceptance milestone are recorded in
[ADR 0001](../adr/0001-rust-kernel.md), which also carries the policy allowing
focused OCaml experiments around specific grammar and semantic questions while
the Rust mainline continues. The Python kernel under
`src/foundation/jv/` is **frozen**: it keeps getting bug fixes and stays the
behavioural reference, and experiment, calibration and annotation scripts stay in
Python, but no new kernel capability goes into it.

**Nothing Rust is in this repository yet, on purpose.** The core crate is under
construction in the research workspace and does not build cleanly, so publishing
it now would publish something broken. This repository currently holds the ADR
and nothing else from that line. The next update on this track will come with an
actual build and a `.jpp` program executing end to end, or it will not come.

What the design documents record about the switch is a *proposal*, not a ratified
change: `research/地基/12-IR与类契约-v0.1.md` gains a closing note saying the Rust
decision replaces the §6 construction plan ("the first surface layer is a Python
builder; independent syntax is deferred indefinitely") while leaving the six
effect forms, the typing discipline, the checking rules, the passes and the
ledger semantics untouched — those remain the specification the Rust kernel is
built against. `research/地基/00-宪法.md` gains one line in the borrowing table
for "Rust interpreter + standalone source". Both are marked as awaiting the
author's ratification, per the rule that only he edits the governing texts and
agents may only append proposals.

**中文摘要。** 正式内核改为 Rust：用户写 `.jpp` 源码，经解析、类型/效应检查，由 Rust
运行系统执行，不再写宿主语言的构建器代码；决定与第一个验收里程碑见 ADR 0001。Python
内核冻结——继续修 bug、继续作行为对照，实验与标注脚本继续用 Python，但不再新增内核
能力。**Rust 代码这次故意不同步**：core crate 还在工作区建设中、尚未 build 通过，现在
推上来只会是坏的；本仓库这条线目前只有 ADR。下一次这条线的更新要么带着真实构建和一个
端到端跑通的 `.jpp` 程序，要么就不发。依据文本这边只加了提议：`12-IR与类契约-v0.1.md`
末尾附注说明 Rust 决定替代 §6 的施工安排而六种效应形式、类型纪律、检查规则、pass 与
账本语义不变，`00-宪法.md` 登记表加一行——两条都标着「待 Nature 认」，因为依据文本只能
由他修改，代理只能附注提议。

## Where the evidence is / 证据在哪

There is no new command this round. The write-ups, pre-registrations and ledger
entries synced with this update:

| Document | Contents |
|---|---|
| `research/地基/foundation/experiments/前提结论.md` | §E9f-2b′ and §E-CAL 正式版 — the two sections above, in full |
| `research/地基/foundation/experiments/EXPERIMENTS.md` | the pre-registrations for both, written before the runs, including the falsification criteria that fired |
| `research/地基/09-研究方法与假设账本.md` | ledger entries for this round |
| `research/地基/DECISIONS.md` | the decision trail, including the switch to Rust |
| `research/地基/12-IR与类契约-v0.1.md` | closing note: the Rust decision replaces the §6 construction plan, semantics unchanged (proposal) |
| `research/地基/00-宪法.md` | borrowing-table row for the Rust interpreter (proposal) |

Raw model records, run ledgers and the private working notes are not published.

## Verification in this repository / 本仓库验证

This is a documentation-only change: it touches `docs/` and `research/` and no
file under `src/`. The tests below were run in a clean worktree taken from
`origin/main` with the synced files in place, on both supported Python versions,
including the Towow tests that live alongside the kernel tests.

| Check | Result |
|---|---|
| `python -m pytest -q`, Python 3.12 | **535 passed** (kernel tests under `tests/foundation_jv`, composition tests, partial-result tests, Towow tests) |
| `python -m pytest -q`, Python 3.13 | **535 passed** |
| Kernel package fingerprint | `93dd4ab507ff` (sha256 of concatenated `src/foundation/jv/*.py`, first 12 hex) — byte-identical to the kernel in the research workspace, so nothing in this change moved a kernel file |

The experiment numbers in sections 1 and 2 are workspace measurements and were
**not** re-run here; see `research/地基/foundation/experiments/前提结论.md`.

本次只改 `docs/` 与 `research/`，`src/` 下一个文件都没动。测试是在从 `origin/main`
新开的独立工作树里、把同步文件放好之后，在 3.12 与 3.13 上各跑一遍完整套件（含通爻
测试）：**各 535 项全过**。内核指纹 `93dd4ab507ff` 与研究工作区的内核逐字节相同。

## Next design question / 下一个设计问题

E9f-2b′ failed on a question whose answer lives outside the paragraph being
judged, and the fix named in every one of its four repair notes is the same:
build the document's structure first, then spend judgment budget where the
structure says the uncertainty is. E-CAL says a `noul` reading on Chinese
material is good enough to be that budget signal, and also that its
discrimination is weakest exactly where the annotators were unsure. So the next
question is whether "how confident is the labeller" and "how uncertain is this
judgment" are the same quantity — and if they are, whether the planner should
read it before spending, rather than the experimenter reading it afterwards.

E9f-2b′ 失败在一道答案不在被判段落之内的题上，而它四条「怎么才能对」指向的是同一件
事：先把文档结构 transform 出来，再按结构说的不确定处花判断预算。E-CAL 说中文材料上的
noul 读数足够好，可以当这个预算信号；它同时说，判别力最弱的地方恰好是标注者也不敢给
高置信的地方。所以下一个问题是：「标注者有多确定」与「这个判断有多不确定」是不是同一
个量；如果是，这个量应该由计划器在花钱之前读到，而不是由实验者在事后读到。
