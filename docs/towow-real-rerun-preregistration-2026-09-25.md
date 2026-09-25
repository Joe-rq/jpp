# Pre-registration: rerunning the real-source demo after the abstract trim / 预注册：删摘要后重跑真实来源演示

2026-09-25. Written and committed before any paid JEV call for this rerun, per this
project's experiment discipline (地基 §5.2: pre-register a falsifiable prediction
before spending on a real measurement). `docs/demos/towow/real/data.json` at commit
`7bc65aa` (sha256 `8116309e18d2bb4e90d475a5e5531542dad852ec4a24b3581d9230aa29e7bce1`)
is the version this rerun measures against: 178 academic-source `context` fields
had their paper abstract replaced with a hand-written, per-title one-sentence
description; the other 147 GitHub/startup records and all 963 `known_relations`
edges are untouched.

2026-09-25，在这次重跑的任何一次付费 JEV 调用之前写下并提交，按项目实验纪律（地基 §5.2：
花钱做真实测量前先预注册可证伪的预测）。本次重跑测的是 `7bc65aa` 提交的
`docs/demos/towow/real/data.json`（sha256 `8116309e18d2bb4e90d475a5e5531542dad852ec4a24b3581d9230aa29e7bce1`）：
178 条学术来源的 `context` 已把论文摘要换成手写的逐标题一句话描述，另外 147 条
GitHub/创业记录与全部 963 条 `known_relations` 边未动。

## System-level hypothesis being tested / 被检验的系统级假设

From `research/地基/DECISIONS.md:1005` (already public in this repository): removing
the shared-abstract text is expected to weaken relation-recovery recall specifically
for the two relation types whose labels were originally inferred from shared paper
text or shared employer text (`coauthor_same_field`, `github_same_org`), leaving
only a weaker "same field" signal.

来自本仓库已公开的 `research/地基/DECISIONS.md:1005`：删除共同摘要文本预计会削弱
`coauthor_same_field`（599 条）和 `github_same_org`（307 条）两类关系的召回，因为
这两类标签本来就是从共同论文文本 / 共同雇主文本推出的，删掉之后只剩「领域接近」
这层弱信号。

**This pre-registration commits to a more specific, and in one place diverging,
prediction**, based on inspecting the actual data rather than the label names alone:

**本次预注册基于实际检查数据（而不只是标签名字）给出更具体、且有一处与上述预期方向不同的预测**：

- All 599 `coauthor_same_field` edges connect two people whose `context` contains
  the *identical* paper title string (verified programmatically: 599/599). The
  trim keeps paper titles verbatim. So unlike the general "shared text was
  deleted" framing, the single strongest signal behind this relation type --
  an exact, distinctive title match -- is still present, undiluted by the
  removed abstract text. **Prediction: this relation type's recall is NOT
  expected to drop materially, and may even rise slightly** (removing ~25% of
  surrounding text can only concentrate lexical/embedding weight on what's left,
  including the shared title).
  全部 599 条 `coauthor_same_field` 边的两端 `context` 都含有完全相同的论文标题字符串
  （已程序核实 599/599）。删摘要保留标题原文，所以这类关系背后最强的信号——精确匹配的
  独特标题——并未被删，反而因为周围文本变短而相对更突出。**预测：这类关系的召回不会
  明显下降，甚至可能略升。**
- All 307 `github_same_org` edges connect two GitHub-source profiles; GitHub
  contexts were not touched by this edit at all. **Prediction: this relation
  type's recall stays within noise of its pre-trim baseline** -- any material
  move would mean the edit had an indirect effect on unrelated rankings (e.g.
  a shift in the shared embedding space), which would itself be a real finding
  worth a follow-up, not something this prediction currently explains.
  全部 307 条 `github_same_org` 边两端都是 GitHub 来源资料，这次改动完全没碰
  GitHub 的 `context`。**预测：这类关系的召回停留在删摘要前基线的噪声范围内**；
  如果明显偏离，说明这次改动对无关排序产生了间接影响（比如共享嵌入空间发生了整体
  偏移），这本身会是一个值得追查的新发现，不在本预测的解释范围内。
- All 33 `same_team_*` edges connect two startup-source profiles, also untouched.
  **Prediction: recall stays at or very near its pre-trim baseline (already at
  or near ceiling).**
  全部 33 条 `same_team_*` 边两端都是创业来源资料，同样未被改动。**预测：召回率
  停留在删摘要前的基线附近（该基线本已接近或等于满分）。**
- All 24 `cross_source_*` edges connect one academic and one GitHub profile, and
  already scored 0/24 (0%) recall at K=10 on every method before this edit --
  a floor, not a baseline to fall from. **Prediction: recall stays at or near 0,
  cannot meaningfully "drop" further.**
  全部 24 条 `cross_source_*` 边一端学术、一端 GitHub，删摘要前所有方法在 K=10
  上已经是 0/24（0%）召回——这是地板，不是会往下掉的基线。**预测：召回停留在 0
  附近，没有进一步「下降」的空间。**

