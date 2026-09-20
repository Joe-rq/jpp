# jpp-core 接口说明（给前端 lower 与 CLI 接线用）

版本：首包，2026-09-21。对应 crate `jpp-core 0.1.0`，edition 2024。

core 负责共同程序表示、值与环境、静态检查、解释执行、效应适配、账本与重放。它不解析源码：前端保留
自己的带 `Span` AST，写 `lower()` 映射到这里的 `ast::Program`。语义依据是 `12-IR与类契约-v0.1.md`
的六种效应形式与 J-01…J-18，诊断编号依据 `11-语言规范-v1.md` §诊断；Rust 自己的类型系统不替代
J++ 的检查器，纪律由 `check.rs` 与 `interp.rs` 两处把关。

## 一、程序表示

```rust
pub struct Program { pub budget: Option<Budget>, pub body: Block, pub span: Span }
pub struct Budget  { pub calls: u64, pub cost: f64, pub depth: Option<u32>, pub escalate: Option<u64> }
pub struct Block   { pub statements: Vec<Statement>, pub result: Option<Box<Expr>>, pub span: Span }
pub struct Expr    { pub kind: ExprKind, pub span: Span }
pub struct Span    { pub start: usize, pub end: usize }   // 字节偏移，与前端一致

pub enum Statement {
    Let { name: String, annotation: Option<Type>, value: Expr, span: Span },
    Function { name: String, function: Function, span: Span },
    Expression(Expr),
}

pub enum ExprKind {
    Integer(i64), Decimal(f64), Bool(bool), Text(String), Unit,
    Name(String), List(Vec<Expr>), Record(Vec<(String, Expr)>),
    Function(Function),
    Call { function: Box<Expr>, arguments: Vec<Expr> },
    Field { value: Box<Expr>, field: String },
    Index { value: Box<Expr>, index: Box<Expr> },
    Unary { op: String, value: Box<Expr> },
    Binary { op: String, left: Box<Expr>, right: Box<Expr> },
    If { condition: Box<Expr>, yes: Block, no: Block },
    Block(Block),
}

pub struct Function {
    pub parameters: Vec<Parameter>,
    pub result_type: Option<Type>,
    pub effects: Option<Vec<String>>,   // None = 未声明；Some([]) = 显式纯
    pub body: Block,
}
pub struct Parameter { pub name: String, pub annotation: Option<Type>, pub span: Span }
pub enum Type { Named(String), Applied(String, Vec<Type>), Function(Vec<Type>, Box<Type>) }
```

没有效应专用节点。`judge`、`do`、`gen`、`ask`、`transform` 都是普通 `Call`，名字在根环境里解析成
`Value::Builtin`；检查器与解释器按名字认它们。前端不需要为效应造节点，只要把调用原样 lower 过来。

`budget` 缺失保持 `None`，由检查器报 E12——前端不要造默认预算。`effects` 里的名字目前只认
`judge` / `gen` / `do` / `ask`；`transform` 是记账变换，不是效应形式，不写进标注。

全部节点 `#[derive(Serialize, Deserialize)]`，serde 默认表示，可往返。外部表示形状见下面第六节的样例。

## 二、值与环境

```rust
pub enum Value {
    Unit, Int(i64), Float(f64), Bool(bool), Text(Rc<str>),
    List(Rc<Vec<Value>>), Record(Rc<Vec<(String, Value)>>),
    Fn(Rc<Closure>), Builtin(&'static str),
    Mat(Rc<Mat>), State(Rc<State>), Question(Rc<Question>), Reading(Rc<Reading>), Exit(Rc<Exit>),
    Fail(Rc<str>),          // do 失败是值，不是异常（J-12）
    Stop(Rc<Value>),        // loop 的显式停止
}

pub struct Closure { pub function: Function, pub env: Env, pub name: Option<String>, pub span: Span, pub hash: String }
pub struct EnvNode { pub vars: RefCell<Vec<(String, Value)>>, pub parent: Option<Env> }
pub type Env = Rc<EnvNode>;
```

**函数值 = 参数表 + 函数体（core AST）+ 显式环境链**，没有 Rust 闭包。环境是一串名字→值的节点，
可以打印（`env_names`）、可以跟着链走。`Value::to_json()` 把函数值渲染成
`{"fn": 名字, "params": [...], "env": [[本层名字…], [上层名字…]], "hash": …}`；账本里存的是效应记录，
不是序列化的闭包，重放靠同一份程序重建环境。

