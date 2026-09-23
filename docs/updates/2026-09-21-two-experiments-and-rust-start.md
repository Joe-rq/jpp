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

> **Corrected on 2026-09-21, after publication.** Two sets of claims in this file
> are wrong and are withdrawn: §2's two `choice` results (permutation consistency
> and "no first-position bias") were measurement artefacts, and §1's cost,
> latency and random-arm figures were produced by a broken accounting path and a
> single random seed. **The original wording stays in place**, marked where it is
> now known to be wrong, with a **Correction** block at the end of each section
> giving the mechanism and the replacement numbers. A published record is
> corrected in the open, not edited silently — the same rule that put the FAIL in
> §1's title. §3's claims were true when written and have since been overtaken by
> events; a dated **Status** block at its end says what is in the repository now.
>
> **2026-09-21 更正（发布之后）。** 本文有两组结论作废：§2 关于 `choice` 的两句
> （置换一致、无首位偏置）是测量假象；§1 的花费、时延与随机臂数字来自一条坏掉的
> 统计路径和一个随机种子。**原文一律留在原处**，错处就地标出，每节末尾附「更正」块，
> 写清机制与替换后的数字。公开记录里的错要改在明处，不做静默修改——这与 §1 标题
> 直接写 FAIL 是同一条纪律。§3 的说法在写下时为真、此后被事实追上，节末加了一个带
> 日期的「现状」块说明仓库里现在有什么。

> **Second correction, 2026-09-21, later the same day.** The correction to §2
> above replaced "permutation consistency 1.000 (74/74)" with "the real
> measurement is 7 items (7/7 consistent; 8/8 counting items without ground
> truth)". **That replacement number is itself wrong.** A direct count of
> `foundation/experiments/raw/e_cal/readings.jsonl` shows all **97** `choice`-type
> readings carry `mode_share = 1.0`, including every one of the **8** items that
> took the native `choice` path with two genuinely independent permutations each
> (`ans.phys == "choice"`, `perms: 2`) — not one of the 97 ever recorded
> disagreement. The corrected count is **0**, not 7; see **Second correction to
> §2** at the end of that section. Found while checking `Pick` in the Rust kernel
> against the raw readings, not by re-reading this write-up.
>
> **二次更正（同日稍晚）。** 上面对 §2 的更正把「置换一致 1.000（74/74）」换成了
> 「真测量只有 7 条（7/7 一致；含无真值条目 8/8）」。**这个替换后的数字本身也是
> 错的。** 直接数一遍 `foundation/experiments/raw/e_cal/readings.jsonl`：全部
> **97** 条 `choice` 类型读数的 `mode_share` 都是 1.0，包括全部 **8** 条真走了
> 原生 `choice` 路径、各自独立置换两次的条目（`ans.phys == "choice"`、
> `perms: 2`）——97 条里没有一条记录过分歧。更正后的数字是 **0**，不是 7；见该节
> 末尾「§2 二次更正」。是在给 Rust 内核的 `Pick` 做检查、核对原始读数时发现的，
> 不是重读本文发现的。

## 1. E9f-2b′ falsified: predicting an author's change requests is not decidable in one literal hop / E9f-2b′ 证伪：预测作者的修改要求，在一跳字面下不可判

**Result: FAIL. The pre-registered falsification criterion fired and this scenario
leaves the "what Jev can do" table.** *The FAIL stands and is not affected by the
correction below; what was wrong were the cost, latency and random-arm figures
around it. Every number marked † is superseded — see **Correction** at the end of
this section. / FAIL 本身不受影响，错的是它周围的花费、时延与随机臂数字；带 † 的数
字均已更正，见本节末「更正」。*

The question was whether a J++ program could read a document before its author
reviews it and mark the paragraphs the author will ask to change — turning an
expensive step (one "ask for a change → AI rewrites → look again" round trip,
measured at a median of ≈ 29 minutes from the session timestamps) into a cheap
one (one judgment, ≈ 5.5 s †). Ground truth came from a real log: every change
request the author made about the same document in a session, and the patch hunks
that followed it.

n = **1,564 paragraphs** across 9 documents and 19 † "just before a request"
checkpoints; **139 positives (8.9 %)**.

| Arm / 臂 | AUC | Skipped at recall 0.95 / 固定召回 0.95 下省掉 | Pre-registered bet / 赌值 | Result |
|---|---|---|---|---|
| Jev (`noul`, one literal hop) | **0.541** | **4.3 %** (68 / 1564) † | AUC ≥ 0.70, skip ≥ 30 % | **FAIL** |
| Heuristic (length + digits + tables) | 0.638 † | 12 % | AUC ≤ 0.60 | FAIL, in reverse — the heuristic beats Jev |
| Random | 0.483 | 6.4 % † | recall ≈ fraction sent | holds |
| `haiku` judge (same 400-item subsample) | **0.492** | — | comparable to Jev but slower and dearer | "comparable" holds, but both sit on the random line |

