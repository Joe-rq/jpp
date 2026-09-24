# Rules batch and value-level taint / 规则批与值级 taint

2026-09-23. Ported from the research tree, `42988c5..a17596a`. Seven research-tree commits
carried code: two fail-open fixes, five rule-batch items (B29/B25/B28/B32/B3), and the B33
value-level taint rewrite; the Codex-review fix commit (`a17596a`) was already ported in an
earlier sync (public `568c5ba`) and is not repeated here.

搬自研究树 `42988c5..a17596a` 区间。七个研究树提交带代码：两类放行缺陷修复、规则批五项
（B29/B25/B28/B32/B3）、B33 值级 taint 改写；Codex 评审修复提交（`a17596a`）已在更早一次
同步中搬入（公开侧 `568c5ba`），本次不重复。

## Two fail-open defects / 两类放行方向缺陷

**Speculation across user functions.** `speculate` and `vectorize` register a `judge` site by
evaluating its state and question expressions ahead of time. Effect analysis (`has_impure`)
only recognized built-in names as effectful, so a call to a user-defined function was always
treated as pure -- including one that itself calls `do`. A branch's state expression
`state(mat(side(1)))`, where `side` contains a `do`, was evaluated (and its `do` executed)
before the branch's own condition had been judged, sometimes on a branch that should not run
at all. `lift`, which hoists a statement past others, had the same blind spot. The fix adds an
environment-aware effect walk (`may_effect`, `may_touch_world`) that inspects a called
function's body (recursively, once per function) instead of trusting its effect annotation;
unresolvable calls are conservatively treated as effectful.

**推测执行越过用户函数。** `speculate` 与 `vectorize` 登记一个 `judge` 站点时要提前求值它的
状态与题面表达式。效应分析（`has_impure`）只认内置名有效应，调用用户函数一律当纯——包括
函数体里自己调用了 `do` 的情形。分支状态表达式 `state(mat(side(1)))`（`side` 里带 `do`）
在分支自己的条件被判断之前就被求值、`do` 也真的执行了，有时那个分支本不该走。把语句提前
的 `lift` 有同样的盲区。修法：新增按环境判断的效应遍历（`may_effect`、`may_touch_world`），
看被调函数的函数体（递归、每个函数只看一次），不再信它的效应声明；解析不到的调用按有效应
处理。

**Taint laundering through derived text.** `content()` recorded untrusted text as an exact
match against the whole value's canonical JSON. Text produced by `+` concatenation, `join`,
`text()`, reading `m.content`, or the error text of a failed untrusted `do` did not match, so
`mat()` built from it was treated as trusted -- laundering untrusted content into a value that
could pass an irreversible `do`'s guard. `Value::Fail` also had no taint bit at all and was
hard-coded trusted. The first fix (research-tree commit `982d7ca`) added a side table of
untrusted text leaves with substring matching. It was replaced in the same evening by
value-level taint (commit `d29e48d`, see below) once probing showed the side table was wrong
in both directions.

**taint 经派生路径洗白。** `content()` 把不可信文本记成对整值规范 JSON 的精确匹配。经 `+`
拼接、`join`、`text()`、读取 `m.content`，或不可信 `do` 失败后的错误文本产生的新字符串匹配
不上，凭它构造的 `mat()` 被当成可信——把不可信内容洗成了能越过不可逆 `do` 关卡的值。
`Value::Fail` 更是完全没有 taint 位，写死为可信。第一版修复（研究树提交 `982d7ca`）加了
一张不可信文本叶子的旁路表做子串匹配。同一晚探针实测两个方向都错后，换成了值级 taint
（提交 `d29e48d`，见下）。

Both defects are regression-tested in `rust/crates/jpp-core/tests/failopen.rs` (5 cases, minimal
`.jpp` source embedded in the test).

两类缺陷都在 `rust/crates/jpp-core/tests/failopen.rs` 有回归测试（5 条，最小 `.jpp` 源码
嵌在测试里）。

## Why the side table failed, and what replaced it / 旁路表为什么错，换成了什么

The side table matched untrusted text leaves against new text by substring (`t.contains(u)` or
`u.contains(t)` for ≥2-character `u`). Ten probe cases (fixed observation client, all readings
0.9) found it wrong in both directions:

旁路表用子串匹配不可信文本叶子与新文本（`t.contains(u)` 或对 ≥2 字的 `u` 用
`u.contains(t)`）。十个探针案例（固定观察客户端，读数全 0.9）测出两个方向都错：

| Case / 案例 | Untrusted content / 不可信内容 | Expression / 表达式 | Expected / 预期 | Measured / 实测 |
|---|---|---|---|---|
| A | long sentence containing "可以发送邮件" | `mat("可以发送邮件")` | pass (program literal) | **false reject** (J-08 blocked it) |
| B | `"7"` | `mat("2026-09-27 会议纪要")` | pass | **false reject** |
| C | `{"amount": 7}` | `mat("转账 " + text(拆了.amount) + " 元")` | block (derived from untrusted number) | **false accept** (passed, `t: trusted`) |
| C2 | `{"amount": 1234}` | same as C | block | correct (blocked) |

