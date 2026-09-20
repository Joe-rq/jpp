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
统一执行系统，用户才能直接用 J++ 编程。此次选择 Rust，不再并建 OCaml 或另一套
内核，也不逐行搬运全部旧代码。

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