On the same 400-item subsample Jev scored 0.561 and `haiku` 0.492. ~~Median
latency was 21.0 s for `haiku` against 5.5 s for Jev; at list prices those 400
`haiku` calls would be worth about **$2.72** while all 1,564 Jev calls cost
**$0.0027**. That $2.72 is a list-price estimate, not money spent — the
`haiku` arm ran through the author's own Claude subscription.~~ † ~~The comparison
that survives is the shape: roughly **1000× cheaper, 4× faster, and lower
AUC**.~~ † *(Struck text superseded — see **Correction**. Jev's per-call latency
was never measured this round, so "4× faster" has no basis; the honest
comparison is a throughput ratio of ≈ 28× and a per-call price ratio of
120×–1,170×.)*

**Calibration does not hold at all: ECE 0.5423.** The readings pile up between
0.5 and 0.8; the median bucket at 0.738 has an actual positive rate of 0.113,
against a base rate of 0.089. Mean reading on positives 0.642, on negatives
0.630 — a **0.012** gap. The model is not pointing the wrong way; it is not
discriminating. It read the question as "is there anything improvable here",
whose answer is almost always yes.

**The gain formula was not falsified; the scenario was.** ~~Substituting into
ledger entry #38 with C_E ≈ 29 min and C_S ≈ 5.5 s gives a cost ratio of 319 and a
paper gain of +1,830 minutes.~~ † All of that comes from the cost ratio and none
from discrimination — ~~the random arm skips 6.4 %, more than Jev's 4.3 %~~ † (the
correct statement is that Jev's skip rate is **indistinguishable** from random,
permutation p = 0.63). The "steps saved at fixed recall" factor is ~~**zero**~~ †
**indistinguishable from zero** in this scenario. The formula is a product; ~~one
factor went to zero~~ † — one factor cannot be told apart from zero, which is as
far as this run licenses.

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
~~19 were used (11 dropped for having fewer than 5 paragraphs or zero
positives)~~ † — 21 were used and 7 dropped, **none of them for having zero
positives**;
(b) the `haiku` arm's bet ("AUC comparable to Jev") turned out to carry no
information, since both arms are on the random line; (c) the agent that ran the
experiment died having written a `report.json` whose `haiku` subsample statistics
were joined wrongly (n = 85, all positive). That file is not used. Every number
in this section was recomputed from the raw JSONL records.

**中文摘要。** 段级「作者会要求改这一段吗」在一跳字面判断下不可判，预注册的证伪判据
触发，2b 场景退出能做域表。n = 1,564 段（9 份文档、~~19~~ † 个「提要求之前」检查点），
阳性 139（8.9 %）。Jev AUC 0.541、固定召回 0.95 下只省掉 ~~4.3 %~~ †，赌的是
AUC ≥ 0.70、省 ≥ 30 %；启发式基线 AUC ~~0.638~~ † 反而更高；~~随机臂 0.483 能省
6.4 %，比 Jev 还多~~ †（正确说法是「与随机不可分辨」，置换 p = 0.63）。
haiku 裁判在同一 400 条子样本上 AUC 0.492，同样在随机线上——两者都没有判别力，所以
「haiku 与 Jev 相当」这个赌值没有信息量。校准彻底不成立：ECE 0.5423，阳性均值 0.642
对阴性 0.630，差 0.012；模型把题读成了「这段有没有可改之处」，答案几乎恒为「有」。
~~按账本 #38 公式代入，C_E ≈ 29 分钟、C_S ≈ 5.5 秒、成本比 319，纸面收益 +1,830
分钟~~ †，但这个正值全部来自成本比，「固定召回下省掉的步骤」这一项在本场景
~~取零~~ †**与零不可分辨**——公式没被证伪，是场景不成立。四条「怎么才能对」：把判断锚在人身上（作者历史要求作 ref）、
按请求类型分层、真值改用落在段内的增删行、先 transform 出全文结构再按位置分配预算。
能做域表新增一行「人作为执行器 / 预测作者的修改要求 = 不能做」；与 E9f 的代码正确性
行（AUC 0.855、ECE 0.085–0.10）对照，同一个 noul 原语在「文本有无可核查缺陷」上校准，
在「某个人会不会提要求」上完全不校准。

### Correction to §1 (2026-09-21): the FAIL stands, the numbers around it did not / §1 更正：FAIL 成立，但它周围的数字不成立

Found by an independent recheck of this section (`followup-2b`) and verified
line by line by the coordinator. **The falsification result is untouched** — the
criterion was "AUC < 0.6 or skip < 10 %", Jev's AUC is 0.541 with a bootstrap
interval of 0.491–0.592 and a permutation p of 0.054, so it does not even clear
"significantly above random". What was wrong is the accounting around it.