一个块里的绑定共享同一个环境节点，所以自递归和「后定义、先被前面的函数体引用」都成立。检查器的名字
解析按同一口径：函数体里能看见所属块的全部绑定，语句位置的直接引用才要求先定义后使用。

读数（`Value::Reading`）没有可读的值：不能比较、不能做算术、不能进状态槽、不能取字段，只能经 `cut`
变成出口（J-01）。`Value::to_json()` 对读数只露 `{reading: 账本键, q, state, op}` 这几项元数据。

## 三、内置操作与签名

根环境里的名字（`interp::BUILTINS`）。用户可以用同名绑定盖住它们，检查器会报 `W-shadow` 提示。

**材料与状态**

| 签名 | 说明 |
| --- | --- |
| `mat(v) -> Mat` | 任意值变材料；出口也能变回材料（带 `derived_from`，J-02 禁自指） |
| `content(m: Mat) -> Value` | 取材料内容。读数进来是 J-01 错 |
| `state(on) -> State` / `state(on, {ctx, ref, over}) -> State` | `on` 恰一个判断对象（关系用一对，J-14）；`over` 是候选集 |
| `transform(f: Fn, mats…) -> Mat` | 记账变换：进槽的材料只能来自字面量、效应输出或这里（J-11）。输出不能是读数/出口/函数/状态 |

**题与判断**

| 签名 | 说明 |
| --- | --- |
| `test(题面: Text, calib: Text) -> Question` | 是非题，下沉成 noul |
| `select(题面: Text, calib: Text) -> Question` | 在 `over` 里挑一个，下沉成 choice |
| `measure(题面: Text, [档位: Text…], calib: Text) -> Question` | 分档题，至少两档，下沉成 score |
| `judge(state, question) -> Reading` / `judge(state, [question…]) -> [Reading]` | 一状态多题一次问完 |
| `cut(reading) -> Exit` / `cut(reading, calib_key: Text) -> Exit` | 过线。`calib` 位只收校准记录的**键**，字面量线是 J-03 错 |
| `ask(state, question) -> Exit` | 问人。没答就是程序级 Pending；次数受 `budget.escalate` 管（E10） |

`calib` 是校准记录的键，不是线。线只从 `CalibStore` 里的记录来；记录状态是「冷」或「停岗」时
`cut` 直接给 `Unsure(cold)` / `Unsure(drift)`，不看概率。

**出口**

| 签名 | 说明 |
| --- | --- |
| `handle(exit, {act, ignore, pick, at, unsure, otherwise}) -> Value` | 按题型穷尽：test 要 `act`/`ignore`/`unsure`，select 要 `pick`/`unsure`，measure 要 `at`/`unsure`；`otherwise` 兜底。臂可以是值，也可以是方法（`pick`/`at` 的方法收一个 Int，`unsure` 的收一个 Text 原因） |
| `consume(exit \| [exit…], "drop") -> Unit` | 显式丢弃并记账 |
| `exit_kind(exit) -> Text` | `act` / `ignore` / `pick(k)` / `at(l)` / `unsure(cause)` |
| `unsure(cause: Text) -> Exit` | 源码自己造一个未决出口 |
| `pending(reason: Text)` | 程序级挂起，整个程序停在这里 |

每个出口都带 `consumed` 标记。程序返回前还有未消费的 `Unsure` 就是 J-05 错；函数要把出口带出去，
返回类型必须提到 `Exit`（`-> Exit`、`-> Record<Exit>` 之类），最外层允许带出并记在
`Outcome.returned_unsure` 里。

**效应与失败**

| 签名 | 说明 |
| --- | --- |
| `do(动作名: Text, [参数…], iter_seq: Int) -> Mat \| Fail` | 只能触发 `ActionRegistry` 登记过的动作（J-11）。循环里 `iter_seq` 必须随轮次变（J-13） |
| `gen(prompt: Text, [ctx], n: Int, retry_seq: Int) -> [Mat]` | 同上，重试必须递增 `retry_seq` |
| `fail(reason: Text) -> Fail` / `is_fail(v) -> Bool` | Fail 是值，进账本，判断时走 `Unsure(fail)`（J-12） |

**有界控制与数据**

