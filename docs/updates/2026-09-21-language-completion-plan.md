# 2026-09-21 update: a second authority text, lazy execution on the native kernel, and 22 pieces of taste dressed as mechanism / 第二份依据、原生内核上的惰性执行，以及 22 处伪装成机制的 taste

Dated update following the practice in `progress.md`. This round synchronises research documents and the model profile archive; no Python or Rust source in this repository changed. Numbers in "Verification in this repository" were re-run here on the date above; everything measured in the research workspace sits in its own block and is labelled as such.

**Read this first, because the rest of the update will mislead you without it.** The kernel work described below happened in the research workspace's Rust core. It is **not** in this repository's `rust/` yet, and this round deliberately does not sync `rust/`: the public `rust/` tree is advanced by the front-end line, the workspace core is several packages ahead, and merging the two lines is a separate item in the private coordination page rather than something to fold into a document sync. You can check the lag here without trusting this sentence: `rust/crates/jpp-core/src/interp.rs:1` still reads「效应即时发出（惰性融合是后续优化，未迁移）」, and `checked_add` does not occur anywhere under `rust/crates/jpp-core/src/`.

## A second authority text: `13`, written from what building in Rust taught us / 第二份依据

`research/地基/13-Rust实践反馈设计修订-v0.2.md` is published here for the first time. It is not a new design. It takes six places where a day of building in Rust contradicted `12-IR与类契约-v0.1.md`, and replaces those places only; everything else still comes from `12`, with `13` winning where they conflict.

Three of the six are correctness defects, not preferences:

- **Method identity did not include captured state.** A closure was hashed by its body, and `transform` used that hash as its ledger key. Two methods built by the same factory with different captured values therefore collided, and the second call reused the first one's result. That is a wrong answer, not a slow one.
- **Going over budget threw away work that had already been paid for.** The old order compared the call's actual cost against the remaining budget after the backend returned and exited before recording anything, so the call had happened, the money was spent, and the result was discarded. The rule now separates the two: check the budget before calling, record the completed request, its result and its actual cost after it returns, and only then decide whether to continue.
- **Integer overflow behaved differently in debug and release builds.** Rust panics in debug and wraps in release; neither is a language behaviour. Overflow, division by zero and the related boundaries are now runtime errors that point at the `.jpp` source.

Each of the six rules has at least one acceptance test in the workspace core named after its clause (`第一条` … `第六条`, eight tests in total), and each test's doc comment quotes the clause it is checking. The other three rules — constructing a method does not execute its body, effects are instantiated at the call site, and an unresolved result is tracked by where it actually goes — were already true in the core, and for those three `13` writes down what was already being done correctly. The other three were wrong, and `13` is what corrects them.

## Lazy execution, refresh points and layering / 惰性执行、刷新点与分层

The specification has always asked for lazy registration: a `judge` is registered rather than sent, and at a refresh point every registered question whose inputs are ready is grouped by state, layered by dependency, and sent one layer at a time. The native kernel executed statement by statement instead. That gap was not on any open list — it lived in a single module comment.

It is now implemented, and the saving is the point of the design rather than a side effect: **three states with two questions each cost 6 calls when every effect is emitted immediately, and 3 calls under lazy registration with same-state fusion.** This is the first time P5 — you pay for the state, the questions on it are free — is **demonstrated** on the native kernel rather than only in the Python reference. Demonstrated, not established: it is shown for questions registered on the same state within one layer, which is the case this program exercises. It is not a claim that every program's questions are now free.

Two things about that number are worth stating plainly. It comes from one shaped program (3 states × 2 questions), so it is not a general "half price" claim. And it is now produced by an ablation switch rather than by comparing two versions of the code: with the `fuse` pass off the same program costs 6 calls, with it on, 3. The implementer's own note on the ablation is the part worth copying — the off arm has to be turned off *completely*, down to sending each question of a single `judge` registration separately, or the "one state, many questions" layer is still fusing and the measured saving comes out too small. **An ablation arm that is not fully off produces a difference that is wrong in a way nobody can see.**

