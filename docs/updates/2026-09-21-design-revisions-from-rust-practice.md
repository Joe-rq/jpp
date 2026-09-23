# 2026-09-21 update: six design revisions fed back from the Rust implementation / 实践反馈回设计的六条修订

> **2026-09-23 merge-time note / 合并时订正：** The body below preserves the original
> September 21 snapshot. PR #25 has since published the runtime fixes and regression
> tests in `v13_rules.rs`; the old three ignored reproducers are superseded.
> `known_defects.rs` now checks the exact runtime error and source span, and runs by
> default. Run `cargo test -p jpp-core --test v13_rules --test known_defects` for the
> current checks. 下文 41 / 69 项、尚未合入及三条失败均为历史状态，不是当前状态。
> 当前实现范围见 [status](../status.md)。

Dated update following the practice in `progress.md`. This one is a **specification increment plus three reproduced defects**, not an implementation announcement. Every number in "Verification in this repository" was re-run on the committed files in this repository on the date above. The rules below are decided; the implementation status column says, rule by rule, what the kernel actually does today.

## What changed for a user / 用户可见的变化

- **The specification now has a current revision layer.** [`research/地基/13-Rust实践反馈设计修订-v0.2.md`](../../research/地基/13-Rust实践反馈设计修订-v0.2.md) carries six local revisions fed back from a day of writing the Rust kernel. Where it conflicts with the IR and class contract (`12`), it wins; everything else in `12` stands. This is an increment to the existing design, not a second language.
- **A stale line in the published specification is fixed.** The public copy of `12` still opened with "the surface grammar is deferred indefinitely." Standalone `.jpp` source has been running natively since PR #12, so that sentence was simply wrong for readers. It now points at the current revision.
- **Three known defects are now reproduced in the repository instead of living in a review thread.** They come from the [PR #12 review](https://github.com/Towow-ai/jpp/pull/12#discussion_r4057143623): a method's identity ignores what it captured, a completed model call is thrown away when its actual cost exceeds the budget, and integer overflow behaves differently in debug and release builds. `rust/crates/jpp-core/tests/known_defects.rs` reproduces all three with local stand-ins and zero paid calls. The tests are `#[ignore]`d so the suite stays green; run them with `--ignored` and you get three honest failures. Each `#[ignore]` comes off when its rule is implemented.
- **Nothing in the kernel changed in this update.** The six rules are scheduled work. Rules 1 and 2 are already largely implemented; rules 3 to 6 are not, and the table below says which is which.

**中文摘要。** 这一版是规范增量加三条缺陷复现，不是实现完成的声明。写了一天 Rust 内核得到的经验回灌成六条局部设计修订，写进公开研究目录的 `13` 号文件；与 `12` 冲突处以 `13` 为准，其余沿用。公开的 `12` 号顶部原本还写着「新文法无限期推迟」，与已经在跑的独立 `.jpp` 源码矛盾，一并更正。PR #12 审查提出的三条缺陷不再只躺在评审串里：`known_defects.rs` 用本地替身把它们复现出来，零付费调用，三条都标了 `#[ignore]` 所以测试套仍然全绿，加 `--ignored` 就能看见它们怎么失败；哪条修好就摘掉哪条的标记。本次没有改内核。

## The six rules and where the kernel stands / 六条规则与内核现状

| Rule / 规则 | What the specification now requires / 设计规则 | Implementation status / 实现状态 | Verified by / 验证结果 |
|---|---|---|---|
| 1. Constructing a method is not calling it / 方法的构造与调用分开 | Creating a method value does not run its body. Its effects belong to the future call, whether it is passed, stored in a record, or returned. | **Implemented** in the workspace kernel (packages three and four), **not yet in the published `rust/`** | Workspace effect-row tests; the published snapshot still over-reports here |
| 2. Effects belong to the method contract, instantiated per call site / 效应属于方法契约，并在调用处实例化 | An unannotated higher-order method infers its effect row from its method arguments and instantiates it at each call site; one `judge`-carrying call must not contaminate a pure one. An explicit annotation is an upper bound the caller may rely on. | **Implemented** in the workspace kernel as rank-1 effect polymorphism, **not yet in the published `rust/`** | Workspace tests; the published `INTERFACE.md` §七.5 still says `!{…}` on a higher-order method is documentation only — that sentence is now out of date and will be corrected when the packages merge |
| 3. An unresolved exit moves by where it actually goes / 未决结果按实际去向传递 | Entering an `unsure` arm no longer discharges the obligation. Handling it, passing it on, dropping it explicitly, or carrying it out in a return value does. Lists, records, captured environments and suspended state must not let it vanish silently. | **Partly implemented**: `Value::Duty` makes the rule real for arms in the workspace kernel; the container, capture and suspend paths are not done | Workspace duty tests; no published test covers the remaining paths |
| 4. A method's identity includes what it captured / 方法身份包括实际捕获状态 | A reusable result may not be keyed on the code body alone. A capture that cannot be fingerprinted disables cross-run caching for that entry rather than returning another method's result. | **Not implemented**; in progress in the kernel workspace | `known_defects.rs`: a factory returns two methods with the same body capturing `n=1` and `n=2`; `transform` on the same material returns 1 for both |
| 5. Budget check before the call, fact accounting after / 调用前预算与调用后事实记账分开 | After the backend returns, record the request identity, the result and the actual cost first, then decide whether to continue. Exceeding the budget may stop further computation; it may not erase a call that already happened. | **Not in this snapshot; implemented in the kernel workspace after this update was written** | `known_defects.rs`: with a stand-in returning a cost above the budget, the request is made, the ledger stays empty, and resuming with that ledger pays again |
| 6. Integer behaviour does not depend on the Rust build profile / 整数行为不随 Rust 构建模式改变 | Overflow, division by zero and the related edges return a runtime error pointing at the `.jpp` source, never a Rust panic or a silent wrap. | **Not in this snapshot; implemented in the kernel workspace after this update was written** | `known_defects.rs`: `i64::MAX + 1` panics with `attempt to add with overflow` in a debug build (`interp.rs:496`) |

**A note on timing, because it matters for reading the table.** The reproductions and the status column describe the published snapshot in this repository, which is what a reader can check out and run. While this update was being written, the kernel workspace implemented rules 5 and 6 and started on rule 4. The defects are therefore real here and fixed there; the `#[ignore]` markers come off as those packages merge. Stating implementation status from a document rather than re-reading the code is exactly how this kind of claim goes stale.

**关于时点。** 表里的实现状态和三条复现说的是本仓库的公开快照——读者能检出、能跑的那份。写这份更新期间，内核工作区已经实现了规则 5 与规则 6，规则 4 在做。也就是说缺陷在这里是真的，在那边已经修了；那些包合进来时，对应的 `#[ignore]` 就摘掉。实现状态这类主张只能重核代码，不能只引文档。

Rules 1 and 2 are the one place where the published snapshot is behind the workspace rather than behind the specification. The two kernel packages that implement them are finished and green in the workspace but have not been merged into public `rust/` yet; the merge will also correct `INTERFACE.md` §七.5.

## Demonstrate / 演示

```sh
cd rust
cargo test --locked --workspace                                   # 41 passed, 0 failed
cargo test -p jpp-core --test known_defects -- --ignored          # 3 failed, on purpose
cargo run -p jpp-cli -- run examples/composition.jpp              # 43
cargo run -p jpp-cli -- run examples/adaptive.jpp --fixtures examples/fixtures/adaptive.json
```

## Verification in this repository / 本仓库验证

| Check | Result |
|---|---|
| `cargo test --locked --workspace` under `rust/` | 41 passed, 0 failed, 4 ignored (3 known defects + 1 doc test) |
| `cargo test -p jpp-core --test known_defects -- --ignored` | 3 failed — one per rule 4, 5, 6, each with the concrete wrong value |
| Rule 4 reproduction | captured `n=2` produces `1`, the cached output of the `n=1` method |
| Rule 5 reproduction | stand-in received 1 request; ledger length 0 after the run; second run issues the request again |
| Rule 6 reproduction | `attempt to add with overflow` at `crates/jpp-core/src/interp.rs:496`, debug profile |
| Paid model calls in this update | none |

Not re-run here, stated as a workspace measurement: the kernel workspace reports 69 passing tests across four packages, including the two packages behind rules 1 and 2. Those packages are not in this repository yet, so the published number remains 41.

未在本仓库复跑、按工作区测量如实记录：内核工作区四包共 69 项通过，其中两包实现了规则 1 与规则 2。它们尚未并入本仓库，所以公开数字仍是 41。

## Next design question / 下一个设计问题

Rule 4 asks for a method identity that is stable across runs, and the honest answer for some captured environments is "cannot be fingerprinted." The open question is what the language should then do: disable cross-run reuse for that entry silently, warn at the call site, or refuse the program at check time. Refusing is the safest and the most likely to reject programs people reasonably want to write — a recursive continuation captures its own environment, and that is exactly the shape `partial.jpp` already uses.

规则 4 要一个跨运行稳定的方法身份，而对某些捕获环境，诚实的答案是「指纹化不了」。未定的是语言此时该做什么：对该项静默关掉跨运行复用、在调用点告警、还是在检查期直接拒绝程序。拒绝最安全，也最可能拦住人们本来就该写的程序——递归续接方法捕获的正是它自己的环境，而 `partial.jpp` 用的就是这个形状。
