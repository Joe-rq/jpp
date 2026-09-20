# 2026-09-21 update: a scope the published numbers were missing, and why it surfaced only now / 已发布数字缺一个适用范围，以及它为什么现在才被发现

Dated update following the practice in `progress.md`. This round adds a scope
limitation to four already-published numbers, reports a new conformal-design
result, and syncs a new pre-registration. **No `.py` or `.rs` source in this
repository changed.**

## 1. The calibration set is not a random sample, and four published numbers need a scope they did not have / 校准集不是随机样本，四个已发布数字缺一个此前没写的适用范围

**Read this section before any other**, because it qualifies numbers already
public: `noul` ECE **0.057**, `noul` AUC **0.748**, `choice` argmax accuracy
**0.757**, `score` adjacent-band hit **0.964** — all reported in
`docs/updates/2026-09-21-two-experiments-and-rust-start.md` §2 and in
`docs/progress.md`.

**The fact.** Ground truth for the E-CAL run is dual-model labelling: an item
gets a ground-truth label only if Fable and Opus agree and at least one is
highly confident. Measured directly against the raw readings: of the **202**
items that carry ground truth, Fable and Opus agree on **202 of 202 (100.0%)**.
Of the **95** items that do not carry ground truth, they agree on only
**77 of 95 (81.1%)**.

**The consequence.** The calibration set is, by construction, the subset where
the two labelling models agree — not a random subset of the material. If
agreement correlates with correctness — and this experiment already supplies
evidence in that direction, in §"The unconfident arm is genuinely harder" below
— then every number computed on this set is biased in the same direction:
optimistic. The hard items are systematically the ones left unlabelled.

**This does not withdraw the four numbers.** It gives them a scope they did not
have before: they describe performance **on the subset both models agree on**,
not on the full material. The only way to remove the scope is to label the 18
disagreeing items among the 95 unlabelled ones — which is what `E-NOUL-HI`'s
second labelling pass (§3 below) exists to do.

**Why this is being published as its own update rather than folded silently into
the next numbers.** While pre-registering `E-NOUL-HI`, checking a sample-size
claim in the conformal design (§2 below) turned up this same structure in the
labelling itself. Before writing the scope note, the workspace measured how
reliably an ad-hoc caveat travels with the number it qualifies: across the 15,
13 and 11 places `noul` ECE 0.057, `noul` AUC 0.748 and the `choice`
permutation-consistency tautology are cited in the research documents,
**a caveat placed next to the number survived at 47%, 46% and 64% of citation
sites respectively — never at 100%.** The two most expensive misses were the
two places `0.057` had already become the stated premise of a bet in another
experiment's pre-registration, unqualified. That measurement is the reason this
update places the scope note at every site listed in §"Where the evidence is"
below, rather than in one place and a promise to remember.

## 2. The bias direction is now measured, not merely hypothesized — for two of three question types / 偏倚方向已经测出来，不再只是假设——三种题型里的两种

This did not require a single new model call: the selection variable
(model confidence, agreement) has a value on the 202 already-labelled items,
so the "does low annotator confidence predict Jev being wrong" question can be
answered there directly, before extrapolating to the unlabelled side.

**On the 202 ground-truth items**, split by whether both models were highly
confident: both-high (n=147) has an error rate of **0.252**; at-least-one-not-high
(n=55) has **0.564** — a gap of **+0.312**. Broken out by question type, that gap
is not uniform: **noul +0.527, choice +0.418, score −0.062 (reversed).** The
pooled +0.312 is partly a mix effect and should not be quoted as a single
number.

**`score` is exactly the question type with the lowest ground-truth coverage
(55 of 100)** — the free predictor fails precisely where it is needed most. A
predictor that works where coverage is already good and fails where coverage is
worst is not saving what it looks like it is saving.