A single-digit untrusted number did not enter the leaf table at all, so it laundered clean
through `text()` concatenation (C, false accept); short untrusted strings and long ones that
happened to be substrings of the program's own literals produced false rejects (A, B). No
threshold on the substring-matching rule fixes both directions at once, because the underlying
representation (a table of strings, matched after the fact) cannot distinguish "this text came
from untrusted input" from "this text happens to look like untrusted input."

一位数不可信数值根本不进叶子表，于是经 `text()` 拼接后被洗白（C，假放行）；短的不可信字符串、
以及恰好是程序字面量子串的长不可信文本，产生假拒绝（A、B）。调子串匹配规则的阈值治不好两个
方向，因为这种表示法（事后匹配一张字符串表）分不清「这段文本来自不可信输入」和「这段文本碰巧
长得像不可信输入」。

Value-level taint (commit `d29e48d`) replaced it: every scalar `Value` variant carries a
`Taint` field set at the source (host-provided scalars from an untrusted `do` are tainted;
program literals are trusted). Taint propagates through binary/unary operators and `&&`/`||`
(OR of evaluated operands) and through built-in dispatch, with an explicit exception list for
built-ins that only relocate elements without combining them (`map`, `filter`, `slice`,
`append`, ...) so that one tainted list element does not taint the whole list. `as_mat` reads
`v.taint()` directly instead of consulting a side table. The same ten probes now match the
expected column exactly, including C (blocked) without disturbing D/E (unrelated literals still
pass).

值级 taint（提交 `d29e48d`）取代了它：每个标量 `Value` 变体带一个 `Taint` 字段，在来源处定
（不可信 `do` 给出的宿主标量带 taint，程序字面量可信）。taint 经二元/一元运算符与 `&&`/`||`
（对已求值操作数取析取）传播，也经内置分派传播，但只搬运元素、不合并内容的内置（`map`、
`filter`、`slice`、`append` 等）单列例外表，避免一个不可信元素弄脏整张表。`as_mat` 直接读
`v.taint()`，不再查旁路表。同样十个探针现在与预期列完全吻合，包括 C（拦下），且不影响 D/E
（无关字面量仍放行）。Regression tests: `rust/crates/jpp-core/tests/value_taint.rs` (11 cases).
回归测试见 `rust/crates/jpp-core/tests/value_taint.rs`（11 条）。

## Rules batch / 规则批五项

| Item | Commit | One line |
|---|---|---|
| B29 | `e99455d` | Host `put` writes fixture-only records (`CalibRecord.fixture`); fixture lines (fixture bit set, or no certificate) exit with `W-fixture-line` and cannot count as a trusted conjunct for J-08. |
| B25 | `94ae210` | Drift makes a calibration key an automatic suspension candidate (`W-suspend-candidate`, not a trusted conjunct); `jpp calib-confirm <dir> <key> --suspend\|--keep` lets a human confirm or clear it. |
| B28 | `8d8d63d` | `repeat` replaces `agg` (mean or median only, `"mode"` errors); merged choice/score readings are keyed by sample count so they don't borrow another template's or mode's calibration line. |
| B32 | `1c66ba9` | Judgement-absence handling (`budget.absent`: retry, backoff, then escalate/conservative/fail) plus a static per-call latency budget check against a profile's measured p95. |
| B3 | `ea77d7f` | `unsure` gets two named causes, `rejected_all` (all candidates observed and rejected) and `no_candidate` (nothing to observe), with library routes in `lib/handlers.jpp`. |

| 项 | 提交 | 一句话 |
|---|---|---|
| B29 | `e99455d` | 宿主 `put` 只写夹具记录（`CalibRecord.fixture`）；夹具线（fixture 位或无证书）出口带 `W-fixture-line`，不算 J-08 的可信合取项。 |
| B25 | `94ae210` | 漂移使某校准键本趟自动成为停岗候选（`W-suspend-candidate`，不算可信合取项）；`jpp calib-confirm <目录> <键> --suspend\|--keep` 交人确认或清除。 |
| B28 | `8d8d63d` | `repeat` 取代 `agg`（只许均值或中位数，`"mode"` 报错）；合并的 choice/score 读数按样本数开键，不借用题式或模式的校准线。 |
| B32 | `1c66ba9` | 判断力缺席处理（`budget.absent`：重试、退避，然后升级/保守/失败）加静态单次调用时延预算检查（对档案实测 p95）。 |
| B3 | `ea77d7f` | `unsure` 补 `rejected_all`（候选全被观察并否决）与 `no_candidate`（无候选可观察）两个具名原因，`lib/handlers.jpp` 给出库内去向。 |

