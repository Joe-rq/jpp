# Contributing / 参与贡献

J++ is early-stage. A small executable method teaches us more than a large speculative feature list.

## Start locally

For language work, follow the [native source guide](rust/README.md): run `cargo test --locked --workspace` from `rust/`, then run the `.jpp` examples. The parser, shared checker/runtime and CLI live in separate crates within one implementation.

语言开发从 `rust/` 开始。优先提交一个能运行的 `.jpp` 方法，说明它怎样继续参与组合。Python 参考实现的维护与实验仍按 README 中的 Python 安装步骤运行。

Useful first contributions:

- Build a new `.jpp` solver using method values, questions, exact computation and bounded iteration.
- Extract a method reused by two source programs, showing what duplicated code it removes.
- Turn an awkward composition into a minimal reproducible example.
- Improve an installation step or translate an API explanation.
- Compare an algorithm using the library with the same algorithm without it.

Please include the problem, example command, expected behavior and actual result. For model experiments, distinguish synthetic observations from real calls and state the backend/version, evaluation data and cost.

## Pull requests

Keep a PR focused on one outcome. Explain the user-visible change and how you checked it. Include tests when behavior changes. Do not include credentials or private source material. Contributions are provided under this repository's MIT license.

## Design discussions

用中文或英文都可以。先给出你想构造的方法，再展示现有接口哪里难以表达。新增基本单元时，最好同时给出不用它的版本，让大家能看出它消除了什么重复工作。

Nature maintains the project and makes release decisions. Opening an issue is enough to start; no invitation is required.

## Publishing progress / 发布进展

Publish each completed, verified advance with a clear Chinese and English commit message and a dated progress note. Explain what changed, what it enables and how it was checked. Follow the [maintenance practice](docs/maintaining.md).

每完成一项经过验证的实际进展，就同步 GitHub，用中英双语提交说明和带日期的进度记录，讲清改了什么、带来什么效果、怎样验证。具体流程见[维护约定](docs/maintaining.md)。