**Three substantive corrections.**

1. **Cost $0.0027 → $0.045, and 1,633 calls, not 1,564.** `run_2b.py:75`
   measured cost by differencing `rt.stats["cost"]` across programs, but
   `runtime.py` calls `reset_stats()` in every `@jv.program`'s `begin()`. The
   difference therefore telescopes and only the last program survives — the log
   shows the giveaway directly, "calls −175" and "$−0.00263". Summing the log's
   per-program cumulative column gives **$0.0452**. **The budget was never
   actually exceeded**: $0.018 for the largest document against a $0.05 cap, and
   $0.045 in total against a $0.30 cap. The instrument was broken; the spending
   discipline held. The same bug poisoned `secs_per_call`, so **C_S ≈ 5.5 s is
   withdrawn** — Jev's per-call latency was not measured this round. The bug is
   confined to `run_2b.py`; E9f ($0.029) and E-CAL ($0.0096) use non-differencing
   accounting and are unaffected.
2. **"Random skips 6.4 %, more than Jev" was a single-seed artefact.** Over 500
   resamples the random arm's mean is 5.2 % with a range of 2.4–9.5 %, and Jev's
   4.16 % sits inside it (permutation p = 0.63). The correct statement is
   **"indistinguishable from random"**, not "worse than random". The heuristic
   arm, by contrast, is a real signal (p ≤ 0.001) — which is the point worth
   keeping: **the free arm was the informative one.**
3. **The `haiku` price is a range, not $2.72.** The $2.72 came from the dead
   agent's broken `report.json`. The script charges `cache_read` at full price
   while most of it is CLI system-prompt cache reads at 0.1×, and the recorded
   fields are already summed, so the true figure cannot be recovered — it lies
   between **$1.3 and $12.9**, i.e. **120×–1,170× dearer per call than Jev**.
   "Roughly 1000× dearer" survives as a per-call upper bound, not as a total.
   **"4× slower" does not survive** (see correction 1); what the data supports is
   a throughput ratio of ≈ 28×.

**Smaller corrections.** Skipped paragraphs at recall 0.95 are **65 (4.16 %)**,
not 68 — eleven paragraphs tie at p = 0.41, one of them positive, and a tie band
has to be sent whole. Heuristic AUC is **0.643**, not 0.638. **21 checkpoints
were used and 7 dropped** (4 with no snapshot, 2 with no patch, 1 with fewer than
5 paragraphs) — **none was dropped for having zero positives**, so deviation (a)
as published was wrong about both the count and the reason. The positive-rate
floor across documents is **1.3 %**, not 0.5 %. Substituting the surviving
figures into #38 gives a throughput-based cost ratio of ≈ 9,212 and a paper gain
of ≈ +1,880 minutes — still positive, still entirely from the cost ratio.

**A robustness check added since publication**: a 50-item hand check of the
labelling rule (`e9f_2b/标注抽检.md`) shows its two clauses differ by an order of
magnitude — hits on deleted lines are **13/13 correct**, hits on context anchors
alone **0/12**, negatives 25/25. Cleaning the bad labels four different ways
gives AUC 0.541 / 0.586 / 0.584 / 0.520 with skip rates up to 7.2 %: **the
criterion fires every time.** Label noise explains part of the magnitude and none
of the FAIL, and the heuristic still beats Jev after every cleaning.