`loop(bound: Int, 初值, fn(acc, i))` 是唯一的循环，`bound` 必须在（E5），体内 `stop(v)` 显式停止；
一轮之内账本键重复即停（J-06）并报 `W-noprogress`。`map(list, fn)` / `filter(list, fn)` 是纯映射，
体内不能含 `loop` / `stop`（E7）。`fold(list, 初值, fn(acc, x))` 用来表达带状态的迭代。

其余：`len`、`range(a, b)`、`append`、`concat`、`slice`、`contains`、`sum`、`reverse`、`keys`、
`has(record, key)`、`with(record, key, value)`、`text`、`join`、`print`、`min`、`max`、`abs`、`floor`。

**类型名**（标注里认得的）：`Int`、`Decimal`/`Float`、`Bool`、`Text`、`List`、`Record`、`Unit`、
`Fn(…)->…`。`Mat`、`State`、`Question`、`Reading`、`Exit`、`Method` 这些标了也接受，但静态不据此
判参数——它们由运行期把关。检查器只在「标注是上面那几个基本档、实参又是字面量」时判不符（`E-type`）。

## 四、运行 API

```rust
pub fn run(
    program: &Program,
    client: &mut dyn Client,
    calib: &CalibStore,
    actions: &ActionRegistry,
    ledger: &mut Ledger,
) -> Result<Outcome, Error>;

pub enum Error { Check(Report), Runtime(RtError) }   // 都带 Span，都能 render()
```

`run` 先静态检查，有错就不执行。要绕过检查器单独试解释器用 `run_unchecked`（只给 core 自己的对照
测试用）。也可以直接 `Interp::new(client, ledger, calib, actions, budget).run(program)`——CLI 现在
走的就是这条，预算要自己从 `program.budget` 取。

```rust
pub struct Outcome {
    pub value: Option<Value>,        // 挂起时为 None
    pub pending: Vec<Pending>,       // 程序级挂起：预算耗尽、ask 未答、显式 pending
    pub trace: Trace,
    pub cost: Cost,                  // calls / replayed / tokens / usd / asks
    pub returned_unsure: Vec<String>,
}
pub struct Pending { pub cause: String, pub key: String, pub site: Span, pub detail: String }
```

**两种未完成要分开看**：`Outcome.pending` 是程序被挂起了；算法自己的未决候选是程序**返回值**里的
普通数据，程序照常跑完。部分结果能交付，不等于每道题都有答案。

**Client**（观察的来源）：

```rust
pub trait Client {
    fn model_id(&self) -> String;
    fn judge(&mut self, state: &State, questions: &[&Question]) -> Result<JudgeResult, EffectError>;
    fn generate(&mut self, prompt: &str, ctx: &[Json], n: usize, retry_seq: u64) -> Result<Vec<Json>, EffectError>;
    fn ask(&mut self, state: &State, q: &Question) -> Result<Option<Answer>, EffectError>;
    fn calls(&self) -> u64;
}
```

> ⚠️ 这个方法本来叫 `gen`，edition 2024 把 `gen` 收成保留字，Rust 侧改名 `generate`。J++ 里的内置
> 名字仍然是 `"gen"`，源码不受影响。

- `FixedClient`：固定观察。键是 `obs_key(state, question)` = 状态规范 JSON 的哈希 + 题面 + 档位。
  **未命中就是错，不猜答案**。`observe(&state, &q, answer)` 登记一条；`model_id()` 是 `"fixed-0"`。
- `NoCallClient`：拒绝一切调用，重放验证用。`model_id()` 同为 `"fixed-0"`，所以同一本账本能直接接上。
- `JevClient`：真机，POST `/v1/systemone`，密钥只从 `~/.typesafe-key` 读。本包不发调用，HTTP 走
  `live` 特性。

**动作登记**：

```rust
let mut actions = ActionRegistry::new();
actions.register("record_check", 0.0, true, TaintOut::Inherit, |args| Ok(args[0].clone()));
//               名字            成本  可逆  输出 taint     实现
```
没登记的动作被 `do` 触发是 J-11 错。`TaintOut` 是 `Trusted` / `Untrusted` / `Inherit`；按 §2.11，
`trusted` 是**显式标记**，语言保证它可见可追，不设审核方。

**校准记录**：`CalibStore::put(键, hi, lo, n, 状态)`，状态是 `上岗` / `冷` / `停岗` / `待真值`；
`上岗` 必须 `n > 0`。没有记录的键回退成「冷」，`cut` 给 `Unsure(cold)`——这是最容易静默走偏的地方，
接线时 `put` 的返回值要 unwrap，别吞掉。

