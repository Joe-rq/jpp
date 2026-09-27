# Research workspace copy / 研究工作区副本

This directory is a curated copy of the language research workspace (`地基`, "the foundation") as of **2026-09-21**. It contains the constitution, the requirements list, the design trail (语言本质 → 组合代数 → 语言规范 v1.1 → IR 与类契约 v0.1), the hypothesis ledger with every claim's status, the running decision log, red-team rounds, zero-context reader trials, pre-registrations and results of every experiment, and the review exchange with the composition-library work. Documents are in Chinese; they are the primary record, not a summary. Source-of-truth documents (constitution, specification, ledger, work orders) are changed only by the project owner; agents and contributors may propose changes as appended notes. Raw model call records, run outputs, private conversation transcripts and agent-to-agent collaboration notes are deliberately not included. `src/foundation/profile/run.py` and `build_from_raw.py` call the live model: they read `~/.typesafe-key` and cost money; pre-register before running. This round synced research documents only; the Python kernel under `src/foundation/jv` was already identical to the workspace and was not re-copied (package fingerprint `93dd4ab507ff`, sha256 of the concatenated module sources, first 12 hex digits — re-measured on 2026-09-21 in both trees; the figure recorded here previously was stale). The Rust kernel under `rust/` is advanced on a separate line and is not synced from this workspace.

本目录是语言研究工作区「地基」在 **2026-09-21** 的整理副本：宪法、需求与启发清单、设计脉络（语言本质 → 组合代数 → 语言规范 v1.1 → IR 与类契约 v0.1）、逐条记录状态的假设账本、决策日志、六轮红队、六轮零上下文读者、全部实验的预注册与结论、与组合库工作的评审往来。文档是中文原件，不是摘要。依据文本（宪法、规范、账本、施工单）只由项目负责人 Nature 修改，代理与贡献者只能以只增附注的形式提议。原始模型调用记录、运行输出、私人对话与代理间协作附注不在此目录。`src/foundation/profile/run.py`、`build_from_raw.py` 会读 `~/.typesafe-key` 跑真机并付费，跑前先预注册。同步脚本：`tools/sync-from-workspace.sh`。

Start with [language design and grammar](../docs/design.md) for the current builder, the archived EBNF and their different statuses. / 先读[语言设计与文法](../docs/design.md)，区分当前构建器与历史 EBNF。

| Path | What it is |
|---|---|
| `地基/00-宪法.md` | Constitution: build from mechanism, register every borrowing with its original conditions, probes are not targets |
| `地基/需求与启发清单.md` | Requirements A0–G5 from the owner |
| `地基/05`–`08`, `10` | Essence of the language, the three judgment types, guarded commands and the literalization law, composition algebra |
| [地基/11-语言规范-v1.md](地基/11-语言规范-v1.md) | Language spec v1.1 (archived as a design study; the surface grammar is deferred) |
| [地基/12-IR与类契约-v0.1.md](地基/12-IR与类契约-v0.1.md) | Authority: class contract, six-form IR, checker rules J-01…J-18, passes, build order |
| [地基/13-Rust实践反馈设计修订-v0.2.md](地基/13-Rust实践反馈设计修订-v0.2.md) | Second authority, 2026-09-21: six local revisions forced by building the Rust kernel; `13` wins wherever it conflicts with `12` |
| [地基/14-实施计划-把语言做完整-v1.md](地基/14-实施计划-把语言做完整-v1.md) | Implementation plan, not a design: what remains, what this version deliberately excludes and why, and 22 places where taste had been recorded as mechanism |
| `地基/09-研究方法与假设账本.md` | Method, axioms P1–P25, claims #1–#38 with status, update log |
| `地基/DECISIONS.md`, `GATES.md` | Decision log and gates |
| `地基/设计/` | Competing designs, 100-task stress tests, zero-context reader rounds G1–G6, independent advisor, judges |
| `地基/红队/`, `研究/` | Red-team rounds, prior-work studies (collaboration notes between agents are not included) |
| `地基/foundation/experiments/` | `EXPERIMENTS.md` (pre-registrations) and `前提结论.md` (results, including E9f) |
| `扩展/codex_composition/` | Composition-library handoff and the kernel-side review reply |