**中文摘要。** 本节的 FAIL 不动——判据是「AUC < 0.6 或省 < 10%」，Jev AUC 0.541
（自助 0.491–0.592、置换 p = 0.054），连「显著高于随机」都没到。错的是它周围的账：
(1) **花费 $0.0027 → $0.045、调用 1,633 次**：`run_2b.py:75` 跨程序差分
`rt.stats["cost"]`，而 `runtime.py` 每个 `@jv.program` 的 `begin()` 都
`reset_stats()`，望远镜求和只剩最后一个程序（日志里「调用 −175」「花 $−0.00263」
就是它）；逐程序求和得 $0.0452。**预算没有真的超**（单文档 $0.018 ≤ $0.05、总
$0.045 ≤ $0.30）——坏的是仪表，纪律没破。同一 bug 污染 `secs_per_call`，故
**C_S = 5.5 s 作废**，本轮没量到 Jev 的单次时延。此 bug 只在 `run_2b.py`，E9f 与
E-CAL 用的是另两种不差分的口径，不受影响。(2)「随机省 6.4% 比 Jev 还多」是**单种子
产物**——500 次重抽均值 5.2%、区间 2.4–9.5%，Jev 的 4.16% 落在其中，置换 p = 0.63，
正确说法是「**与随机不可分辨**」；启发式臂才是真信号（p ≤ 0.001），**免费那条臂才是
有信息的那条**。(3) haiku **$2.72 → 区间 $1.3–$12.9**（脚本把 cache_read 按满价计，
记录字段已加总无法还原），即**每次调用贵 120×–1,170×**；「贵约 1000 倍」作为单次上界
成立，作为总额不成立，「慢 4 倍」**不成立**，能支持的只有吞吐比 ≈ 28×。小处：省掉
**65 段（4.16%）**而非 68（p = 0.41 处 11 段并列含 1 阳性，并列必须整送）、启发式 AUC
**0.643** 而非 0.638、检查点**用 21 剔 7**（4 无快照、2 无补丁、1 段数 < 5，**无一条
因阳性 = 0 被剔**，故已发布的偏差记录 (a) 连数目带理由都错）、阳性率下界 **1.3%** 而非
0.5%；代回 #38 得成本比 ≈ 9,212、纸面收益 ≈ +1,880 分钟。发布后补的稳健性检查：50 条
人工抽检显示标注规则两个分句差一个量级（命中被删行 13/13 对，只命中上下文锚 0/12 对，
阴性 25/25），四种清洗口径 AUC 0.541 / 0.586 / 0.584 / 0.520、saved 最高 7.2%，**判据
次次触发**——标注噪声解释幅度，不解释 FAIL。

## 2. E-CAL final run: Chinese readings are usable as probabilities, but discrimination came in under every bet / E-CAL 正式版：中文读数可当概率用，但判别力全线低于赌值

**Result: none of the three falsification criteria fired; roughly half the bets
were met.** The headline numbers are good and the discrimination numbers are not,
and both belong in the same summary.

> **Corrected 2026-09-21: two of this section's `choice` results are withdrawn.**
> Permutation consistency was vacuously true on 67 of 74 items, and the
> "no first-position bias" finding is void — the items it was computed over carry
> no position at all. One of the three falsification criteria was the
> permutation one, so the sentence above should read *two of the three were
> evaluated and did not fire; the third was never evaluated at the scale it was
> pre-registered for* — it did not fire on the 7 items that were really measured,
> but 7 items cannot carry it. The passes go from three to two. The original wording below is kept as the published
> record; read **Correction** at the end of this section before citing anything
> about `choice` here.
>
> **2026-09-21 更正：本节两条 `choice` 结论作废。** 置换一致在 74 条里有 67 条是恒
> 真项；「无首位偏置」那条直接无效——算它的那些条目根本不含位置。三条证伪判据里有一条
> 就是置换那条，所以上面那句应读作「**三条里两条经检验未触发，第三条没有在它被写成
> 的规模上得到检验**」——它在真测量的 7 条上没触发，但 7 条撑不住它。过关数由三条降为
> 两条。原文照留作记录，引用本节 `choice` 相关结论前请先看本节末的
> 「更正」。

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
| `choice` | permutation consistency | 0.85 | ~~**1.000**~~ 1.000, of which **67 of 74 are vacuously true** | ~~PASS~~ **withdrawn — see Correction** |
| `score` | adjacent-band hit | 0.85 | **0.964** | PASS |
| `score` | expected-band MAE | 0.5 | 0.553 | marginal FAIL |

> **Scope added 2026-09-21.** The "all" column above is the both-models-agree
> calibration set, not a random sample of the material (202/202 agreement
> where ground truth exists, 77/95 where it does not) — every number in this
> table is optimistically biased by an amount now measured for two of the
> three question types. See
> [`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md).
> **范围补注（2026-09-21）**：上表「全体」栏是「两模型都同意」的校准集，不是材料的
> 随机样本（有真值处一致率 202/202，无真值处 77/95）——表中每个数字都同向乐观有偏，
> 偏多少现在对三种题型里的两种已经测出来了，见
> [`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md)。

**The `noul` reliability curve is monotone and close to the diagonal** (n = 73):
0.15 → 0.00, 0.25 → 0.00, 0.55 → 0.54, 0.66 → 0.59, 0.74 → 0.74, 0.83 → 1.00. The
falsification criterion — "ECE > 0.10 means the reading is ordinal only" — does
not fire, so **`noul` readings on Chinese material can be used as probabilities**,
consistent with the code-correctness reading in E9f (ECE 0.085–0.10).

> **Withdrawn on 2026-09-21 — measurement artefact.** The paragraph below is kept
> word for word as the published record. Do not cite it; see **Correction** at the
> end of this section. Its closing sentence — "the bias is a property of the key,
> not of the question type" — still stands as a statement of scope. This run
> simply did not test it.
>
> **`choice` showed no first-position bias on this material.** The first candidate
> was chosen 8.1 % of the time while it is correct 12.2 % of the time — the model
> picks the first option *less* often than ground truth. Forward and reversed
> candidate orders produced the same argmax in **74 of 74** cases. The ledger's
> premise that "first-position bias requires a compiler pass that permutes and takes
> the majority" does not hold for Chinese `select` with K = 4 and whole-paragraph
> candidates; there, that pass is pure overhead. This does not overturn H8, which
> was measured on other keys. It narrows it: **the bias is a property of the key,
> not of the question type.**