**Extrapolating only the defensible half** (the 77 "agree but neither
confident" items; the 18 disagreeing items have no comparable group among the
labelled ones and are not extrapolated) puts a **lower bound, not an estimate**,
on the true error rate: `noul` **0.301 → at least 0.392** (+0.090), `choice`
0.243 → 0.302, `score` 0.509 → 0.490 (predictor reversed, effectively unmoved).
It is a lower bound because every one of the 202 labelled items has at least one
highly-confident model behind it, while none of the 77 unlabelled ones do —
using the labelled set's least-confident tier (55 items) to fill in the 77 fills
in low.

**`noul` is the type to watch, not because it is biased the most, but because
it is the type behind "ECE 0.057, best calibrated"** — a number already written
into `calibration.zh_reliability_curve` and, until corrected, the stated premise
of the `E-ALLOC` pre-registration's bet. ECE itself has not been recomputed —
it needs ground truth per bucket, which the unlabelled side does not have — but
the direction is now known to run worse, not better.

## 3. `E-NOUL-HI`: the "just label 22 more" plan is withdrawn — with a real path in its place / 「再标 22 条」被撤回，换成一条真的路

`EXPERIMENTS.md` carries a new pre-registration, `E-NOUL-HI`, written before any
new labelling. Its first move was to count where the conformal design's own
proposed fix actually lived — and the count contradicted the design document.

The conformal design (§4 below) argued that `noul`'s tightest risk bound is
limited by sample size, not by reading quality, and that the fix had a clear
endpoint: label the high-reading region up to 22 zero-error items (the count a
zero-error certificate needs at α=0.10, `ln δ / ln(1−α)`). **Counting where
those 22 items would come from, before labelling anything, found they do not
exist**: `noul` has 100 items total, and only **7** carry `p ≥ 0.775` whether or
not the remaining unlabelled items are ever labelled. Reaching 22 would need
roughly 315 **new readings** landing on **new material** — a new experiment, not
a labelling pass — and even then the high-reading region spans only 6 distinct
object segments, so the smallest certifiable α at cluster granularity is
`1/7 ≈ 0.143`, meaning α=0.10 cannot be certified on 6 clusters regardless of
how much is labelled.

**The two measurements pre-registered instead, ranked by information value per
label spent:**

1. **One item (`N017`)** — the single unlabelled item in the high-reading region,
   where the two labelling models disagree (Fable says `ignore`, Opus says
   `act`). This one label directly tests the "zero error in the high-reading
   region" premise the whole `noul` conclusion rests on.
2. **The remaining 26 unlabelled `noul` items (27 total)** — a direct test of
   the selection bias itself, comparing error rates between the
   agree-and-confident and agree-but-not-confident groups on paired readings.

Both bets, the falsification line, and the "what I will not conclude even if
this comes back clean" boundary are written down before either label is placed;
see `EXPERIMENTS.md` for the full pre-registration.

## 4. Conformal risk control: FAIL as designed — every ≤20% false-release target is infeasible on this data / 保形风险控制：FAIL 原样报——任何 ≤20% 假放行目标在这批数据上无解

A new design document, `设计/保形弃权域-设计-2026-09-21.md`, asks whether
conformal risk control (Angelopoulos 2022) can turn a calibration threshold into
a number with a finite-sample guarantee, using the 297 already-paid-for E-CAL
readings — no new calls.

**Conclusion: it cannot, on this data, for any of the three question types.**
The tightest certifiable risk upper bound reachable is **noul 0.319, choice
0.269, score 0.251** (δ=0.10) — so any target of "false-release rate ≤ 20%" is
infeasible across the board. **Both this conclusion and the three bounds share
the same scope as §1 above**: they are computed on the 202-item calibration
set, which is the both-models-agree subset, so the true bounds could be worse.

This is not an implementation gap; it is two different data shortages that need
opposite fixes:

- **`noul` is short on sample size.** Its tightest band (threshold 0.775)
  releases 6 items with zero errors. Certifying zero-error at α=0.10 needs 22
  released items (at α=0.05, 45) — a fact about counting, independent of
  reading quality. The design document's first draft argued this gives `noul` a
  clear path via more labelling; §3 above found that path does not exist in the
  labelled material and requires new readings on new material instead.