Layering fell out of the semantics rather than needing a scheduler: a judgment that depends on a previous exit can only be registered after the previous flush, because getting that exit requires a `cut`, and `cut` is itself a refresh point. So one flush is one layer.

## Two rounds of inventory, and what was wrong with them / 两轮盘点，以及它们自己的问题

The plan published here as `14-实施计划-把语言做完整-v1.md` rests on two agent rounds: a nine-dimension inventory of the specification against the kernel (18 agents, audit plus adversarial review) and a close reading of the front-end design documents hunting for taste (16 agents). Both cost nothing to run.

Neither round should be read as authoritative, and the plan says so in its own first section:

- **`13` was absent from the first round's material list.** The audits went out at 01:10 and `13` was written at 01:14. The 21 "conflicts with the new design" the audits reported are therefore untrustworthy — most of them hold `12` against the Rust code on points `13` had already settled. The plan does not use that category at all; every conflict was re-judged against the text of `12` plus `13`.
- **Adversarial review failed all nine audits.** It overturned 30 classifications and supplied 27 gaps the audits had missed. The plan only uses items the review confirmed or supplied itself. The review is not a final court either: several of the 30 it overturned were "wrong category, right observation", so every item in the plan carries its evidence location and the instruction that the implementer must check the code before acting on it.

## The most valuable output: 22 pieces of taste dressed as mechanism / 22 处伪装成机制的 taste

The reviewers' first task was to find entries that read like rules but cannot be written into a checker and cannot be pinned by a test. They found 22. Several are ours.

**The rule that came out of it: if a rule cannot say what would turn what red, it is taste, however much it reads like a rule.** Writing it in a document is fine. Recording it as a delivered mechanism is not.

Two of the 22, to make the shape concrete:

- **"Reserve a field in the result protocol."** A `goal` field on `Outcome` with no producer, no consumer, and no test that could ever fail because of it. The reviewer's verdict was that this is exactly the failure the task named: a shell that sounds like a mechanism, built to hold taste.
- **"Every new builtin must state which kind of language-level operation it is," enforced by a test that walks the builtin table against a whitelist.** The test can verify that somebody attached a category label to a name. It cannot verify that the label is honest. A domain-specific `search_bounded` labelled "general container operation" passes.

The rule has since been used twice against its authors. The discipline that `unsure_bound`'s independent estimate is a reference value and never a criterion used to live only in a comment, which stops nobody; the independent estimate now has a type that does not implement ordering, so using it as a criterion fails to compile. And a proposal to build the "unknown must not be reported as zero" symbolic estimate ahead of the lazy layer was rejected by the same rule — with no pass producing estimates yet, that type would be a shell with no producer and no consumer.

## A negative premise worth publishing: when `allocate` does not measure what it claims / `allocate` 的负面前提

`allocate` picks the k readings whose uncertainty is highest. On a **cold key** — one with no calibration record — the line falls back to the archive's conservative default, the band is wide, and 85–89% of readings land inside it. Uncertainty inside the band is zero by definition and ties break by ascending index, so `allocate` degenerates into **taking the first k readings in file order. It is not measuring uncertainty at all.** New keys are cold by default, which makes this the easy case to hit in a real program.

The implementer recorded this as a result rather than rerunning it away, and that is where the usable part came from. With a warm key the ordering does work: the tie rate on `noul` drops from 89% to 51%, and at k=30 the error gap is 0.068 (0.110 against 0.178). But the applicability criterion is **not** "is the key warm". It is **how many readings on this key fall inside the band** — because the band width is set by the archive's δ and the calibration record's hi/lo, not by whether the key is warm. That is the criterion more programs will actually trip over.

A correction to our own reasoning belongs with it. The bet was that `noul` would show the clearest gain and `score` the weakest, attributed to calibration quality. The direction happened to be right and **the reason was wrong**: what drives it is band width and tie rate. The corrected attribution is worth more than the bet it corrects.

## A real bug the tests caught / 一个被测试抓住的真 bug