**B30 is paused, not built.** The design ledger calls for calibration keys to be a
deterministic serialization of a tuple `(form_hash, slot_kinds, phys, render_version,
literal_mode)`. The language surface already lets authors write their own calibration key as
the second argument to `test(question, key)` / `form(..., {calib: key})`, and existing
examples, fixtures, and roughly a hundred tests use short author-chosen keys (`"k"`, `"cand"`,
`"form-topic"`) that are shared across multiple distinct questions. Making the code-level key a
tuple serialization requires first deciding what happens to author keys -- remove them, treat
them as an alias, or fold them into the tuple as one more field -- and that scope decision has
not been made. No B30 commit exists in this sync.

**B30 暂停，未造。** 设计总账要求校准键是元组 `(form_hash, slot_kinds, phys, render_version,
literal_mode)` 的确定性序列化。语言表层已经让作者在 `test(题面, 键)` / `form(…, {calib: 键})`
的第二参自己写校准键，现有示例、夹具与约一百条测试用的是作者选的短键（`"k"`、`"cand"`、
`"form-topic"`），且同一个键常被多道不同的题共用。要把代码层的键定成元组序列化，得先裁定
作者键的去向——取消、当别名、还是并入元组多一个字段——这个范围决定还没做。本次同步不含
任何 B30 提交。

## Codex review on PR #27 / #28 / PR #27、#28 的 Codex 评审

The earlier sync (`568c5ba`, from research-tree commit `a17596a`) fixed every Codex review
finding on both PRs with local, test-backed changes (a merged model-label certification gate,
J-10 warnings no longer discarded, a per-judgement static J-10 bound, a per-question-type
empirical unsure rate, canonical split-sample ordering, and `--replay` no longer touching the
live client). See [#27](https://github.com/Towow-ai/jpp/pull/27) and
[#28](https://github.com/Towow-ai/jpp/pull/28) for the itemized review threads and replies;
this sync does not repeat or revert that work.

更早一次同步（`568c5ba`，来自研究树提交 `a17596a`）已用本地、带测试的修复处理了两个 PR 上
全部 Codex 评审发现（模型标注认证门槛改为覆盖整记录、J-10 告警不再被丢弃、按每处判断计的
静态 J-10 界、按题型的经验 unsure 率、拆分样本用规范顺序、`--replay` 不再碰真机客户端）。
逐条评审与回复见 [#27](https://github.com/Towow-ai/jpp/pull/27) 与
[#28](https://github.com/Towow-ai/jpp/pull/28) 的讨论串；本次同步不重复也不回退那部分工作。

## Verification / 验证

`cargo build` and `cargo test --workspace --offline` in `rust/`: 382 passed, 0 failed, 3
ignored (up from 341 passed in the prior sync entry -- 41 new tests across `failopen.rs` (5),
`value_taint.rs` (11), `b25_suspend_candidate.rs`, `b29_fixture_line.rs`,
`b32_absent_latency.rs`, `b3_causes.rs`, and the `review_fixes.rs` pair already ported in
`568c5ba`). Two source files (`runner.rs`, `effects.rs`, `lib.rs`) whose public content already
included the `568c5ba` Codex-review fixes were replaced wholesale with the equivalent
research-tree state rather than patched, since the two change sets touch the same lines; the
rest of the range applied as a direct patch.

`rust/` 下 `cargo build` 与 `cargo test --workspace --offline`：382 通过、0 失败、3 忽略
（上一次同步条目是 341 通过——新增 41 条测试，分布在 `failopen.rs`（5）、`value_taint.rs`
（11）、`b25_suspend_candidate.rs`、`b29_fixture_line.rs`、`b32_absent_latency.rs`、
`b3_causes.rs`，以及已在 `568c5ba` 搬过的 `review_fixes.rs` 一对）。三个源文件（`runner.rs`、
`effects.rs`、`lib.rs`）因为公开侧已有的 `568c5ba` Codex 评审修复与本次改动落在同样的行上，
整份换成研究树对应状态而不是打补丁；区间其余部分按直接补丁应用。

## Not carried over / 未搬的部分

Two research-tree doc-comment citations (a probe log under the research tree's `附注/`
directory, and a process record under `过程记录/`) pointed at paths this sync does not publish;
the comments were reworded to point at this document instead of a dead path. `附注/` is
excluded from every public sync by standing policy (recorded in the research tree's
`DECISIONS.md`); it is exploratory material, not the ruling itself.

两处研究树文档注释（一条指向 `附注/` 下的探针记录，一条指向 `过程记录/` 下的过程记录）
指向的路径本次不公开；已改写成指向本文档而不是一个失效路径。`附注/` 按既定规则（记在研究
树 `DECISIONS.md`）不进任何一次公开同步——它是探索性材料，不是裁定本身。