## Falsifiable numeric predictions, order20 (the page's default method) / 可证伪的数值预测（order20，页面默认方法）

Pre-trim baseline (from the currently published `results.json`, `order20` method,
undirected recall unless noted):

删摘要前基线（来自已发布 `results.json` 的 `order20` 方法，未注明则为无向召回）：

| K | overall undirected | overall directed | coauthor_same_field | github_same_org | same_team_* (33) | cross_source_* (24) |
|---|---|---|---|---|---|---|
| 1  | 0.1900 | 0.1277 | -- | -- | -- | -- |
| 5  | 0.5348 | 0.4273 | -- | -- | -- | -- |
| 10 | 0.7238 (697/963) | 0.6153 | 0.9048 (542/599) | 0.3974 (122/307) | 1.0000 (33/33) | 0.0000 (0/24) |
| 20 | 0.8619 | 0.8006 | -- | -- | -- | -- |

Predicted ranges after the rerun, same method and K, with the falsification line
for each (a result outside the stated range means this specific prediction is
wrong, not the whole hypothesis):

重跑后的预测区间（同方法、同 K），并给出每条的推翻线（结果落在区间外即判该条预测
不成立，不代表整体假设一并作废）：

| Metric (order20, K=10) | Predicted range | Falsification line |
|---|---|---|
| `coauthor_same_field` recall | **0.83 - 0.95** | Below 0.80 (materially dropped despite intact title match -- the "title dominates" explanation would be wrong) or above 0.97 (unexplained further gain) |
| `github_same_org` recall | **0.35 - 0.44** | Outside 0.30 - 0.48 (an untouched relation type moved outside baseline noise -- indirect/systemic effect, worth its own investigation) |
| `same_team_*` recall (33 edges) | **0.90 - 1.00** | Below 0.85 (an untouched relation type regressed) |
| `cross_source_*` recall (24 edges) | **0.00 - 0.15** | Above 0.20 (a floor metric moved enough to need explaining, though this direction would be a welcome surprise, not a problem) |
| overall undirected recall@10 | **0.68 - 0.76** | Below 0.65 or above 0.78 (the per-type predictions above don't compose the way this pre-registration expects) |
| overall directed recall@10 | **0.57 - 0.66** | Below 0.54 or above 0.68 |
| overall undirected recall@1/@5/@20 | within **±0.05** of the 0.19 / 0.5348 / 0.8619 baselines respectively | any point outside that band |

`cut20` and `cut324` (same underlying `context` text, different candidate limit
and no independent-ordering step) are expected to move in the same direction and
rough magnitude as `order20` per relation type; this pre-registration does not
commit separate numeric ranges for them to keep the registered claim set
tractable, but the full per-type breakdown for all methods will be reported
alongside the `order20` numbers once the rerun completes.

`cut20`、`cut324`（同样的 `context` 文本，只是候选数量上限和是否做独立排序不同）
预计各关系类型的变化方向与幅度与 `order20` 大体一致；为了让预注册的断言集合可控，
本次不单独给这两个方法定数值区间，但重跑完成后会把所有方法的分类型明细和 `order20`
的数字一起报出来。

## No "precision" metric exists to predict / 没有可预测的「精确率」指标

`src/jpp/towow_real.py::evaluate()` only computes fixed-K recall (undirected,
directed, and a source-backed-only subset); its own documentation states
"无已知边不能当作真实负例，暂不计算完整 precision" (absent labels are not
negatives, so full precision is not computed). This pre-registration therefore
predicts recall and the source-backed hit count, not "precision/recall" as a
pair -- there is no precision number in this harness's output to check against.

`src/jpp/towow_real.py::evaluate()` 只算固定 K 的召回（无向、有向，以及仅
source-backed 子集），其自身文档写明「无已知边不能当作真实负例，暂不计算完整
precision」。所以本预注册只预测召回和 source-backed 命中数，不预测所谓「精确率/
召回率」二元组——这套评测本身就不产出精确率数字可供核对。

`source_backed` (939 edges, K=10, order20 baseline 697/939 = 0.7423): predicted
range **0.71 - 0.79**, same falsification logic as overall undirected recall
above (it is dominated by the same `coauthor_same_field` + `github_same_org`
edges).

`source_backed`（939 条，K=10，`order20` 基线 697/939 = 0.7423）：预测区间
**0.71 - 0.79**，推翻逻辑同上面的整体无向召回（主要由同样的 `coauthor_same_field`
和 `github_same_org` 边构成）。

## What happens after the rerun / 重跑之后怎么做

The actual per-type and overall numbers, compared against these predicted ranges,
will be reported on PR #33 once the rerun completes and `build_towow_real.py`
regenerates `results.json`. Predictions confirmed or falsified are reported as
such, not adjusted after the fact -- this file is not edited to match the result;
a follow-up note records the comparison instead.

重跑完成、`build_towow_real.py` 重新生成 `results.json` 后，会把实际的分类型与
整体数字对照上面的预测区间，报在 PR #33 上。命中还是被推翻都如实报，不事后改这份
文件去迁就结果——对照结果另写一条跟进记录。