`allocate` reads how far a reading sits from the decision band. That is a quantity **on the answer**, and `allocate` was not in `12` §2.2's list of refresh points — so it ran against unanswered readings and selected `[0,1,2,3]` where the correct answer was `[2,4,6,8]`.

The fix is a one-line classification. The lesson is about the shape of the specification: **a list of refresh points fails silently.** Every new operation that reads an answer has to be remembered and added, and forgetting produces a wrong result rather than an error. The proposal now standing against `12` is to make the criterion primary — *any operation whose result depends on an answer is a refresh point* — with the list demoted to examples, so a new answer-reading operation is a refresh point by default and an exception is what needs an argument.

The implementation went one step further than the proposal, and that step is the transferable part. The single entry point for reading an answer was renamed to say so (`Reading::answer_after_flush()`), with the criterion in its doc comment. **A criterion written in the authority text still has to be looked up; written on the only entry point, everyone who reads an answer walks into it.** (`Reading::answer_after_flush()` is at `crates/jpp-core/src/value.rs:269` in the workspace core — again, not in this repository.)

## Known limits and what this round does not do / 已知限制与未做项

From `14` §8, each with the authority text that excludes it. These are excluded deliberately, not missed, and none of them is excluded for being hard:

- **Explicit syntax for effect variables (`!{ε}`).** `13` §2: "留待实际需求，不作为本版前置". A related defect was fixed along the way — the effect-name table was defined and never referenced, so `!{ε}` parsed as a concrete effect named ε and produced an error pointing in the wrong direction, which is worse than no annotation at all. Unknown effect names now raise a dedicated diagnostic.
- **A full linear type system.** `13` §3: "不要求所有值都使用线性类型". The obligation being tracked is the unresolved result, not every value; runtime accounting plus a small amount of static checking is the accepted v1.
- **A conformal abstention region.** Neither implementation has one; `12` §G4 lists it as a gap and this version does not close it.
- **Runtime continuations as the basis for resume.** Host `async`, effect handlers and coroutines are all one-shot and none of them serialises, so none can carry cross-process recovery. The standing proposal is that suspension and resumption are decided by the ledger alone, and a runtime continuation may only ever be an in-process implementation detail that does not let recovery bypass ledger accounting. Writing that down now is cheaper than reclaiming it later, because the failure mode is that J-18 stops holding while nobody is watching and the ledger quietly degrades from an audit record into a cache.

Two further scoping notes. The raw agent results behind `14` (`foundation/experiments/审计/*.json`) stay in the research workspace and are not published, so the paths `14` cites for them do not resolve in this repository. And `14` §9 lists five questions that are the project owner's to decide; one of them concerns third-party data and its details are deliberately not expanded in the public copy, through the same redaction mechanism `tools/sync-from-workspace.sh` applies to every synced document.

## 中文

**先说最要紧的一条，否则下面全会被读错。** 本次同步的是研究文档与模型档案，不是内核。下面讲的内核改动发生在研究工作区的 Rust core，**不在本仓库的 `rust/` 里**；本轮有意不同步 `rust/`——公开 `rust/` 由前端那条线在推，工作区 core 已领先数包，两条线的合并是内部协作页上的单独一项，不塞进文档同步。这句话你不必信，可以自己查：`rust/crates/jpp-core/src/interp.rs:1` 仍写着「效应即时发出（惰性融合是后续优化，未迁移）」，`checked_add` 在 `rust/crates/jpp-core/src/` 下全无命中。

