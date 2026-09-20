# J++ design / 设计方向

The unit of reuse is a computation with an input, an output, and declared effects. Combining computations produces another computation with the same external interface. Questions and methods can both be passed as values.

复用的基本单元是一段有输入、输出和效应声明的计算。组合的结果仍然是一段同样接口的计算。问题和方法都能作为值传递。

```mermaid
flowchart LR
  A[Material + question] --> B[Judgment observation]
  B --> C[Programmable strategy]
  C --> D[Exact computation or action]
  D --> E[New state and evidence]
  E --> A
```

## Three cooperating layers

1. **Host computation / 宿主计算**: Python performs exact arithmetic, data manipulation and algorithmic checks.
2. **Judgment runtime / 判断运行内核**: `foundation.jv` represents materials, questions, judgments, exits, generation and actions. It tracks execution and budgets.
3. **Composition / 组合层**: `jev_compose` provides a uniform Component interface and reusable algorithm constructors.

An unresolved observation is represented explicitly. A caller can seek new evidence, retain several candidates, or continue exact calculations that do not depend on that observation. Accepted model judgments remain assumptions about the world; composing them does not turn them into mathematical truth.

## Why these demonstrations?

Adaptive inquiry shows that the next question is programmable. Counterexample feedback shows that a judgment can prioritize a candidate while an exact checker determines whether it meets a declared finite specification. Both return ordinary components that can be reused by other methods.

自适应提问体现“下一道问题也可以编程”；反例反馈体现“判断负责安排尝试，精确检查负责验证指定条件”。它们共用组件与迭代机制，没有单独发明两套执行器。

Binary search and counterexample-guided search are established algorithms. J++ explores a reusable language interface for combining such algorithms with semantic operations; it does not claim to invent those algorithms.

## Language versus implementation

The language goal is broader than the current Python implementation. We will derive syntax from working programs and composition needs. Rust or OCaml may be evaluated later against concrete runtime requirements; no independent compiler has been delivered in this release.

See [composition API in Chinese](composition.zh-CN.md) for exact current operations.