**账本与重放**：`Ledger` 既是输出也是输入。把上一次的账本传回来即重放，命中的键零调用；配
`NoCallClient` 可以验证「同程序重放零调用」。账本头（预算、model_id、渲染版本、handler 版本）不同
会报 `W-header`，进 `Trace.warnings`，不承诺重放一致（J-18）。反序列化出来的账本要先
`rebuild_index()`。

## 五、诊断

```rust
pub fn check(program: &Program) -> Report;

pub struct Report { pub diagnostics: Vec<Diagnostic> }
impl Report {
    pub fn errors(&self) -> Vec<&Diagnostic>;     // 非空即不应运行
    pub fn warnings(&self) -> Vec<&Diagnostic>;
    pub fn is_ok(&self) -> bool;                  // 没有错；warning 不拦
    pub fn find(&self, rule: &str) -> Option<&Diagnostic>;
    pub fn render(&self) -> String;
}

pub struct Diagnostic { pub rule: String, pub severity: Severity, pub message: String, pub span: Span }
pub enum Severity { Error, Warning }
```

诊断按 `span.start` 排序。报文格式是「一句话说错在哪。修法：…」，前端拿 `span` 渲染到 `.jpp` 的
文件/行/列。`Diagnostic` 与 `Report` 都可 serde 序列化。

**静态检查的错**

| 规则 | 判什么 |
| --- | --- |
| `E5` | `loop` 缺 bound，或 bound 是非正整数字面量 |
| `E7` | `map` / `filter`（`for…yield`）的体内含 `loop` / `stop` |
| `E10` | 程序里有 `ask` 而 `budget.escalate` 是 0 |
| `E12` | 程序缺 `budget` |
| `J-01` | 读数进状态槽、做算术、做比较、取字段、当 `if` 条件、当 `handle` 的第一个参数 |
| `J-03` | `cut` / `test` / `select` / `measure` 的 calib 位是数字字面量（线不可字面） |
| `J-05` | 出口绑定后再没被提到；`handle` 的字面臂表缺 `unsure` 与 `otherwise`；函数直接返回出口而返回类型没提 `Exit` |
| `J-13` | 循环体内 `do` / `gen` 用常量序号 |
| `J-14` | `state` 的 `on` 槽字面量超过两个对象 |
| `E-name` | 未定义的名字；语句位置引用了后面才定义的绑定 |
| `E-arity` | 具名方法的参数个数不对 |
| `E-type` | 实参字面量与参数标注的基本类型不符 |
| `E-effect` | `!{…}` 标注少了函数体里实际会发生的效应 |

**提示**（不拦程序）：`W-shadow` 盖住内置名、`W-bound` loop 的 bound 不是字面量、`W-seq-const`
循环里的常量序号但键还不碰撞、`W-effect` 标了用不上的效应。

**运行期的错**（`RtError`，也带 `Span` 与规则号）：J-02 禁自指、J-05 的 `consumed` 返回前核、
J-06 键重复即停、J-11 动作未登记、E5 调用深度超限、类型不符、固定观察未命中。运行期的提示进
`Trace.warnings`：`W-header`、`W-bound`、`W-noprogress`、`returned_unsure`。

检查器的口径是**宁可漏报也不误报**：静态判不准的一律交给运行期。被调者是参数里的方法值时，效应集
当作未知，整条 `E-effect` 跳过；`Type` 里 core 认不得的名字不参与 `E-type`。

## 六、core AST 的外部表示

`budget {calls: 10, cost: 0, depth: 64}; fn twice(x: Int) -> Int !{} { x * 2 } twice(21)` 完整 JSON：

```json
{
  "budget": {"calls": 10, "cost": 0.0, "depth": 64, "escalate": null},
  "body": {
    "statements": [
      {"Function": {
        "name": "twice",
        "function": {
          "parameters": [{"name": "x", "annotation": {"Named": "Int"}, "span": {"start": 49, "end": 55}}],
          "result_type": {"Named": "Int"},
          "effects": [],
          "body": {
            "statements": [],
            "result": {"kind": {"Binary": {
              "op": "*",
              "left":  {"kind": {"Name": "x"},    "span": {"start": 70, "end": 71}},
              "right": {"kind": {"Integer": 2},   "span": {"start": 74, "end": 75}}
            }}, "span": {"start": 70, "end": 75}},
            "span": {"start": 68, "end": 77}
          }
        },
        "span": {"start": 40, "end": 77}
      }}
    ],
    "result": {"kind": {"Call": {
      "function": {"kind": {"Name": "twice"}, "span": {"start": 78, "end": 83}},
      "arguments": [{"kind": {"Integer": 21}, "span": {"start": 84, "end": 86}}]
    }}, "span": {"start": 78, "end": 87}},
    "span": {"start": 0, "end": 87}
  },
  "span": {"start": 0, "end": 87}
}
```