**第二份依据 `13` 首次公开。** 它不是新设计，只替换掉一天 Rust 施工中与 `12` 冲突的六处，其余沿用 `12`，冲突处 `13` 优先。六条里三条是正确性缺陷：方法身份只按正文哈希，工厂造出的同正文不同捕获的两个方法撞键，第二次调用复用了第一次的结果——这是算错不是变慢；超预算的旧顺序在后端返回后先核预算再记账，于是调用已发生、钱已花、结果被丢掉，现在改成调用前核预算、返回后先记录请求身份/结果/实际费用、再决定是否继续；整数溢出在 debug 崩溃、release 回绕，两者都不是语言行为，现在溢出、除零及相关边界都是指向 `.jpp` 源码的运行错误。六条各有按条号命名的验收测试（`第一条`…`第六条`，共八条），测试的文档注释引的是该条原文。另外三条（造方法不执行方法体、效应在调用处实例化、未决按实际去向传递）在 core 上本来就成立——就这三条而言，`13` 是把我们做对的事写成规则；另外三条本来是错的，`13` 是来纠正它们的。

**惰性执行 + 刷新点 + 分层落地。** 规范一直要求登记后不发，到刷新点把已登记且输入就绪的题按状态分组、按依赖分层，一层一次发出；原生内核此前是逐语句即时执行，而这个缺口不在任何待定清单上，只活在一句模块注释里。现在实测：**三状态各两题，即时执行 6 次调用，惰性 + 同状态融合 3 次。** 这是 P5（状态收费、状态上的题免费）第一次在原生内核上**被演示出来**，而不只在 Python 参照实现里。是演示不是确立：兑现范围是「同一层内登记在同一状态上的题」，也正是这个程序所走的那一格；它不等于「从此所有程序的题都免费」。两点要说清楚：这个数来自一个特定形状的程序（3 状态 × 2 题），不是「普遍省一半」；它现在由消融开关量出——关掉 `fuse` 同一程序 6 次，开着 3 次。实现者关于消融的那条更值得抄：**消融臂必须关干净**，要关到「同一次 `judge` 登记的多道题也逐题发」，否则「一状态多题」那层仍在融合，量出来的省钱偏小——**一个没关干净的消融臂，差值是假的，而且假得看不出来。** 分层是从语义里掉出来的，不需要调度器：依赖前一条出口的判断只可能在前一次刷新之后才登记得上（要拿出口就得先 `cut`，而 `cut` 本身是刷新点），所以一次刷新就是一层。

**两轮盘点，以及它们自己的问题。** `14` 基于两轮：九维度规范盘点（18 代理，审计 + 对抗复核）与 Codex 设计文档的 taste 挖掘（16 代理），两轮都零成本。两轮都不该当终审，计划自己第一节就写明：其一，**`13` 整份缺席于第一轮的材料清单**——审计 01:10 派出、`13` 01:14 写成，所以审计报出的 21 条「与新设计冲突」不可信（多数是拿 `12` 去对 Rust，而 `13` 已经把其中几条定死），计划整类不采用，冲突一律回 `12`+`13` 原文重判；其二，**对抗复核把 9 份审计全部判为不通过**，推翻 30 条分类、补出 27 条漏掉的缺口，计划只采用经复核确认或复核补出的条目——但复核也不是终审，它推翻的 30 条里有若干是「分类错、现象对」，所以进计划的每一条都标了证据位置，实施者动手前必须自核代码。

**最有价值的产出是 22 处「被包装成机制的 taste」，其中几处是我们自己写的。** 由此立下的法则：**一条规则若说不出「什么情况下它会让什么东西变红」，它就是 taste，不管措辞多像规则**；写进文档可以，不许记成「已落实的机制」。两个具体例子：「在结果协议里预留字段」——`Outcome` 上加一个 `goal` 字段，没有生产者、没有消费者、没有任何测试能因为它红，复核员判它「正好是任务点名的那种失败：造一个听起来像机制的壳来装 taste」；「新增内置必须说明它是哪一种语言级操作」配一个遍历内置表对照白名单的测试——这个测试只能验证有人给这个名字贴了一个分类标签，**验证不了标签诚实**，一个领域专用的 `search_bounded` 贴上「通用容器操作」就能过。这条法则此后两次被用来约束它的作者：`unsure_bound` 的「独立估计只作参考不作判据」原先只写在注释里（注释拦不住任何人），现在那个独立估计的类型不实现序比较，拿它当判据**编译不过**；另有一条「资源估计里未知不许写成零」的排序提议被同一条法则否掉——Rust 侧还没有任何会产出估计的 pass，先建那个类型就是造一个没有生产者也没有消费者的壳。