## Standalone write-ups / 独立研究报告

These are curated, bilingual write-ups published directly into `research/` (not part of the synced `地基/` copy above, and not touched by `tools/sync-from-workspace.sh`). Internal-only paths and materials have been replaced with public equivalents or marked "internal record"; conclusions, including failed predictions and known gaps, are kept as-is.

以下是直接发布在 `research/` 下的独立整理报告（双语），不属于上面同步的 `地基/` 副本，`tools/sync-from-workspace.sh` 不会碰它们。仅本机可见的路径与材料已替换为公开对应物或标「内部记录」；结论（含预测不成立与已知缺口）照原样保留，不作美化。

| Path / 路径 | What it is / 是什么 |
|---|---|
| [18-repetition-in-real-jev-projects](18-repetition-in-real-jev-projects.md) · [中文](18-repetition-in-real-jev-projects.zh-CN.md) | What repeated "busywork" looks like across 84 real JEV projects plus a 100-repo ecosystem sample, and which J++ mechanisms take over which category / 84 个真实 JEV 项目加 100 个生态抽样里反复出现的「杂活」是什么、J++ 哪些机制接管了哪一类 |
| [19-what-the-rewrites-cut](19-what-the-rewrites-cut.md) · [中文](19-what-the-rewrites-cut.zh-CN.md) | Reading the ~60% of judgment-core lines the J++ rewrites cut, block by block, to see whether it's busywork or composition that's really being repeated / 逐块读改写压掉的约六成判断核心行，回答「重复的是杂活还是组合方式」 |
| [2026-09-26-rewrite-study](2026-09-26-rewrite-study.md) · [中文](2026-09-26-rewrite-study.zh-CN.md) | Field-by-field rewrite comparison across 84 real JEV projects: equivalence rate, code-volume ratio under two scopes, call counts, and real defects found in unmodified original code; live version at [jpp.towow.net/rewrite-study](https://jpp.towow.net/rewrite-study/) / 84 个真实 JEV 项目逐字段改写对照：等价率、两种口径的代码量倍率、调用次数，以及在未改动原代码里发现的真实缺陷；同一数据的可交互版见 [jpp.towow.net/rewrite-study](https://jpp.towow.net/rewrite-study/) |

Supporting data tables are in `research/data/2026-09-27-repetition/`, `research/data/2026-09-27-what-got-cut/`, and `research/data/2026-09-26-rewrite-study/`; the scripts that produced them are in `research/scripts/` (each script's docstring notes that it reads a private-workspace corpus not published with this release — the published CSVs are the reproducible artifact).

这些报告用到的数据表在 `research/data/2026-09-27-repetition/`、`research/data/2026-09-27-what-got-cut/`、`research/data/2026-09-26-rewrite-study/`；产出它们的脚本在 `research/scripts/`（每个脚本的文档字符串都注明它读取的是未随本次公开的私有工作区语料库，已发布的 CSV 是可复现引用的产物）。

这份副本由 `tools/sync-from-workspace.sh` 从工作区生成，**不要手改**——手改的内容下次同步会被静默盖回去。
工作区里不适合公开的行，在它的上一行写一条单行 HTML 注释标记：一种把下一行替换成给定的公开版本，
一种把下一行整条删掉；同步脚本在复制之后执行它们，`tools/sync-from-workspace.sh --self-test` 可以验证这一步还工作。

This copy is generated by `tools/sync-from-workspace.sh` and **should not be edited by hand** — the next sync
would silently overwrite the edit. Lines that must not be published are marked in the workspace with a
single-line HTML comment on the line above: one form replaces the following line with a public version, the
other drops it. The sync script applies them after copying; `--self-test` checks that step still works.