- **`choice` is short on reading accuracy.** Its tightest band releases 18 items
  with 2 errors, an empirical false-release rate of 0.111 — an asymptote no
  amount of additional labelling moves past α=0.10. The fix is the reading
  itself, not the labelling set — the same conclusion as E-CAL-0's "calibration
  is a function of how literal the wording is."
- **`score` sits in between**: empirical rate 0.071, reachable at α=0.10,
  unreachable at α=0.05.

**The single most important number in this design is not one of the three
bounds — it is a unit-of-count effect on `score`.** At the item level, α=0.30 is
achievable; re-run as a 200-fold cluster-level resample (drawing one reading per
distinct object segment, since the 297 readings recombine only 61 distinct
segments), **0 of 200 resamples achieve a solution.** Same data, same method,
changing only what counts as one independent observation flips the answer from
"solvable" to "not solvable" — concrete evidence that treating repeated
readings of the same passage as independent observations manufactures a
guarantee that is not there.

**Certified acceptance requires a certificate, and today nothing produces one.**
The design proposes a gate: a calibration record may only move from "awaiting
ground truth" to "in service" behind a `Certificate::Line`; a `Certificate::Refused`
is a first-class outcome, not an error, and must report both the tightest bound
reachable and how many more zero-error items would be needed. Today, the only
code path that promotes a record to "in service" is `put`, which checks
nothing but `n > 0` — a hand-written threshold of 0.78 on 73 labelled `noul`
items goes into service with nothing checking what it is worth.

## 5. `conformal-proto`: an independent prototype crate, not part of `rust/`, and not built by this repository's tests / `conformal-proto`：独立原型 crate，不在 `rust/` 里，也不在本仓库的测试范围内

The design in §4 above ships with a runnable prototype, now published at
`research/地基/foundation/experiments/conformal-proto/`. Read this paragraph
before running anything in it: **this crate is not part of `rust/` and does not
build inside this repository.** Its one dependency is a `path` reference to
`rust-jpp/crates/jpp-core` in the private research workspace — a different tree
from the `jpp-core` published under this repository's `rust/crates/`, per
§3 of `docs/updates/2026-09-21-second-correction-and-kernel-progress.md`. There
is nothing to point that dependency at inside this repository, so `cargo test`
inside `conformal-proto/` cannot be run here; every number below was verified in
the private workspace, not in this repository's own CI.

**Current, honest state: 9 passed, 0 failed, 1 test that fails on purpose.**

```
running 10 tests across src/lib.rs, tests/{boundary,certified,ecal,gate}.rs
9 passed
1 FAILED — tests/gate.rs :: 证书门站在待真值到上岗这一步
```

**The failing test is the most publishable line in this section, and it is
failing for the right reason.** `tests/gate.rs` was written to *demonstrate a
gap*: it asserted that `store.put("e_cal.noul", 0.78, 0.22, 73, "上岗")` — a
hand-written threshold with no certificate behind it — succeeds and promotes
the record to "in service", because at the time nothing in the kernel stopped
it. That gap has since been closed elsewhere in the workspace kernel by the
certificate gate described in §4: `put` now refuses a promotion that has no
certificate behind it. The test still asserts the old, now-wrong expectation,
so it fails — correctly. **A red test has two possible meanings that look
identical in the output: "a gap still exists" and "a gap this test was built to
show has been closed, and the test was not updated to match."** This one is the
second kind, and it is being published in that state deliberately, because the
alternative — quietly fixing the assertion before publishing — would erase the
one piece of evidence that the fix actually landed.