两个必保留程序里的关键节点（省略 `span`，实际都带）：

```jsonc
// cut(judge(state(m), q))
{"kind":{"Call":{"function":{"kind":{"Name":"cut"}},"arguments":[
  {"kind":{"Call":{"function":{"kind":{"Name":"judge"}},"arguments":[
    {"kind":{"Call":{"function":{"kind":{"Name":"state"}},"arguments":[{"kind":{"Name":"m"}}]}},
    {"kind":{"Name":"q"}}]}}}]}}

// handle(e, {act: fn() { 1 }, ignore: fn() { 0 }, unsure: fn(cause) { cause }})
{"kind":{"Call":{"function":{"kind":{"Name":"handle"}},"arguments":[
  {"kind":{"Name":"e"}},
  {"kind":{"Record":[
    ["act",    {"kind":{"Function":{"parameters":[],"result_type":null,"effects":null,
                                    "body":{"statements":[],"result":{"kind":{"Integer":1}}}}}}],
    ["ignore", {"kind":{"Function":{"parameters":[],"result_type":null,"effects":null,
                                    "body":{"statements":[],"result":{"kind":{"Integer":0}}}}}}],
    ["unsure", {"kind":{"Function":{"parameters":[{"name":"cause","annotation":null}],
                                    "result_type":null,"effects":null,
                                    "body":{"statements":[],"result":{"kind":{"Name":"cause"}}}}}}]
  ]}}]}}

// loop(10, s, fn(acc, i) { stop(acc) })
{"kind":{"Call":{"function":{"kind":{"Name":"loop"}},"arguments":[
  {"kind":{"Integer":10}}, {"kind":{"Name":"s"}},
  {"kind":{"Function":{"parameters":[{"name":"acc","annotation":null},{"name":"i","annotation":null}],
   "result_type":null,"effects":null,
   "body":{"statements":[],"result":{"kind":{"Call":{"function":{"kind":{"Name":"stop"}},
                                               "arguments":[{"kind":{"Name":"acc"}}]}}}}}}}]}}

// do("record_check", [r], i)
{"kind":{"Call":{"function":{"kind":{"Name":"do"}},"arguments":[
  {"kind":{"Text":"record_check"}}, {"kind":{"List":[{"kind":{"Name":"r"}}]}}, {"kind":{"Name":"i"}}]}}

// transform(fn(old) { with(content(old), "k", 1) }, m)
{"kind":{"Call":{"function":{"kind":{"Name":"transform"}},"arguments":[
  {"kind":{"Function":{"parameters":[{"name":"old","annotation":null}],"result_type":null,"effects":null,
   "body":{"statements":[],"result":{"kind":{"Call":{"function":{"kind":{"Name":"with"}},"arguments":[
     {"kind":{"Call":{"function":{"kind":{"Name":"content"}},"arguments":[{"kind":{"Name":"old"}}]}},
     {"kind":{"Text":"k"}}, {"kind":{"Integer":1}}]}}}}}},
  {"kind":{"Name":"m"}}]}}

// measure("多大把握", ["low", "high"], "conf")
{"kind":{"Call":{"function":{"kind":{"Name":"measure"}},"arguments":[
  {"kind":{"Text":"多大把握"}},
  {"kind":{"List":[{"kind":{"Text":"low"}},{"kind":{"Text":"high"}}]}},
  {"kind":{"Text":"conf"}}]}}

// if xs[0].cost <= 2 { [1, 2] } else { {a: 1} }
{"kind":{"If":{
  "condition":{"kind":{"Binary":{"op":"<=",
    "left":{"kind":{"Field":{"value":{"kind":{"Index":{"value":{"kind":{"Name":"xs"}},
                                              "index":{"kind":{"Integer":0}}}}},"field":"cost"}}},
    "right":{"kind":{"Integer":2}}}}},
  "yes":{"statements":[],"result":{"kind":{"List":[{"kind":{"Integer":1}},{"kind":{"Integer":2}}]}}},
  "no": {"statements":[],"result":{"kind":{"Record":[["a",{"kind":{"Integer":1}}]]}}}}}}
```

