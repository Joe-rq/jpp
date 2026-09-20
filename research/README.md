# Research workspace copy / 研究工作区副本

This directory is a curated copy of the language research workspace (`地基`, "the foundation") as of **2026-09-20**. It contains the constitution, the requirements list, the design trail (语言本质 → 组合代数 → 语言规范 v1.1 → IR 与类契约 v0.1), the hypothesis ledger with every claim's status, the running decision log, red-team rounds, zero-context reader trials, pre-registrations and results of every experiment, and the review exchange with the composition-library work. Documents are in Chinese; they are the primary record, not a summary. Source-of-truth documents (constitution, specification, ledger, work orders) are changed only by the project owner; agents and contributors may propose changes as appended notes. Raw model call records, run outputs, private conversation transcripts and agent-to-agent collaboration notes are deliberately not included. `src/foundation/profile/run.py` and `build_from_raw.py` call the live model: they read `~/.typesafe-key` and cost money; pre-register before running. Kernel source synced at the same time: `src/foundation/jv` (package fingerprint `4f7a31bc48fa`, sha256 of the concatenated module sources, first 12 hex digits).

本目录是语言研究工作区「地基」在 **2026-09-20** 的整理副本：宪法、需求与启发清单、设计脉络（语言本质 → 组合代数 → 语言规范 v1.1 → IR 与类契约 v0.1）、逐条记录状态的假设账本、决策日志、六轮红队、六轮零上下文读者、全部实验的预注册与结论、与组合库工作的评审往来。文档是中文原件，不是摘要。依据文本（宪法、规范、账本、施工单）只由项目负责人 Nature 修改，代理与贡献者只能以只增附注的形式提议。原始模型调用记录、运行输出、私人对话与代理间协作附注不在此目录。`src/foundation/profile/run.py`、`build_from_raw.py` 会读 `~/.typesafe-key` 跑真机并付费，跑前先预注册。同步脚本：`tools/sync-from-workspace.sh`。

Start with [language design and grammar](../docs/design.md) for the current builder, the archived EBNF and their different statuses. / 先读[语言设计与文法](../docs/design.md)，区分当前构建器与历史 EBNF。

| Path | What it is |
|---|---|
| `地基/00-宪法.md` | Constitution: build from mechanism, register every borrowing with its original conditions, probes are not targets |
| `地基/需求与启发清单.md` | Requirements A0–G5 from the owner |
| `地基/05`–`08`, `10` | Essence of the language, the three judgment types, guarded commands and the literalization law, composition algebra |
| [地基/11-语言规范-v1.md](地基/11-语言规范-v1.md) | Language spec v1.1 (archived as a design study; the surface grammar is deferred) |
| [地基/12-IR与类契约-v0.1.md](地基/12-IR与类契约-v0.1.md) | Current authority: class contract, six-form IR, checker rules J-01…J-18, passes, Python builder, build order |
| `地基/09-研究方法与假设账本.md` | Method, axioms P1–P25, claims #1–#38 with status, update log |
| `地基/DECISIONS.md`, `GATES.md` | Decision log and gates |
| `地基/设计/` | Competing designs, 100-task stress tests, zero-context reader rounds G1–G6, independent advisor, judges |
| `地基/红队/`, `研究/` | Red-team rounds, prior-work studies (collaboration notes between agents are not included) |
| `地基/foundation/experiments/` | `EXPERIMENTS.md` (pre-registrations) and `前提结论.md` (results, including E9f) |
| `扩展/codex_composition/` | Composition-library handoff and the kernel-side review reply |