**以上一段已于 2026-09-21 作废（测量假象），原文照留，勿引用；末句「偏置是键的性质，
不是题型的性质」作为适用范围的话仍成立，只是本次没有检验它。详见本节末「更正」。**

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

**Why discrimination came in low, and what would make it right.** *(The AUC/argmax/MAE
numbers below carry the same 2026-09-21 scope note as the table above — see
[`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md).)*

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

**中文摘要。**〔**范围补注，2026-09-21**：下面「全体」栏是「两模型都同意」的校准集，
不是随机样本，见
[`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md)。〕
~~三条证伪判据一条都没触发~~ **（2026-09-21 更正：两条经检验未触发，
第三条「置换一致 < 0.75」没能被检验，见本节末「更正」）**，赌值只对了一半。真值是模型双标（Fable 与
Opus 一致且至少一方高置信），所以「全体」栏只含有真值的条目：noul 73 / choice 74 /
score 55；两模型一致但都非高置信的另列核对臂（21 / 20 / 36）。286 次调用、$0.0096。
过的：noul ECE 0.057（可靠性曲线单调贴对角线，中文 noul 读数可以当概率用）、
~~choice 置换一致 1.000（74/74）~~ **（更正：67/74 是恒真项，真测量只有 7/7，不成立）**、
score 相邻档 0.964。没过的：noul AUC 0.748（赌 0.85）、
Brier 0.194（赌 0.15）、choice ECE 0.199（赌 0.15）与 argmax 0.757（赌 0.85）、
score MAE 0.553（赌 0.5）。~~choice 在这批材料上**没有首位偏置**——首位被选 8.1 %，而真值里
首位正确占 12.2 %；账本里「首位偏置需要编译器置换取众数」的前提在中文 select、K=4、
候选为整段文本时不成立~~ **（2026-09-21 更正：整句作废，见本节末「更正」）**，偏置是
键的性质不是题型的性质（这句仍成立，但本次没有检验它）。**有效 n 远小于条目数**：300 条
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

### Correction to §2 (2026-09-21): one `select`, two physical question forms / §2 更正：同一个 `select` 走了两种物理题

Found by an independent recheck (`followup-ecal`) and confirmed by the
coordinator reading the compiler path. The two `choice` statements above are
artefacts of how the question was actually asked.

**What actually ran.** Of the 97 `select` items, **only 8 emitted a `choice`
physical question**. The other 89 had candidates 176–372 tokens long, which fall
into the model archive's `k_limit` bands "120–250, unmeasured" and
"≥ 300 → K_max = 4", so the compiler lowered them to **per-candidate `noul`**:
one `noul` per candidate, argmax over the readings. The summary table reported
both paths under one row.

**Consequence 1 — permutation consistency was a tautology.** On the K-noul path
the aggregation branch hard-codes `mode_share` to 1.0, and the permutation test
is exactly `mode_share ≥ 1.0`. **67 of the 74 ground-truth items took that path**,
so those 67 were guaranteed to "agree" before any model was called. ~~The real
measurement is **7 items (7/7 consistent; 8/8 counting items without ground
truth)**. A bet of 0.85 has no power at n = 7~~ ‡, and the pre-registered
falsification criterion for this metric — "permutation consistency < 0.75 means
first-position bias must be handled by a compiler pass that permutes and takes
the majority" — ~~did not fire on those 7~~ ‡ was never given a single data point
capable of making it fire, on the real path or the degenerate one, and was never
evaluated at the scale it was written for. ‡ *(Both figures struck above are
themselves wrong — see **Second correction to §2** below.)*

**Consequence 2 — the first-position finding is void, not merely weak.**
Per-candidate `noul` has no notion of position, so the 67 lowered items carry no
positional information whatsoever; the 8.1 % and 12.2 % figures mixed two
incommensurable paths. On the 8 items that really ran `choice`, **the first
candidate was chosen 3 times out of 8 against a ground-truth first-position rate
of 1 in 8 — the direction points toward bias, not away from it.** At n = 8 the
only defensible statement is that **this run does not support "no first-position
bias"**; it does not establish the opposite either. Measuring it properly needs
either the `k_limit` 120–250 band filled in or `phys="choice"` forced in the item
set.

**What survives.** The `noul` results (ECE 0.057, monotone reliability curve),
the `score` results (adjacent band 0.964) and every discrimination number
(`noul` AUC 0.748, `choice` argmax 0.757, `score` MAE 0.553) are untouched by
*this* correction, as is the effective-n finding (17–26, not 100) — but see the
scope note added 2026-09-21:
[`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md),
"untouched" describes this correction only, not the calibration set's
selection bias. The scope sentence "the bias is a
property of the key, not of the question type" still stands on its own terms;
this run did not test it. The proposal that the permute-and-vote pass should be
switched **per key** also stands, but its justification changes: the basis is now
the permutation consistency actually measured for that key, and where a key has
no such measurement, the pass stays on.

**The artefact is itself a finding.** The same `select`, on the same material,
took two different physical forms, and one summary row averaged across them. Two
gaps are recorded against the kernel: K-noul's `mode_share` being fixed at 1.0
turns every permutation-style metric into a tautology, and a single calibration
key spanning two physical forms makes the meaning of its threshold ambiguous.

**中文摘要。** 97 条 `select` 里**只有 8 条真的发出了 `choice` 物理题**；其余 89 条因
候选长 176–372 token，落在模型档案 `k_limit` 的「120–250 未测」档与「≥ 300 →
K_max = 4」档，被编译器下沉成**逐候选 noul**（每候选一道 noul，取 argmax），而汇总表
把两条路的数字并在一行里报。后果一：K-noul 的 `mode_share` 被聚合分支写死为 1.0，而
「置换一致」的判据就是 `mode_share ≥ 1.0`——**74 条有真值条目里 67 条走这条路**，在调
模型之前就注定「一致」。~~真测量只有 7 条（7/7；含无真值条目 8/8），赌值 0.85 在
n = 7 上没有检验力~~‡；预注册里「置换一致 < 0.75 → 首位偏置必须由编译器置换取众数」这条
证伪判据~~在那 7 条上没触发~~‡，从未拿到过任何一个真能让它触发的数据点——不论走的是
真路径还是退化路径，也从未在它被写成的规模上得到检验。‡ **（以上两处「7」本身也是
错的，见本节末「§2 二次更正」。）** 后果二：**首位偏置那条
直接无效**——逐候选 noul 根本没有「位置」，那 67 条不含位置信息，8.1 % 与 12.2 % 是把
两条不可比的路混在一起算的；在唯一真跑了 choice 的 8 条上，**首位被选 3/8，真值首位
1/8，方向反而朝着有偏置**。n = 8 只能说「本次不支持无偏置」，不能反向断言；要真判中文
长候选 select 的偏置，得先补 `k_limit` 的 120–250 档，或在题集里强制 `phys="choice"`。
**没受影响的**：noul（ECE 0.057、曲线单调）、score（相邻 0.964）、全部判别力数字
（noul AUC 0.748、choice argmax 0.757、score MAE 0.553）与「有效 n 17–26 而非 100」。
**「没受影响」说的只是这次更正，不是校准集的选择偏倚**——范围补注见 2026-09-21
[`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md)。
「偏置是键的性质不是题型的性质」这句本身仍成立，只是本次没有检验它；「置换取众数按键
开关」的提议也仍成立，但依据改为「该键实测的置换一致率，没有这个数就保守开」。这件事
本身是一个发现：同一个 `select` 在同一批材料上走了两种物理形式，而汇总指标把两条路的
数字混着报。已记两条内核缺口：K-noul 的 `mode_share` 恒 1.0 让一切置换类指标变成恒真
项；同一校准键横跨两种物理形式会让线的含义不同。

### Second correction to §2 (2026-09-21, later the same day): the corrected count of 7 was itself wrong / §2 二次更正：更正后的「7」本身也是错的

Found while checking `Pick` in the Rust kernel against the raw readings, not by
re-reading this write-up.

**What the first correction got right and what it got wrong.** It correctly
identified that the K-noul path's `mode_share` is hard-coded to `1.0`
(`foundation/jv/runtime.py:1080`) and is therefore a tautology, and it correctly
separated that from the native `choice` path, whose `mode_share` is genuinely
computed as `Counter(picks).most_common(1)[0][1] / len(picks)`
(`runtime.py:1069-1070`) over `perms = 2` independently ordered runs — a value
that could have come out at 0.5 had the two permutations disagreed. What it got
wrong is the count: it reported this genuine computation as "7 items (7/7
consistent; 8/8 counting items without ground truth)".