要看完整的一份，把 lower 的结果 `serde_json::to_string_pretty(&program)` 打出来即可；两个程序的
手工构造版本在 `crates/jpp-core/tests/adaptive.rs` 与 `tests/partial.rs`，文件头的注释里有对应的
J++ 源码写法。

## 七、当前实现与历史规范的差异

以下是首包的实际范围及尚未统一的规范口径。通过示例测试不表示全部历史规范都已实现；
后续语义修改需要同时更新规范、实现与行为测试。

1. **E12 还是 E10。** `11-语言规范-v1.md` §诊断把「`budget` 缺失」与「`escalate` 出现而
   `budget escalate` 为 0」都编为 **E10**，**E12** 是「静态可估的最省计划超出 budget」（见该文件
   第 139、140 行）。但 CLI 的 `runner.rs` 与承接简报都按 **E12** 发「预算必填」。一次交付里两个
   组件对同一个错发两个码更糟，所以 core 按团队约定发 **E12**，把「ask 无升级预算」单独发 **E10**。
   诊断码仍需与共同规范统一。

2. **E12 本义（最省计划超预算）没做。** 它要 J-07 的符号成本签名，而且与账本重放相互作用——重放命中
   的调用不花钱，静态的直线段调用计数会变成假错。列为后续，不假称已有。

3. **E7 的范围。** `11-语言规范-v1.md` §4 写的是「`for … yield` 体内不得赋值（E7）」，并**明确允许**
   体内含 `do`。core 没有赋值语法，所以 E7 落成「`map` / `filter` 体内禁 `loop` / `stop`」。简报里
   「体内禁副作用」比依据严，core 没有按简报收紧（`examples/partial.jpp` 的 `advance` 正是在 `map`
   体内做 `judge` 与 `transform`）。这是首包当前的限制，尚未与历史规范统一。

4. **J-05 在源码语言下的构造。** `12` §J-05 的 v0.1.1 修订说「Python 里由返回注解构造」，并注明
   「Rust/OCaml 类宿主可静态得到同一纪律」。core 现在是两半：静态判三种确定情形（绑定后从未被提到、
   字面臂表缺 `unsure`、函数结果就是出口名而返回类型没提 `Exit`），其余仍由运行期的 `consumed` 标记
   在返回前核。更强的静态保证尚未实现。

5. **core 本地诊断码。** `E-name` / `E-arity` / `E-type` / `E-effect` / `W-shadow` / `W-bound` /
   `W-seq-const` / `W-effect` 都不在 11 的 E 表与 12 的 J 表里。它们带命名空间前缀，不占用共享编号。
   这些实现诊断尚未收进历史规范。效应标注一致性使用本地编号：J-07 是预算/成本签名，不是标注。

   **`E-effect` 有一个结构性的洞，首包不打算补。** 函数体里只要有一处被调者是参数里的方法值
   （`method(s)`、`strategy(item)`、`project(state)`），整个函数的效应集就变成未知，这条检查直接
   跳过。结果是真正带效应的高阶函数——`examples/adaptive.jpp` 的 `solve`、`partial.jpp` 的 `advance`
   ——一个都没被核，被核的只有 `observe` / `read` / `inspect` / `validate` 这些叶子。方法值一等是这门
   语言的立身之本，所以「未知」是常态不是例外。要做实得让函数类型带效应参数（效应变量），远超首包。
   在那之前，`!{…}` 标注对高阶函数只是文档，不是保证。

6. **`Client::gen` 改名 `generate`。** edition 2024 的保留字问题，J++ 内置名 `"gen"` 不变。

7. **`cut` 的 taint 继承。** `12` §2.11 说出口 taint 继承状态 taint，但 `Reading` 里现在没带状态的
   taint，首包一律按 `Trusted`。要把它做实，`Reading` 要加一个字段——属于 core 内部改动，等 J-08
   的守卫检查一起做。

8. **J-02 / J-04 / J-08 / J-09 / J-10 / J-15 / J-16 / J-17 没有静态面。** J-02 的一部分与 J-06、
   J-07、J-12、J-18 在运行期有；其余这几条首包没做，不假称已等价于 Python 检查器。