**A second, smaller version of the same lesson: the crate itself needed a
one-line fix to compile at all.** The design document's own text reports
"10 passed" — true at the specific `jpp-core` commit it names (`b3747a0`).
Since then, `jpp-core` made a `cluster` field on `Sample` required (the
clustering discipline §4's design argues for), and three call sites in the
prototype's own test files needed that field added before `cargo test` would
even compile. **Changing a mechanism does not automatically propagate to
the trees that depend on it** — the same shape of gap as a documentation
citation that does not travel with the number it qualifies (§1 above), one
layer down, in code instead of prose.

## Where the evidence is / 证据在哪

| Document | Contents |
|---|---|
| `research/地基/foundation/experiments/前提结论.md` | §E-CAL 正式版: the selection-bias scope note (§1 above) and the bias-direction measurement (§2 above), in full |
| `research/地基/foundation/experiments/EXPERIMENTS.md` | the same two scope notes placed at each citation site inside this file, plus the new `E-NOUL-HI` pre-registration (§3 above) |
| `research/地基/09-研究方法与假设账本.md` | ledger entries for this round, synced in full — including entries on other topics from the same working session, per this project's audit-log discipline: a ledger that is curated before publication is a ledger nothing can be audited against |
| `research/地基/DECISIONS.md` | the decision trail for this round, synced in full, same reasoning |
| `research/地基/设计/保形弃权域-设计-2026-09-21.md` | the conformal design in full (§4 above); the scope note from §1 was added to its own text (§0 and §5) before this sync, so the design document does not need this update page to be read alongside it |
| `research/地基/foundation/experiments/conformal-proto/` | the prototype crate in full (§5 above): `src/lib.rs`, `tests/{boundary,certified,ecal,gate}.rs`, its fixture, the two `analyze*.py` scripts behind §4's numbers, plus two further analysis scripts and a pre-registration (`analyze3_noul_hi.py`, `analyze4_限定留存率.py`, `analyze5_第二个值.py`, `预注册-第二个值.md`) documenting the retention-rate measurement in §1 and the bias-direction measurement in §2 — included for completeness, not because they are all concluded work |
| `docs/updates/2026-09-21-two-experiments-and-rust-start.md` | pointer added at each site citing the four numbers, to this page |
| `docs/progress.md` | scope note added next to the same four numbers |

Raw model records, run ledgers, agent audit output, and private working notes
are not published. `foundation/experiments/raw/` is not published.

**Baseline.** This update was prepared against `origin/main` at `ec1720a`
(re-fetched immediately before this page was written), and against the research
workspace at commit `5157d89` — re-checked immediately before writing this
section, because the workspace kept moving while this page was drafted.

## Verification in this repository / 本仓库验证

This touches `docs/` and `research/` only: no `.py` or `.rs` source under
`src/` or `rust/` in this repository changed.

| Check | Result |
|---|---|
| `python -m pytest -q`, Python 3.12.12 | **544 passed** |
| `python -m pytest -q`, Python 3.13.12 | **544 passed** |
| `tools/sync-from-workspace.sh --self-test` | PASS |
| `tools/sync-from-workspace.sh --filter-only` | 1 redaction marker applied (the conformal design document's absolute workspace path, replaced with the crate's real relative `path` dependency); re-run afterward: 0 remaining |
| Third-party identifiers | best-effort scan (email-address pattern, the private workspace's own git author string) on the files this update touches: 0 new hits. This update does not have the exact "four tracked categories" tool prior rounds used; flagged in the sync report for confirmation rather than asserted as equivalent |
| Secret patterns (`ghp_`, `AKIA`, `PRIVATE KEY`, `xox`, `sk-`) | raw hits 3, 3, 3, 3, 6 across the repository — all confirmed non-credential substrings already documented in `research/地基/DECISIONS.md` (`ask-codex-typing`, `dask-jobqueue`, `runtime.py`'s `reason="ask-input"`, and prose describing the redaction rule itself); real credential count: 0 |
| Path-level exclusions (`附注/`, `experiments/raw`, `runs/`, `.venv`, verbatim-quote files) | 0 — none of these paths are among the files this update adds or modifies |

## Verified on the workspace kernel, which is not in this repository / 在工作区内核上复跑（该内核不在本仓库）

| Check | Result |
|---|---|
| `cargo test` inside `foundation/experiments/conformal-proto/`, against the workspace's `rust-jpp/crates/jpp-core` at the commit named in §5 | **9 passed, 0 failed, 1 failed on purpose** (`tests/gate.rs`, explained in §5) |
| `analyze5_第二个值.py`, re-run | reproduces the §2 numbers: noul +0.527, choice +0.418, score −0.062; lower bounds 0.301→0.392 / 0.243→0.302 / 0.509→0.490 |

## Next design question / 下一个设计问题

The 18 disagreeing items among the 95 unlabelled ones remain the one thing that
cannot be settled without new human labels (§3). Whether `E-NOUL-HI`'s single
highest-value label (`N017`) gets placed, and what it does to the `noul`
zero-error premise if it comes back `ignore`, is the next thing this line of
work is waiting on.

## 中文

**先读这一节，因为它限定的是已经公开的四个数字**：noul ECE **0.057**、noul AUC
**0.748**、choice argmax 正确率 **0.757**、score 相邻档命中 **0.964**——分别见
`docs/updates/2026-09-21-two-experiments-and-rust-start.md` §2 与
`docs/progress.md`。

### 一、校准集不是随机样本，四个已发布数字缺一个此前没写的适用范围

**实测**：E-CAL 的真值是模型双标——两个模型（Fable、Opus）一致且至少一方高置信才算
有真值。直接数原始读数：**有真值的 202 条里，两模型一致的是 202/202 = 100.0%**；
**没有真值的 95 条里，两模型一致的只有 77/95 = 81.1%**。

**后果**：这个校准集在定义上就是「两个模型都同意」的那个子集，不是随机子集。若「是否
同意」与「对不对」相关——本实验自己在下面「待抽检臂是真的更难」那条已经给出了这个方向
的证据——那么在这个集合上算出来的每一个数都同向有偏，方向是乐观：难例被系统性地留在
了未标注那一侧。

**这不是把那四个数作废**，是给它们加一个此前没写的适用范围：它们描述的是「两个模型都
同意的那类条目」上的表现，不是整批材料上的表现。拆掉这个限定只有一条路——把未标注的
95 条里那 18 条不一致的标出来，这正是下面 `E-NOUL-HI` 第二次标注要做的事（第三节）。

**为什么这条限定单独发一篇，而不是悄悄并进下一次数字更新里。** 写 `E-NOUL-HI` 预注册
时，核对保形设计（第四节）里一条样本量说法，顺带查出了标注集本身的这个结构。**在写这
条限定之前，工作区先测了一件事：写在数字旁边的限定条件到底能不能守住。** 在引用
noul ECE 0.057、noul AUC 0.748、choice 置换一致恒真项这三条限定的研究文档里，各自出现
15、13、11 次，**写在数字旁边的限定分别只在 47%、46%、64% 的引用处留存下来——没有一条
守住 100%。** 丢失代价最大的两处，是 0.057 已经变成另一个实验预注册里「我赌的结果」的
前提、且不带任何限定。这次把限定放进下面「证据在哪」列出的每一处，而不是放一处、指望
自己记得，理由就是这个实测。

### 二、偏倚方向已经测出来，不再只是假设——三种题型里的两种

**这一步没花一次新调用**：选择变量（模型置信、是否一致）在已标注的 202 条上本来就有
值，所以「标注者置信低是否预测 Jev 判不准」可以直接在这 202 条上测，不必等未标注那侧
补标。

**在有真值的 202 条上**，按两模型是否都高置信分组：两方都高（n=147）错误率
**0.252**；至少一方非高（n=55）错误率 **0.564**，差 **+0.312**。**分题型看，这个差不
是均匀的**：noul **+0.527**、choice **+0.418**、**score −0.062（反向）**。合并的
+0.312 有一部分是题型构成拖出来的，不能整体引用。

**而 score 恰恰是标注覆盖最低的那一型（55/100）**——这个免费代理在最需要它的地方
失效。一个在覆盖好的地方好用、在覆盖最差的地方失效的代理，它省下的钱和它该省的钱不是
同一笔。

**只外推站得住的那一半**（77 条「一致但都不高置信」；18 条不一致在构造上没有对照组，
不外推），得到的是**下界不是估计**：noul 账面错误率 **0.301 → 至少 0.392（+0.090）**、
choice 0.243 → 0.302、score 0.509 → 0.490（代理反向，等于不动）。是下界的理由：已标注
202 条每一条都至少一方置信高，未标注 77 条一条都没有——拿已标注里置信最低的 55 条去补
77 条，是补低了。

**noul 最该在意，不是因为它偏得最多，而是因为它正是「ECE 0.057 校准最好」那个数的
题型**——那个数已经进了档案 `calibration.zh_reliability_curve`，在订正之前还是
`E-ALLOC` 预注册里赌的前提。ECE 本身没有重算（要逐桶真值，未标注那侧没有），但方向已
经知道只会变差，不会变好。

### 三、`E-NOUL-HI`：「再标 22 条」被撤回，换成一条真的路

`EXPERIMENTS.md` 新增预注册 `E-NOUL-HI`，写在任何新标注之前。它的第一个动作，是去数
保形设计自己提出的那条修法到底成不成立——数出来的结果推翻了设计文原来的说法。

保形设计（第四节）原本认为 noul 卡在样本量而不是读数质量，而且修法有明确终点：把高读
数区标到 22 条零错（零错认证 α=0.10 所需的条数，`ln δ / ln(1−α)`）。**标注之前先去数
那 22 条从哪儿来，结果是它们不存在**：noul 一共 100 条，`p ≥ 0.775` 的**总共只有 7
条**，把剩下的全标完也还是 7 条。凑够 22 条大约需要 315 条**新读数**落在**新材料**
上——那是一次新实验，不是一次标注；而且高读数区只有 6 个不同对象段，簇级最小可认证的
α 是 `1/7 ≈ 0.143`，α=0.10 在 6 个簇上无论怎么标都认证不到。

**按每条标注的信息量排序，预注册了两次测量：**

1. **标 1 条（`N017`）**——高读数区里唯一未标注的条目，两个标注模型在这条上分岔
   （Fable 说 `ignore`、Opus 说 `act`）。这一条直接检验「高读数区零错」这个 noul 全部
   结论的支点。
2. **标其余 26 条（合计 27 条全标）**——直接检验选择偏倚本身：按配对读数比较「一致且
   高置信」与「一致但不高置信」两组的错误率。

两条赌、过关线、以及「就算结果干净我也不会下的结论」都在动手之前写清楚，完整预注册见
`EXPERIMENTS.md`。

### 四、保形风险控制：FAIL 原样报——任何 ≤20% 假放行目标在这批数据上无解

新设计文档 `设计/保形弃权域-设计-2026-09-21.md` 问：保形风险控制（Angelopoulos
2022）能不能把一条校准阈值变成一个带有限样本保证的数字，用的是已经付过费的 297 条
E-CAL 真机读数，不发新调用。

**结论：在这批数据上，三种题型都不能。** 能认证到的最紧风险上界分别是 **noul
0.319、choice 0.269、score 0.251**（δ=0.10）——任何「假放行率 ≤ 20%」的目标全部无
解。**这个结论与三个上界，适用范围与第一节相同**：都算在 202 条「两模型都同意」的校准
集上，真实上界可能更差。

这不是实现的问题，是两处不同的数据短缺，修法完全相反：

- **noul 卡样本量。** 最紧那一格（线 0.775）放行 6 条、零错。零错认证 α=0.10 需要放
  行区有 22 条（α=0.05 需要 45 条）——这是计数上的事实，与读数准不准无关。设计文最初
  认为这给了 noul 一条靠标注就能走完的路；第三节查出那条路在已有材料里不存在，需要新
  读数落在新材料上。
- **choice 卡读数本身。** 最紧那一格放行 18 条、错 2 条，经验假放行率 0.111——这是渐
  近值，再多标注也压不到 α=0.10 之下。要动的是读数，不是标注集，与 E-CAL-0「校准是字
  面化程度的函数」是同一个结论。
- **score 居中**：经验率 0.071，α=0.10 可达、α=0.05 不可达。

**这份设计里最重的一条不是那三个上界，是 score 上的一个计数单位效应。** 按条数算，
α=0.30 有解；换成 200 次簇级重采样（每个不同对象段取一条，因为 297 条读数只由 61 个
不同片段重组而成），**200 次里 0 次有解**。同一批数据、同一个方法，只换「什么算一次独
立观察」，结论从「有」翻到「没有」——把同一段落的多条读数当成多次独立观察，会凭空造
出一个不存在的保证。

**认证上岗需要一张证书，而今天没有任何东西产生它。** 设计提出一道门：校准记录要从
「待真值」变成「上岗」，必须先拿到一张 `Certificate::Line`；`Certificate::Refused` 是
一等出口不是错误，必须同时报出这批数据能拿到的最紧上界，以及零错还差多少条。今天唯一
能让记录上岗的入口是 `put`，它只核 `n > 0`——一条手写的 0.78 配 73 条标注数据就能上
岗，没有任何东西核过它凭什么。

### 五、`conformal-proto`：独立原型 crate，不在 `rust/` 里，也不在本仓库测试范围内

第四节的设计带着一个可跑的原型，现已发布在
`research/地基/foundation/experiments/conformal-proto/`。**在这里跑任何东西之前先读
这句：这个 crate 不属于 `rust/`，在本仓库里编译不过。** 它唯一的依赖是一条指向研究工
作区 `rust-jpp/crates/jpp-core` 的 `path` 引用——与本仓库 `rust/crates/` 下发布的
`jpp-core` 是两棵不同的树（参见
`docs/updates/2026-09-21-second-correction-and-kernel-progress.md` §3）。本仓库里没
有东西可以把这条依赖指过去，所以 `cargo test` 在 `conformal-proto/` 目录下**在本仓库
跑不起来**；下面每个数字都是在研究工作区里验证的，不是本仓库自己的 CI。

**当前的真实状态：9 通过、0 失败、1 条故意失败。**

```
running 10 tests across src/lib.rs, tests/{boundary,certified,ecal,gate}.rs
9 passed
1 FAILED — tests/gate.rs :: 证书门站在待真值到上岗这一步
```

**这条红是本节最值得公开的一条，而且它红得对。** `tests/gate.rs` 当初写出来是为了
**展示一个缺口**：它断言 `store.put("e_cal.noul", 0.78, 0.22, 73, "上岗")`——一条没
有证书背书的手写阈值——会成功，并把记录推上「上岗」，因为写这条测试的时候，内核里没
有任何东西拦得住它。这个缺口后来被工作区内核里第四节说的那道证书门堵上了：`put` 现在
拒绝没有证书背书的上岗。测试本身还断言着旧的、现在已经错误的预期，所以它变红——这是
对的。**一条变红的测试有两种可能，而它们在输出上长得一模一样：「缺口还在」，与
「测试当初要展示的缺口已经被堵上，只是测试没有跟着改」。** 这一条是第二种，而且是故意
带着这个状态发布出来的——如果先把断言悄悄改对再发布，就抹掉了「修法真的落地了」这唯一
的证据。

**同一件事的一个更小版本：这个 crate 本身需要一处一行修法才能编译。** 设计文自己写的
「10 passed」，在它点名的那个 `jpp-core` 提交（`b3747a0`）上是真的。此后 `jpp-core`
把 `Sample` 的 `cluster` 字段改成了必填（正是第四节设计自己主张的分簇纪律），原型的三
处测试字面量因此需要补上这个字段才能编译。**改一个机制，不会自动传播到依赖它的树**
——这与「一条文档限定不会自动跟着引用它的数字走」（第一节）是同一个形状的缺口，只是
换到了代码这一层。

（后续小节——证据在哪、本仓库验证、工作区内核上复跑、下一个设计问题——见上方英文
版，数字与结论完全一致，不再重复。）