**A direct count of `foundation/experiments/raw/e_cal/readings.jsonl` shows
something stronger.** All **97** `choice`-type readings in the file — not just
the 74 that have ground truth — carry `mode_share = 1.0`. That includes every
one of the **8** items whose `ans.phys` is `choice` (the native path, each with
`perms: 2`). Zero of the 97, real or tautological, ever recorded a value below
1.0. The pre-registered falsification criterion for this metric — "permutation
consistency < 0.75" — was therefore never given a single data point capable of
making it fire, on the real path or the degenerate one. A count of "7 real
measurements, 7/7 consistent" describes a test that came in just short of its
bet; a count of "0" describes a test this dataset was never able to run at all.
The corrected report should say the latter, not the former: **the third
falsification criterion was pre-registered and never once evaluated on any item
in this experiment**, and the 0.85 bet against it was uncontested by data rather
than narrowly missed by it.

**The correction that made this mistake was itself a correction about exactly
this failure mode.** The paragraph it corrected exists to say "a hard-coded 1.0
is not a measurement" — and it then reported eight genuinely-computed 1.0 values
as if counting them settled the question, without checking that those eight also
sit at the one value the criterion can never distinguish from "untested". The
eight readings were printed in the same recheck's own output; nobody had to
re-run anything to catch this, only to read the printout past the ground-truth
filter.