**一条值得公开的负面前提：`allocate` 什么时候量的不是它声称的东西。** `allocate` 取不确定度最高的 k 条读数。在**冷键**（没有校准记录的键）上，线退回档案保守默认、带很宽，85–89% 的读数落在带内；带内不确定度按定义一律为 0、并列按下标升序，于是 `allocate` **退化成「按文件顺序取前 k 条」——它量的根本不是不确定度**。新键默认就是冷的，这是真实程序里最容易踩的一种。实现者把它当结果记下而不是当失败重跑，可用的部分正是从这里来的：换上岗线后排序确实起作用，`noul` 的带内并列从 89% 降到 51%，k=30 时误差差 0.068（0.110 对 0.178）。但适用判据**不是**「键上没上岗」，而是**这个键上有多少读数落在带内**——因为带宽由档案 δ 与校准记录的 hi/lo 决定，不由键冷不冷决定，而后者才是更常撞上的那一条。连带更正我们自己的一条归因：原赌「noul 最明显、score 最弱」并归因于校准好坏，方向碰巧对，**理由是错的**——真正的驱动是带宽与并列比例；这条更正比它更正的那条赌有用。

**一个被测试抓住的真 bug。** `allocate` 读的是「离决定带多远」，那是**答案上的量**，而它不在 `12` §2.2 的刷新点清单里，于是拿到的全是未答读数，选出 `[0,1,2,3]` 而正确答案是 `[2,4,6,8]`。修法是一行归类，教训在规范的形式上：**清单形式会静默失效**——每加一个读答案的操作都要记得补清单，漏补的失效方式是给出错误结果而不是报错。现在对 `12` 立着的提议是改成判据优先：*凡结果依赖于答案的操作皆是刷新点*，清单降为例子，这样新增的读答案操作默认即是刷新点，要例外才需论证。实现比提议多走了一步，而那一步才是可迁移的：读答案的唯一入口改名成 `Reading::answer_after_flush()`，判据写在它的文档注释里。**判据写在依据里仍要人去读，写在唯一入口上才是机制。**

**已知限制与本轮未做项**（照 `14` 第八节，每条都引依据原文；都是明确排除不是漏掉，也都不是因为难）：**效应变量 `!{ε}` 的显式语法**——`13` §2 原文「留待实际需求，不作为本版前置」；顺带修掉一处相关缺陷：效应名表定义后全树零引用，于是 `!{ε}` 被当成一个叫 ε 的具体效应解析成功，再报一条指向错误方向的错，比不写标注还糟，现在认不得的效应名报专门的诊断。**完整线性类型系统**——`13` §3 原文「不要求所有值都使用线性类型」；要追踪的义务是未决结果而不是所有值，运行期记账加少量静态检查是被接受的 v1。**保形弃权域**——两边实现都没有，`12` §G4 列为缺口，本版不做。**运行时续延做恢复**——宿主的 `async`、effect handler、协程都是 one-shot 且都不可序列化，都不能承担跨进程恢复；立着的提议是挂起与恢复只以账本为准，任何运行时续延只许作为同一次进程内的实现细节，不得让恢复路径绕过账本记账。现在写进规范比事后回收便宜，因为它的失效方式是 J-18 在没人注意时失效、账本从审计物悄悄退化成缓存。

两条范围说明：`14` 引用的两份原始代理结果（`foundation/experiments/审计/*.json`）留在研究工作区、不公开，所以 `14` 里那两个路径在本仓库解析不到；`14` 第九节列了五条要项目负责人裁的问题，其中一条涉及第三方资料，公开副本按 `tools/sync-from-workspace.sh` 对每份同步文档都执行的同一套脱敏机制有意不展开细节。

## Verification in this repository / 本仓库验证

