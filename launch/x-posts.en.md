# J++ X posts — en

Drafts / 待发布。Copy only the text inside each block / 只复制代码块中的正文。

## en-T01 · Launch thread

Weighted upper bound / 加权长度上界：209/280

```text
1/10 We are building J++, an experimental programming language.

The starting intuition: what could people build if questions and solving methods were values they could pass around, compose, and compose again?
```

## en-T02 · Launch thread

Weighted upper bound / 加权长度上界：215/280

```text
2/10 Algorithms show how much organization matters: ten reliable binary answers can distinguish 1,000 candidates.

We want to explore what happens when semantic judgment works alongside these algorithmic structures.
```

## en-T03 · Launch thread

Weighted upper bound / 加权长度上界：210/280

```text
3/10 JEV prompted us to treat judgment as a callable operation. Around it, a program can select material, construct the next question, and run exact checks.

Those organizational choices should be reusable too.
```

## en-T04 · Launch thread

Weighted upper bound / 加权长度上界：220/280

```text
4/10 One design requirement matters especially: a composition should still be a component.

A method can accept another method or return a new one. Its internals can grow while the calling interface stays understandable.
```

## en-T05 · Launch thread

Weighted upper bound / 加权长度上界：215/280

```text
5/10 Uncertainty needs a place in the program: which part is unresolved, and what can already be computed?

We want exact computation, search and new evidence to keep working around explicit unresolved observations.
```

## en-T06 · Launch thread

Weighted upper bound / 加权长度上界：224/280

```text
6/10 Today J++ is a Python embedded language, a runtime and a composition library.

We are using executable programs to discover the rules we need. Independent syntax can follow. We do not yet know the language's final form.
```

## en-T07 · Launch thread

Weighted upper bound / 加权长度上界：223/280

```text
7/10 The first offline demo finds a target among 1,000 candidates in ten questions.

Answers are synthetic; no API key is needed. It makes the inquiry mechanism runnable without treating the demo as a model-accuracy result.
```

## en-T08 · Launch thread

Weighted upper bound / 加权长度上界：255/280

```text
8/10 Another demo generates expressions, executes checks and feeds counterexamples back. After three trials it finds an absolute-value expression and checks all nine declared inputs.

Generation is finite enumeration; the checks and feedback actually run.
```

## en-T09 · Launch thread

Weighted upper bound / 加权长度上界：221/280

```text
9/10 We hope to explore code construction, investigation and constrained planning. We also hope other people find uses we did not anticipate.

What we most want to see now: a new method built from the existing primitives.
```

## en-T10 · Launch thread

Weighted upper bound / 加权长度上界：216/280

```text
10/10 J++ is open source under MIT. The repository includes setup instructions, offline demos, design motivation and a roadmap.

Try it. Compose a method. Show us what is difficult to express.
https://github.com/Towow-ai/jpp
```

## en-D01 · 十次提问

Weighted upper bound / 加权长度上界：230/280

```text
J++'s offline example narrows 1,000 candidates to one using ten synthetic binary answers.

Binary search is familiar. Try changing the component that chooses the next question: how much of the surrounding method can stay the same?
```

## en-D02 · 反例反馈

Weighted upper bound / 加权长度上界：228/280

```text
A small loop: generate candidates, execute checks, return a failing input to the generator.

J++'s expression example finds its result in three trials. We want the whole loop to be a method another program can receive and reuse.
```

## en-D03 · 从 Python 开始

Weighted upper bound / 加权长度上界：197/280

```text
J++ currently uses Python. That lets us run compositions and see where the interfaces become awkward.

We want concrete programs to tell us what an independent syntax should make easier to express.
```

## en-D04 · 问题作为值

Weighted upper bound / 加权长度上界：209/280

```text
We want questions to be values that can be saved, passed and constructed.

A program can decide what to ask next. J++ already includes question serialization and restoration as a small step toward that design.
```

## en-D05 · 局部不确定

Weighted upper bound / 加权长度上界：210/280

```text
Three observations: true, true, unresolved. "At least two?" is decided; "at least three?" remains open.

J++'s count bounds preserve this distinction. The conclusion is conditional on the accepted observations.
```

## en-D06 · 当前任务

Weighted upper bound / 加权长度上界：178/280

```text
Next for J++: clearer question, material and result interfaces, then more algorithms built from the same components.

We want concrete use to reveal what the language is missing.
```

## en-D07 · 组合的组合

Weighted upper bound / 加权长度上界：230/280

```text
The question I keep returning to: after we build a composition, can we compose it again?

J++ uses one component interface for small operations and compound methods. Reuse should remain understandable as internal complexity grows.
```

## en-D08 · 第三种方法

Weighted upper bound / 加权长度上界：236/280

```text
J++ includes a separately authored allocation method. It connects review outcomes to exact allocation, then packages the result inside a larger component.

We are looking for more examples that test how far the interfaces can be reused.
```

## en-D09 · 怎样判断价值

Weighted upper bound / 加权长度上界：209/280

```text
How should we judge J++?

Can someone reuse a method they did not write? How much code changes when a strategy changes? Does a new algorithm require kernel changes?

Concrete programs are the evidence we want.
```

## en-D10 · 首版验证

Weighted upper bound / 加权长度上界：201/280

```text
The first J++ alpha passed 25 mechanism tests, with Linux CI passing on Python 3.12 and 3.13.

You can install it and run offline. Real-model quality needs separate experiments.
https://github.com/Towow-ai/jpp
```

## en-D11 · 后端演进

Weighted upper bound / 加权长度上界：229/280

```text
JEV is the starting point for J++. We want method composition to evolve with judgment backends.

Which rules survive a model change, and which need new measurements? Backend portability is a design question we still need to test.
```

## en-D12 · 贡献邀请

Weighted upper bound / 加权长度上界：239/280

```text
Want to help shape J++? Build a method the examples do not contain, then use it inside another method.

Bring the runnable example, awkward interfaces and repeated glue code. That is useful language-design feedback.
https://github.com/Towow-ai/jpp
```