**What survives.** Every other number in §2 — the `noul` and `score` results,
the discrimination figures, the effective-*n* finding — is untouched by *this*
correction (see the 2026-09-21 scope note on the calibration set's selection
bias:
[`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md)).
The scope
statement "the bias is a property of the key, not of the question type" still
stands and this run still does not test it.

**中文摘要。** 在给 Rust 内核的 `Pick` 做检查、核对原始读数时发现，不是重读本文
发现的。**第一次更正对的地方与错的地方**：它正确指出 K-noul 路径的 `mode_share`
是硬编码的 1.0（`foundation/jv/runtime.py:1080`），因此是恒真项；也正确把它与原生
`choice` 路径区分开——原生路径的 `mode_share` 是真算出来的，
`Counter(picks).most_common(1)[0][1] / len(picks)`（`runtime.py:1069-1070`），
基于 `perms = 2` 次独立排序的真跑，若两次置换不一致，这个值本可以是 0.5。它错的
地方是数目：把这个真计算的结果报成了「7 条（7/7 一致；含无真值条目 8/8）」。
**直接数一遍 `foundation/experiments/raw/e_cal/readings.jsonl` 得到更强的结论**：
文件里全部 **97** 条 `choice` 类型读数——不只是有真值的 74 条——`mode_share` 都是
1.0，包括全部 **8** 条 `ans.phys` 为 `choice`（原生路径，每条 `perms: 2`）的
条目。97 条里，真跑的也好、恒真的也好，没有一条记录过低于 1.0 的值。预注册的第
三条证伪判据「置换一致 < 0.75」因此从未拿到过任何一个能让它触发的数据点——不论
走的是真路径还是退化路径。「7 条真测量，7/7 一致」描述的是一次差一点没过判据的
检验；「0」描述的是一次这批数据从未能真正跑起来的检验。更正应该写后者，不是
前者：**第三条证伪判据预注册了，但在本实验里没有在任何一条条目上被真正评估过**，
赌值 0.85 不是「小幅未达标」，而是从未遇到过对手数据。**犯这次错的更正，本身就是
在讲同一种失效方式的更正**——它更正的那段话存在的意义正是「硬编码的 1.0 不是
测量」，而它接着把 8 个真算出来的 1.0 值当成数出来就算数了，没有再检查这 8 个值
同样落在「和未测无法区分」的唯一那个值上。这 8 条读数当时就打印在同一次核查的
输出里，不需要重跑任何东西才能抓到这一点，只需要把打印结果看过真值过滤那一步
之后再往下看一眼。**没受影响的部分**：§2 其余全部数字——`noul` 与 `score` 的结果、
判别力数字、有效 n 的发现——就本次更正而言都不受影响；**校准集本身的选择偏倚是另一条
限定**，见 2026-09-21 范围补注
[`docs/updates/2026-09-21-scope-note-and-conformal-fail.md`](2026-09-21-scope-note-and-conformal-fail.md)。
「偏置是键的性质、不是题型的性质」这句
适用范围的话仍然成立，本次仍然没有检验它。

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

**Nothing Rust is in this repository yet, on purpose.** † The core crate is under
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
能力。**Rust 代码这次故意不同步** †：core crate 还在工作区建设中、尚未 build 通过，现在
推上来只会是坏的；本仓库这条线目前只有 ADR。下一次这条线的更新要么带着真实构建和一个
端到端跑通的 `.jpp` 程序，要么就不发。依据文本这边只加了提议：`12-IR与类契约-v0.1.md`
末尾附注说明 Rust 决定替代 §6 的施工安排而六种效应形式、类型纪律、检查规则、pass 与
账本语义不变，`00-宪法.md` 登记表加一行——两条都标着「待 Nature 认」，因为依据文本只能
由他修改，代理只能附注提议。

### † Status on 2026-09-21, later the same day / † 当日稍晚的现状

The two statements marked † above were accurate when this page was written and
have since been overtaken. **The repository now contains a Rust workspace under
`rust/`**: `rust/crates/jpp-core/`, `rust/crates/jpp-frontend/` and
`rust/crates/jpp-cli/` — 28 `.rs` files — plus six `.jpp` examples under
`rust/examples/`, a `Cargo.toml`/`Cargo.lock` workspace, and `README.md`,
`FRONTEND.md` and `COMPARISON.md`. It arrived on the front-end line
(`d39b036` "run standalone J++ source on Rust", `b471f4b` "distinguish stored
methods from invoked effects", merged as `0e76955`, PR #12), so the milestone §3
set — a real build with a `.jpp` program executing end to end — was met by that
line rather than by this one.

**The kernel side has not been merged and the two copies have diverged.** The
research workspace's `jpp-core` carries a second package that the published copy
does not: `Value::Duty`, which makes J-05 hold strictly, and `Type::Method`
carrying an effect row. Neither symbol appears in the published
`rust/crates/jpp-core/src/value.rs`. Reconciling the two lines goes through
`COORDINATION.md`, not through a commit whose subject is correcting experiment
numbers, so **this correction does not touch `rust/`** and takes no position on
which copy is authoritative — that is the coordinator's call, and it will be
published on its own.

**当日稍晚的现状。** 上面两处标 † 的说法在写下本页时为真，此后被事实追上。**本仓库
现在有 Rust 工作区，在 `rust/` 下**：`rust/crates/jpp-core/`、
`rust/crates/jpp-frontend/`、`rust/crates/jpp-cli/`（共 28 个 `.rs`），加
`rust/examples/` 下六个 `.jpp` 示例、`Cargo.toml`/`Cargo.lock` 工作区，以及
`README.md`、`FRONTEND.md`、`COMPARISON.md`。它由前端那条线推来
（`d39b036`「独立源码检查与执行贯通」、`b471f4b`「区分保存方法与执行效应」，
合并于 `0e76955`，PR #12）——§3 立下的里程碑「带着真实构建和一个端到端跑通的 `.jpp`
程序」由那条线兑现，不是这条线。**内核那一侧尚未合入，两份副本已经分叉**：研究工作区
的 `jpp-core` 多出第二包——`Value::Duty`（使 J-05 严格成立）与带效应行的
`Type::Method`，这两个符号在已发布的 `rust/crates/jpp-core/src/value.rs` 里一个都没有。
两条线怎么合要走 `COORDINATION.md`，不该塞进一个「更正实验数字」的 commit，所以**本次
更正不碰 `rust/`**，也不对「哪一份为准」下结论——那是总控要裁的，将另行发布。

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

**Correction re-sync (2026-09-21).** The corrections in §1 and §2 were written in
the research workspace first; the three documents carrying them —
`前提结论.md`, `09-研究方法与假设账本.md` and `DECISIONS.md` — were re-synced to
`research/地基/` in the same commit as this file, so the research copy and this
page state the same thing. `EXPERIMENTS.md` was re-synced too, bringing the
pre-registrations for **E-LABCONF** (annotator confidence as a free difficulty
predictor) and **E9f-2c** (asking the 2b′ question again with the author's earlier
requests as a `ref` anchor) — both written before their runs, and E9f-2c not yet
approved to run. Nothing under `src/` changed.

**更正同步（2026-09-21）。** §1、§2 的更正先写在研究工作区，携带它们的三个文件
（`前提结论.md`、`09-研究方法与假设账本.md`、`DECISIONS.md`）与本文在同一个 commit
里同步到 `research/地基/`，使研究副本与本页说法一致。`EXPERIMENTS.md` 一并同步，带上
**E-LABCONF**（标注者置信作为免费难度预测器）与 **E9f-2c**（把 2b′ 那道题重新问成对人
的预测，作者历史要求作 `ref` 锚）两份预注册——都写在开跑之前，E9f-2c 尚未获批开跑。
`src/` 下未改动。

**二次更正再同步（2026-09-21，同日稍晚）。** §2「二次更正」小节写在本页与
`前提结论.md`（同一处「E-CAL 正式版」一节），随本轮内核进展一并同步；细节与
更完整的内核进展见
[`docs/updates/2026-09-21-second-correction-and-kernel-progress.md`](2026-09-21-second-correction-and-kernel-progress.md)。
`src/` 下仍未改动。

**Second correction re-sync (2026-09-21, later the same day).** The §2 "Second
correction" subsection is written here and in `前提结论.md` (same "E-CAL 正式版"
section) and synced alongside this round's kernel progress; see
[`docs/updates/2026-09-21-second-correction-and-kernel-progress.md`](2026-09-21-second-correction-and-kernel-progress.md)
for the fuller picture. Nothing under `src/` changed.

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