| Check | Result |
|---|---|
| `python -m pytest -q`, Python 3.12.12 | 544 passed (identical to the pre-sync baseline on the same tree) |
| `python -m pytest -q`, Python 3.13.12 | 544 passed (identical to the pre-sync baseline) |
| Python sources changed | none — every `src/**/*.py` is byte-identical to `origin/main` |
| Non-document change under `src/` | one data file: `src/foundation/profile/profiles/jev-1.13.0.json`, which now carries the measured Chinese reliability curve |
| Browser bundle (`docs/demos/towow/lab/`) | rebuilt so `jpp-source.zip` and `manifest.json` match that data file; rebuilding from the *unchanged* sources first reproduced the recorded archive sha256 byte-for-byte, so the only content difference in the new archive is that one file |
| `tools/sync-from-workspace.sh --self-test` | PASS (replacement, deletion, and inline quotes not damaged) |
| Redaction markers applied to the synced copies | 6 (`DECISIONS.md` 3, `13` 2, `14` 1); 0 markers remain in `research/` |
| `rust/crates/jpp-core/src/interp.rs:1` | still「效应即时发出（惰性融合是后续优化，未迁移）」— the lazy work described above is not in this repository |
| `checked_add` / `checked_mul` under `rust/crates/jpp-core/src/` | 0 hits — the integer-overflow rule is not in this repository either |

## Verified on the workspace kernel, which is not in this repository / 在工作区内核上复跑（该内核不在本仓库）

These were re-run for this update on a clean extract of the research workspace's committed tree (`a0b0260`), outside that workspace and outside this repository. They are reported here because the update describes them; **none of this code is in this repository's `rust/`.**

| Check | Result |
|---|---|
| `cargo test` on the extracted workspace core | 111 passed, 0 failed, 1 ignored (a doc-test); 22 test binaries |
| `13`'s six rules (`tests/v13_rules.rs`) | 8 tests, all passing, each named after the clause it checks (`第一条` … `第六条`) |
| Fusion ablation (`tests/lazy_layers.rs`) | `fuse` on: 3 calls; `fuse` off: 6 calls — the 3 states × 2 questions figure quoted above, asserted in both directions |

## Workspace measurements, not re-run for this update / 工作区实测，本次未复跑

Source: `research/地基/DECISIONS.md` and `research/地基/14-实施计划-把语言做完整-v1.md`. The decision log records the suite growing 103 → 108 → 112 as the packages landed; the 111 above is what the committed tree actually gives today, and the difference is uncommitted work in the workspace, not a discrepancy to reconcile.

- `allocate` on a warm key, difference = random − allocate: `noul` in-band tie rate 89% → 51%, +0.068 at k=30 (0.110 against 0.178); `choice` 3% tie rate, +0.034 at k=10; `score` 75% tie rate, +0.085 at k=30. Two caveats travel with these figures in the source and are not dropped here: **no interval estimate was made and they do not extrapolate**, and the `score` column is the weak one — its high in-band tie rate is exactly the condition under which the ordering stops discriminating.
- The two agent rounds behind `14` cost $0; cumulative paid model spend for this line of work is about $0.35.

## Next design question / 下一个设计问题

The plan's own first gap is the root of five others: the execution model. With lazy layering landed and the pass switches in place, the open question is what a fused group is allowed to be. Effect rows are not enough — `ε={judge}` does not mean two judgments may be fused. What fusion actually needs is a structural summary: site, dependency, demand point, branch, order, loop, barrier. That summary does not exist in either implementation. Whether it belongs in the IR, so the planner can price a mis-speculation and the checker can name what blocked a fusion, is the next thing to decide.

计划自己的第一条缺口是另外五条的共同根：执行模型。惰性分层已落地、pass 开关已就位，接下来要定的是**一组可融合的题到底允许是什么**。效应行不够——`ε={judge}` 不等于可融合；融合真正要的是一份结构摘要：站点、依赖、需求点、分支、顺序、循环、屏障。这份摘要两边实现都没有。它该不该进 IR（好让计划器给推错定价、让检查器指出是什么挡住了融合），是下一个要定的问题。
