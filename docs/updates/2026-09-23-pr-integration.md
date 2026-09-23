# PR integration review / PR 整合审查

2026-09-23. Reviewed the four open PRs in isolated public-repository checkouts.
No paid model calls or unrelated research-workspace edits were made.

| PR | Review outcome / 审查结果 |
|---|---|
| [#17](https://github.com/Towow-ai/jpp/pull/17) | Community Python examples use synthetic fixtures; targeted tests pass. Retained as examples, not new formal language primitives. / 社区示例保留，不改变 Rust 正式内核定位。 |
| [#25](https://github.com/Towow-ai/jpp/pull/25) | Fixed public profile paths and portable legacy fixtures; regenerated the browser bundle. Reproduced and fixed binomial-CDF underflow and reject-all threshold clamping. / 修复公开副本可运行性及两处数值错误。 |
| [#20](https://github.com/Towow-ai/jpp/pull/20) | Kept newer authority documents when resolving conflicts. The three original defects are covered by `v13_rules.rs`; an enabled test now requires the precise overflow runtime error and source span. / 保留现行规范，清理过期忽略复现并回应审查意见。 |
| [#26](https://github.com/Towow-ai/jpp/pull/26) | Preserves dated research findings while updating the current map and handoff to the merged public code. / 历史记录保留，当前说明对齐最新公开代码。 |

## Verification / 验证

- PR #25 isolated Rust suite: 304 passed, 3 ignored. The two numerical regressions failed before their fixes and passed afterward.
- PR #20 adds one enabled test: the combined Rust suite has 305 passing tests, 3 ignored. Its source-span test and `v13_rules` pass in both debug and release.
- Rust and Python 3.12/3.13 checks pass on the combined PR #20 branch: [CI run](https://github.com/Towow-ai/jpp/actions/runs/35811645710). Rust CI also executes adaptive and partial-result source examples.
- Local relative links in changed publication documents were checked. Final documentation-branch CI remains visible on #26.

本轮验证针对公开仓库。研究树 315 项是另一个快照，不替代公开版验证。三项忽略和
缺少私有数据时提前返回的历史探针，不构成新真机实验通过；没有测量模型质量或生产收益。

## Remaining boundaries / 剩余边界

`certify` selects thresholds using the same samples as its pointwise binomial bounds.
No selection correction or independent holdout validation is implemented. Costed
commissioning checks different dataset IDs but still uses the same stored samples;
IDs alone do not prove independence. The existing certificate API is experimental,
and its synthetic regression does not establish general finite-sample risk control.

统计路径可以作为实验宿主能力合入，但不宣称一般风险保证。下一次研究/公开同步应
保留本轮数值修复和可移植测试，逐项核对增量，避免整树覆盖。真实后端 CLI 与可复用
源码算法仍按现有任务推进，本轮没有另起语言重构。
