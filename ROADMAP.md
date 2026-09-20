# Roadmap / 路线图

Updated 2026-09-21. Milestones describe observable results, without speculative release dates.

| Milestone / 阶段 | Status / 状态 | Acceptance / 完成标准 |
|---|---|---|
| Reproducible Python reference / 可复现 Python 对照 | Delivered / 已交付 | Installable package, examples and CI; retained for experiments |
| Standalone source on Rust / 独立源码与 Rust 执行 | Delivered / 已交付 | `.jpp` → shared checking → execution; complete algorithms, native installation and replay. [PR #12](https://github.com/Towow-ai/jpp/pull/12) |
| Reusable source methods / 可复用源码方法 | Next / 下一步 | Extract shared observation/continuation methods into a source library; two programs reuse it without copying definitions or changing the runtime |
| Composition rules / 组合规则统一 | Next / 下一步 | Resolve the [documented core/spec differences](rust/crates/jpp-core/INTERFACE.md#七当前实现与历史规范的差异); use executable positive and negative examples to verify selected rules |
| Application using standalone source / 用独立源码构造应用 | Next, after library integration / 组合库贯通后 | Move a bounded discovery task onto `.jpp`, show inputs, intermediate results and output, and compare against the existing application |
| Broader composition / 更多独立方法 | Open / 待验证 | Contributors build three substantially different methods without modifying the core |
| Live judgment and portability / 真实判断与后端替换 | Partial / 部分已有 | [Published JEV discovery recordings](docs/towow-demo.zh-CN.md) exist on the retained Python path; a Rust live quickstart and a measured second backend remain open |

Current development starts with reusable semantics and executable programs. New primitives should replace demonstrated duplication or enable a concrete method, and should be compared with an implementation that omits them.

接下来先让已经运行的源码方法成为真正可复用的标准库，再统一影响组合的规则，最后让现有发现应用实际使用这套语言。下一步以可运行结果验收，不以新增语法或文件数量验收。
