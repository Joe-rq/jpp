# ADR 0001: Rust kernel and standalone J++ source / Rust 内核与独立源码

Date / 日期: 2026-09-20. Status / 状态: accepted implementation direction;
construction is starting, not a delivered Rust runtime.

## Decision / 决定

J++ will use one Rust implementation for its formal kernel. Users will write
`.jpp` source, which is parsed, checked and interpreted by that runtime. They will
not need to write Python or Rust to compose methods in the language. The first
milestone is interpreted execution; machine-code compilation is not a prerequisite.

正式内核选择 Rust。用户编写 `.jpp` 源码，经解析和语言自身的类型/效应检查，再由
Rust 内核运行。第一版采用解释执行。这个决定替代此前将独立文法无限期推迟、继续
以 Python 构建器为唯一入口的施工安排；旧规范保留为历史材料。

## Why / 原因

The Python implementation has provided executable examples of first-class
questions, methods, composition and explicit uncertainty. The next step is to
give those rules an explicit program representation and an independent source
language in a single distributable implementation. Rust is the chosen engineering
direction; its own type system does not replace the J++ checker.

已有 Python 程序给出了行为依据。现在把这些规则落实到独立源码、显式程序结构与
统一执行系统，用户才能直接用 J++ 编程。Rust承担正式内核；针对具体设计问题可以
开展OCaml实验，约定见下节。不预先并建第二套完整内核，也不逐行搬运全部旧代码。

## Rust mainline and OCaml experiments / Rust 主线与 OCaml 实验

Rust remains the formal runtime and delivery implementation. OCaml may be used to
explore a specific grammar or semantic question with small, complete runnable
programs—for example, whether a continuation receives state or carries a remaining
program, and how unresolved items propagate through another composition. The tool
choice itself does not establish that a language design is better.

Rust承担正式运行与交付；OCaml可围绕具体文法、组合、执行或未决处理问题开展实验。
每次从明确问题出发，用小而完整的程序比较方案。不预先建设第二套完整内核、不重复
一般性选型，也不暂停不受影响的Rust开发。

Selected rules enter the shared, implementation-independent specification with
examples, expected behavior and reasons. Rust must reproduce those behaviors;
essential rules cannot exist only in OCaml code. Rejected alternatives may remain
as clearly labeled experiment records.

选中的规则进入独立于实现语言的共同规范，附程序、预期行为和选择依据，再由Rust
复现。未选方案与取舍可以保留，但明确标为实验记录。

An OCaml reference program can be retained after its rule is implemented in Rust.
If an OCaml part, such as a parser or checker, is worth retaining as a formal
component, define its responsibility, program/data interface, build and runtime
path, then verify the complete integration. Experiment dependencies do not
automatically become user installation requirements.

成熟实验可保留作参考；若某部分值得正式保留为OCaml组件，也可以另行明确职责、
程序结构或数据接口、构建运行方式，并验证完整路径。不能默认把实验依赖带入用户安装。

Experiments, selected rules and implemented behavior are labeled separately and
published with the existing bilingual progress practice. This update records the
policy only: it does not start an OCaml experiment, install a toolchain or claim a
completed mixed-language system. 实验随真实设计问题安排，本轮只记录规范，不启动安装或实验。

## Preserve and build / 保留与建设

The published Python packages and demonstrations remain available as behavior
references, experiment/calibration tools and necessary adapters. New formal
kernel and language-interface development moves to Rust; history is not deleted.
In particular, [partial results and continuation](../partial-results.md) provide
reference behavior for the migration.

保留问题/方法可传递、组合后继续组合、显式未决与续接、精确计算与模型观察分离、
材料来源和执行记录。Python 已发布结果继续可运行；正式内核与语言接口的新建设
转到 Rust，不继续扩大 Python 专属能力。

## First acceptance milestone / 第一整包验收

- `.jpp` source → explicit program → checking → Rust execution and readable output.
- Adaptive question selection expressed in source, including questions/methods passed as values.
- Partial candidate validation followed by exact constraint combination, then a replacement continuation strategy, retaining prior checks and observation identities.
- Source-position diagnostics, native build/run instructions and fixed-observation comparison against the retained implementation.

完整路径必须实际运行，两类算法共用语言结构。源码不能只调用隐藏的 Python 整程序
或领域专用命令。固定观察验证组合行为；真实模型质量另行评估，不因迁移重复付费实验。

## Current status / 当前状态

This commit records the decision and updates the design entry points only. The
Rust lexer/parser, common program representation, checker, runtime and CLI are
being built together under `rust/`; they are not claimed as available by this ADR.
Delivered behavior will be documented with commands, results and commit links.

本次提交只公布路线和验收目标。Rust 工程正在开始建设；本页不表示 Rust 运行时或
独立源码执行已经交付。后续实际进展继续以代码、运行结果和提交链接公开。
