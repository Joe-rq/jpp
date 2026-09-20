# Distribution verification

## Native source package / 原生源码整包 · 2026-09-21

[PR #12](https://github.com/Towow-ai/jpp/pull/12) delivered the source-to-execution package. In an independent publication checkout, 41 Rust tests passed; one illustrative documentation snippet was explicitly ignored. Tests cover source/core equivalence, complete adaptive and partial-result algorithms, source diagnostics, known literal argument type errors, budget stopping and replay. Native installation ran outside the checkout with PATH empty, without Python or Cargo. GitHub Rust and Python 3.12/3.13 checks passed for the final PR revision.

实际结果：方法组合得到 43；十次固定判断定位 731；部分方案由成本 9 改进到 2，共六次观察、三次本地检查；重放返回相同值，零新增判断与重复检查。固定观察验证执行机制，不是模型质量测量。源码、命令和范围见 [Rust 包](../rust/README.md)。

## Original Python snapshot / 最初 Python 快照（历史记录）

Verified locally on 2026-09-20 using Python 3.12 on macOS ARM64.

| Check | Result |
|---|---|
| Fresh virtual environment, `pip install -e '.[dev]'` | Passed |
| Installed `jpp demo` | Passed; target 731 in 10 questions; expression in 3 trials, 9 inputs checked |
| `python -m pytest -q` | 25 passed; two expected budget-exhaustion warnings |
| Wheel build | Passed |
| Publication file scan | No credential-shaped strings or private absolute paths detected |

The credential scan is a scoped check, not a general security audit. Test observations and calibration entries are synthetic. Live model accuracy and service compatibility were not measured in this release preparation.

CI runs the offline example and tests on Linux with Python 3.12 and 3.13. The Actions page is the source of truth for remote run status.
