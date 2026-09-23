# Constructs, the live backend, and template-level calibration / 构造施工、真实后端与题式级校准

2026-09-23. Synced from the research tree through commit `42988c5` (research-tree path
`地基/rust-jpp`, commits since `d292f6d`). Ported: Rust source under `crates/`, `lib/*.jpp`,
`examples/` (including fixtures), `crates/*/tests`, `crates/jpp-core/INTERFACE.md`, and the
"getting usable exits on the live backend" section of `README.md`. Raw research-workspace
data and the human spot-check sheets under `进展/` and `实测/` are not synced; this document
summarizes their conclusions in aggregate only, with no individual annotation content and no
personal information.

同步来源是研究树 `地基/rust-jpp`，截至提交 `42988c5`，覆盖 `d292f6d` 以来的增量。搬运范围：
`crates/` 下的 Rust 源码、`lib/*.jpp`、`examples/`（含夹具）、`crates/*/tests`、
`crates/jpp-core/INTERFACE.md`，以及 README 中「怎么让真机跑出可用出口」一节。研究工作区
`进展/` 与 `实测/` 下的原始数据、人工抽检逐条内容不同步；本文只汇总其结论，不含个人信息、
不含逐条标注内容。

Every command below runs from `rust/` (this crate's root). The fixed-observation commands
were run against this sync and exit 0. The `--backend live` command needs a real
`~/.typesafe-key` and spends money, and the `calib-import` command needs a labels file you
supply; both are shown as templates, marked below.

以下命令都在 `rust/`（本 crate 根目录）下执行。带固定观察的命令已针对本次同步实际跑过、
退出码 0；`--backend live` 需要真实的 `~/.typesafe-key` 且会花钱，`calib-import` 需要你
自己给的标注文件，这两条是模板，已在下方标出。

## What got built / 做成了什么

- **Real JEV backend (`--backend live`).** `jpp run` can now talk to the real JEV service
  through `JevClient`; credentials are read only from `~/.typesafe-key`, never logged or
  written to a report. Replay reconstructs the client-independent `model_id` from the
  ledger header, so a live-backed run replays with zero new calls. Template — spends money,
  not run as part of this sync:
  ```sh
  cargo build -p jpp-cli --features live --release
  ./target/release/jpp run examples/sieve.jpp --backend live --calib calib --ledger-out ledger.json
  ```
  真实 JEV 后端：`jpp run --backend live` 接通真机，凭据只从 `~/.typesafe-key` 读，不进日志
  或报告；重放从账本头读回 `model_id`，真机跑一次后可零调用重放。模板命令——会花钱，本次
  同步没有执行。

- **Questions as first-class values (`form`/`fill`, readable question fields).** A question
  now decomposes into a template (`form`) and its filling (`fill`), with readable fields a
  program can inspect and use to compute new questions.
  ```sh
  cargo run -p jpp-cli -- run examples/question-forms.jpp --fixtures examples/fixtures/question-forms.json
  ```
  题成为一等值：题式（`form`）与填法（`fill`）分离，题字段可读、可参与计算生成新题。

- **Three-way sieve that takes questions directly, plus review material.** `sieve` now
  consumes questions directly and routes into act/ignore/unsure/unobserved; same-state
  questions fuse into one call; review opinions render into material via `review_material`.
  ```sh
  cargo run -p jpp-cli -- run examples/sieve-review.jpp --fixtures examples/fixtures/sieve.json
  ```
  三路过滤直接吃题：act/ignore/unsure/unobserved 四流分派，同状态题合并成一次调用，评审
  意见经 `review_material` 渲染成材料。

- **Pairing (`pair`).** Two candidate groups, or caller-given candidate pairs, produce a
  relation record routed by the same sieve; the composed relation can itself become new
  material for a second round of pairing.
  ```sh
  cargo run -p jpp-cli -- run examples/pair-team.jpp --fixtures examples/fixtures/pair-team.json
  ```
  配对 `pair`：两组材料或给定候选对产出关系记录，交三路过滤分流，组合结果可再配对。

- **Set aggregation (`tally`/`first_k`).** Counting under undecided answers returns a
  precise interval, not a false point estimate; ordered top-k selection declines to claim a
  result once it hits an undecided item.
  ```sh
  cargo run -p jpp-cli -- run examples/tally-budget.jpp --fixtures examples/fixtures/tally.json
  ```
  聚合 `tally`/`first_k`：存在未决时计数给区间而非假点估计；`first_k` 遇到未决即不宣称结果。

- **Bounded iteration with a second termination line (`iterate`).** Step cap plus a strict
  shrink requirement on the material at each layer (`noshrink`), with the termination reason
  written into the result.
  ```sh
  cargo run -p jpp-cli -- run examples/iterate.jpp --fixtures examples/fixtures/iterate.json
  ```
  迭代 `iterate` 带第二条终止线：步数上限之外，每层材料必须严格变少（`noshrink`），终止
  原因写进结果。

- **Calibration intake (`calib-import`, the truth channel).** A CLI entry point that folds
  labelled readings (human / computed / model-labelled) into calibration records and
  certifies them; model-only labels are certified only when a same-template human spot
  check reaches the gate. Template — `labels.jsonl` is a file you supply, one JSON object
  per line, e.g. `{"form": {"op": "test", "template": "..."}, "item": "n1", "p": 0.99,
  "label": true, "source": "computed"}` (verified against a synthetic 200-row file during
  this sync: `--calib-out` produced a certified record with `status: "上岗"`):
  ```sh
  cargo run -p jpp-cli -- calib-import labels.jsonl --calib-out calib --spot-check-min 0.9
  ```
  校准进料 `calib-import`（真值通道）：把带真值的读数（人工/构造/模型标注）折进校准记录
  并认证；只有模型标注时，需要同题式人工抽检达到门槛才能上岗。模板命令——`labels.jsonl`
  由你自己提供，每行一个 JSON 对象；本次同步用一份 200 行的合成标注文件验证过，
  `--calib-out` 确实产出了 `status: "上岗"` 的认证记录。

- **Form-level line fallback.** A calibration lookup falls back from the literal question key
  to its template key, then to a mode-level key, with the fallback path recorded on the exit.
  题式级线回退：校准查找从字面题键回退到题式键，再回退到模式级键，回退路径记在出口上。

- **Split-sample two-sided certification (B24).** Threshold selection and certification now
  use disjoint halves of the labelled set (select on one half, certify once on the other);
  the certificate records the method, seed, and both halves' sizes. Both the upper (`hi`)
  and lower (`lo`) exit boundaries get their own certificate.
  拆分样本两侧认证（B24）：选线与认证分用不重叠的两半（选线半选线、认证半上只检验一次
  选出的那一对），证书记方式、种子、两半条数；`hi`/`lo` 各有自己的证书。

- **Composition-closure contract (B17).** `sieve` / `pair` / `tally` / `first_k` / `iterate` /
  `outcome` now return the same contract value
  `{kind, value, pending, evidence, resume, spent, detail, purpose}`; the product of one
  composition satisfies the same interface a basic unit does, so it can re-enter another
  composition.
  ```sh
  cargo run -p jpp-cli -- run examples/contract.jpp --fixtures examples/fixtures/contract.json
  ```
  组合封闭性契约（B17）：五个组合算子与 `outcome` 统一返回同一契约值，产物与基本单元
  同接口，可再次参与组合；未决随包转移，丢弃契约值在检查阶段报错（J-05）。

## What was found / 发现了什么

- **The live backend returns readings, but every exit is `unsure(cold)` without a
  calibration record.** JEV's readings themselves were correct on a synthetic probe
  (`target<500` answered `p=0.01`), but `cut` needs a certified line to turn a reading into
  act/ignore, and a fresh program has no such line. This was the head blocker for making
  J++ usable on the real backend; `calib-import` (above) closes it.
  真机无校准时全部出口都是 `unsure(cold)`：JEV 的读数本身是对的，但 `cut` 要靠已认证的
  线才能给出 act/ignore，新程序没有线可用。这是真机可用性的头号阻塞，靠 `calib-import`
  这条真值通道打开。

- **Literal question templates read bimodally; a single global line suffices.** On mention/
  membership-style templates, readings cluster near 0 or 1; one global threshold band
  (0.3–0.5) produced zero to low-single-digit misclassifications across several template
  families. Calibration only matters where reading density sits near the middle.
  字面题式读数两极：提及/成员判断这类题式的读数集中在 0 或 1 附近，一条全局线
  （0.3–0.5）在多个题式族上零到个位数错判；只有读数密度落在中间的题式才需要认真校准。

- **Semantic templates need calibration, misclassifications persist, and re-asking is
  useless.** A "does this mention {concept}" template over ~410 items showed a real middle
  band (about 15% of readings), and misclassifications did not change on 16/16 reruns —
  the error is systematic, not noise a repeated question would average out.
  语义题式需要校准、错判持久、重问无效：「是否提到{概念}」这类题式约 410 条中出现明显
  中间地带（约 15% 的读数），错判在 16/16 次重跑里保持不变——错误是系统性的，不是能靠
  重复提问抹平的噪声。

- **Human spot-checking exposed an undefined question scope, not labelling noise.** An
  initial spot check against model labels landed at 78% agreement (below the 0.9 gate); all
  disagreements ran one direction — the human judge treated associated/adjacent topics as
  "mentioned," the model did not. Splitting the question into a narrower "directly named or
  referenced" template and a broader "topic-relevant" template resolved this: the
  topic-relevance template reached 30/30 agreement (one-sided 95% lower bound ≈0.905,
  clearing the 0.9 gate) and is certified for production use; the narrower template still
  sits at 84%, below the gate.
  人工抽检揭示的是题面外延未定，不是标注噪声：初次抽检与模型标注一致率 78%（低于 0.9
  门槛），全部分歧同向——人工判断把话题相邻内容算作「提到」，模型没有。把题拆成更窄的
  「直接写出或代称指向」与更宽的「话题相关」两道题式后：话题相关题式抽检 30/30 一致
  （单侧 95% 下界约 0.905，过门槛），转正上岗；更窄的题式仍停在 84%，未过门槛。

## What was designed in response / 针对问题做了什么设计

- **The question template (`form`), not the literal question or its filling, is the
  calibration primary key.** Misclassifications on semantic templates trace back to
  referent ambiguity and concept-boundary vagueness that keying by filling or candidate
  class cannot remove, and the error persisting across reruns rules out per-instance noise
  as the cause (supports the ledger's B9 ruling). This became the B2 ruling.
  题式（`form`）而非字面题或填法是校准主键：语义题式的错判根源是代称指代与概念边界
  含糊，按填法或候选类细分键消不掉这个问题，错误在重跑间保持一致也排除了逐条噪声这个
  解释（支持总账 B9）。写成 B2 裁定。

- **Split-sample two-sided certification (B24) plus multiple-comparison discipline.**
  Selecting a threshold and certifying it on the same data invalidates the pointwise
  binomial bound; certification now selects on one half of the labelled set and certifies
  once on the other, and both the `hi` and `lo` exit boundaries get independent
  certificates. The certificate records its method, seed and both halves' sizes so the
  measurement is auditable.
  拆分样本两侧认证（B24）加多重比较纪律：在同一批数据上选线又认证会让逐点二项上界失效，
  改为在标注集的一半上选线、在另一半上只对选出的那一对认证一次；`hi`/`lo` 两侧各自独立
  认证。证书记录方式、种子与两半条数，测量过程可审。

- **The model-labelling gate is a one-sided 95% confidence lower bound on the human
  spot-check agreement rate, not the raw point estimate.** A raw 25/25 agreement rate looks
  like 100%, but its lower bound (~0.887) still falls short of the 0.9 gate; the topic-
  relevance template only certified after five more agreeing checks pushed the lower bound
  to ~0.905. This is the B19 correction.
  模型标注上岗门槛按人工抽检一致率的单侧 95% 置信下界判定，不看原始点估计：25/25 的
  原始一致率看着是 100%，但下界（约 0.887）仍不过 0.9 门槛；话题相关题式追加 5 条抽检、
  下界推到约 0.905 才转正。这是 B19 的修正。

- **Composition-closure contract (B17).** Nature's requirement that composed programs
  become elements of larger compositions was formalized as a single return shape shared by
  every set-level construct; pending values carry through the contract instead of being
  silently dropped, and a contract value discarded without consuming its pending field is a
  static (J-05) error, not a runtime surprise.
  组合封闭性契约（B17）：把「组合的产物要能再当元素参与更大组合」这条要求，落实成所有
  集合级构造共用的一个返回形状；未决值随契约转移而不是被悄悄丢弃，丢弃一个未消费未决字段
  的契约值是静态错误（J-05），不是运行期意外。

- **Changes carried from the question-theory literature review.** A four-line review
  (question logic, probabilistic information/decision theory, philosophy of method, and
  query learning/algebra) produced a five-part definition of a question (subject,
  predicate, partition, request, presupposition) now reflected in `form`/`fill`, and
  informed the B2/B19/B24 rulings above; the review's full registry proposals remain
  pending Nature's review and are not yet all in the authority texts.
  问题理论调研带来的改动：四线调研（问题逻辑、概率信息决策论、方法论哲学、查询学习与
  查询代数）给出题的五件套定义（主体、谓词、划分、请求、前提），现体现在 `form`/`fill`
  的结构里，并为上述 B2/B19/B24 裁定提供依据；调研给出的完整登记表提议仍待 Nature 审阅，
  尚未全部写入依据文本。

## Unfinished and known issues / 未完成与已知问题

- The project's design ledger (`18-设计总账-v1.md`) tracks 415 registered design items; 89
  are built, 233 are decided but not built, 51 have only been experimented on without a
  ruling, and 42 have been superseded by later rulings.
  项目设计总账（`18-设计总账-v1.md`）共登记 415 条：已造出 89 条，已定未造 233 条，只做过
  实验未定 51 条，被取代 42 条。

- **Two classes of fail-open defects are being fixed, not yet fixed.** (1) `speculate` can
  execute a user's `do` effect on a branch that should not have run. (2) Untrusted content
  can become "trusted" via string concatenation, `join`, or a failure path, and then pass an
  irreversible `do` gate that should have blocked it. Both were found during the ledger
  review and are ruled to need a fix plus a regression test; neither fix has landed in code
  as of `42988c5`.
  两类放行方向的缺陷正在修，尚未修好：(1) `speculate` 可能在不该执行的分支上把用户的
  `do` 效应真的执行了；(2) 不可信内容经拼接、`join` 或失败路径变得「可信」后，能越过本该
  拦住它的不可逆 `do` 关卡。两条都是总账复核时发现的，已裁定要修并补回归测试；截至
  `42988c5`，两处修复都还没有落进代码。

- **B28–B32 (kernel-layer ledger rulings) are decided but not yet in code.** These cover
  repeated-reading aggregation (mean/median only, no exit-mode voting), host-side J-03
  constraints, the calibration-key tuple, routing pending truth checks through `ask`
  instead of adding a fifth exit, and the absence-of-judgment and latency-budget seams. They
  are written into the authority texts (`12`) but not yet implemented.
  B28–B32（内核层总账裁定）已定未造：分别是重复读数只许均值/中位数聚合（不许出口
  众数投票）、宿主侧 J-03 约束、校准键元组、待真值走 `ask` 而不是新开第五去向、判断力
  缺席与时延预算两条缝。已写入依据文本（`12`），尚未实现。

- **Split-sample holdout certification (B24) is wired only on the `calib-import` path.**
  `calib-import` calls `truth::import_labels` → `commission_two_sided`, which does select on
  one half and certify on the other. The older entry points `certify`, `commission` and
  `commission_costed` (used directly by Rust callers, not through the CLI) still select a
  threshold and certify it on the same samples — the "no selection correction, no
  independent holdout" caveat already disclosed in `conformal.rs`'s and `effects.rs`'s doc
  comments, and in [the prior PR-integration review](2026-09-23-pr-integration.md), still
  applies to those. The composition-closure contract (B17) itself is a return shape, not a
  certificate; it carries no statistical claim.
  拆分样本留出认证（B24）目前只接在 `calib-import` 这条路径上：它经
  `truth::import_labels` → `commission_two_sided`，真的做到选线半选线、认证半认证。更早
  的入口 `certify`、`commission`、`commission_costed`（供 Rust 调用方直接用，不经 CLI）
  仍然在同一批样本上选线又认证——「无选择校正、无独立留出集」这条边界（`conformal.rs`
  与 `effects.rs` 的文档注释、以及[此前的 PR 整合审查](2026-09-23-pr-integration.md)已
  写明）对它们依然成立。组合封闭性契约（B17）本身只是一个返回形状，不是证书，不带统计
  意义上的保证。

## Verification / 验证

`cargo build --workspace --offline` and `cargo build -p jpp-cli --features live --offline`
both succeed. `cargo test --workspace --offline`: 341 passed, 0 failed, 3 ignored.
`cargo test -p jpp-cli -p jpp-core --features live --offline`: 328 passed, 0 failed, 3
ignored (same tests as the default run for these two crates; the `live` feature does not
change test outcomes). No test in this run performs a real network call to the JEV backend
or spends any money.

`cargo build --workspace --offline` 与 `cargo build -p jpp-cli --features live --offline`
均成功。`cargo test --workspace --offline`：341 通过、0 失败、3 忽略。
`cargo test -p jpp-cli -p jpp-core --features live --offline`：328 通过、0 失败、3 忽略
（与默认构建的这两个 crate 测试相同，`live` feature 不改变测试结果）。本轮测试不发生
任何真实网络调用，不产生费用。
