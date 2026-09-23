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
pub enum Type {
    Named(String),
    Applied(String, Vec<Type>),
    Function(Vec<Type>, Box<Type>),   // 旧形式：效应行未知
    Method(MethodType),               // 方法类型：效应行与责任捕获跟着类型走
}

pub struct MethodType {
    pub params: Vec<Type>,
    pub ret: Box<Type>,
    pub effects: Option<Vec<String>>,    // 效应行；None = 未知/待推断
    pub captures_responsibility: bool,   // true = Fn¹，false = Fnω
}
```

**`Type::Method` 是给前端加的位置。** 方法一旦经参数、记录字段、返回值传递，`Function` 定义节点上的
`!{…}` 就跟不过去了，契约在边界上丢掉。`Type::Method` 把效应行放进**类型**本身，所以
`fn solve(input: Mat, method: Fn(Record) -> Record !{judge})` 里的 `method(s)` 不再是「静态判不了」。
`captures_responsibility` 区分 Codex 说的 `Fn¹`（捕获了未决责任，不可重复调用、不可丢弃）与 `Fnω`；
core 现在会拦住把 `Fn¹` 交给 `map` / `filter` 的写法。

前端现在的文法还写不出类型上的效应行（`type` 产生式里没有 `!{…}`），lower 出来的是旧的
`Type::Function`，core 按「效应未知」处理，与改动前行为一致。要把这一半用起来，前端需要把
`!{…}` 加进**类型**的文法，并在 lower 时产出 `Type::Method`。`Type::Function` 会一直保留。

没有效应专用节点。`judge`、`do`、`gen`、`ask`、`transform` 都是普通 `Call`，名字在根环境里解析成
`Value::Builtin`；检查器与解释器按名字认它们。前端不需要为效应造节点，只要把调用原样 lower 过来。

`budget` 缺失保持 `None`，由检查器报 J-07——前端不要造默认预算。`effects` 里的名字目前只认
`judge` / `gen` / `do` / `ask`；`transform` 是记账变换，不是效应形式，不写进标注。

全部节点 `#[derive(Serialize, Deserialize)]`，serde 默认表示，可往返。外部表示形状见下面第六节的样例。

## 二、值与环境

```rust
pub enum Value {
    Unit, Int(i64), Float(f64), Bool(bool), Text(Rc<str>),
    List(Rc<Vec<Value>>), Record(Rc<Vec<(String, Value)>>),
    Fn(Rc<Closure>), Builtin(&'static str),
    Mat(Rc<Mat>), State(Rc<State>), Question(Rc<Question>), Reading(Rc<Reading>), Exit(Rc<Exit>),
    Duty(Rc<Exit>),         // 未决责任 U(q)：unsure 臂收到的就是它
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
| `ask(state, question) -> Exit` | 问人。没答就是程序级 Pending；次数受 `budget.escalate` 管（J-07） |

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

**未决责任（`U(q)`）**

`handle` 的 `unsure` 臂收到的不是原因文本，是**未决责任本身**——一个 `Value::Duty`，与出口共享同一份
销账记录。它不可伪造（只能由 `handle` 交付），把它变成材料或 JSON 也不消除义务。**进臂不等于销账**：
臂体跑完，core 会核这份责任是不是真的交出去了，没交就是 J-05 错，报文指着那条臂。

| 签名 | 说明 |
| --- | --- |
| `unsure_cause(u) -> Text` | 读原因（`band` / `cold` / `tie` / `fail:…`）。读取不转移责任，只读不算处理 |
| `escalate(u, state, question) -> Exit` | 把责任交给明确关联的人工请求，效应 `ask`。未答即程序级 Pending |
| `literalize(u, state, question) -> Exit` | 接走旧责任，按更字面的题重问，效应 `judge`。换来的新出口仍要自己处理 |
| `unsure(u) -> Exit` | 重新包装成出口，继续由调用者负责（`unsure(原因: Text)` 仍是新造一个） |
| `consume(u, "drop")` | 显式丢弃并记账。`12` §6 允许这条路，core 留一条 `W-drop-vs-escalate` 提示 |

第四条去向是**把 `u` 放进臂的返回值**，责任随数据交给调用者。四条都不走，就是静默丢弃。

`otherwise` 兜不住 `Unsure`：三种题的出口都可能是未决，`unsure` 必须自己写一臂（通配只能替 `act` /
`ignore` / `pick` / `at`）。臂必须是收至少一个参数的方法——字面量收不下责任，这条静态就报。

**效应与失败**

| 签名 | 说明 |
| --- | --- |
| `do(动作名: Text, [参数…], iter_seq: Int) -> Mat \| Fail` | 只能触发 `ActionRegistry` 登记过的动作（J-11）。循环里 `iter_seq` 必须随轮次变（J-13） |
| `gen(prompt: Text, [ctx], n: Int, retry_seq: Int) -> [Mat]` | 同上，重试必须递增 `retry_seq` |
| `fail(reason: Text) -> Fail` / `is_fail(v) -> Bool` | Fail 是值，进账本，判断时走 `Unsure(fail)`（J-12） |

**有界控制与数据**

`loop(bound: Int, 初值, fn(acc, i))` 是唯一的循环，`bound` 必须在（J-06），体内 `stop(v)` 显式停止；
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
| `J-06` | `loop` 缺 bound，或 bound 是非正整数字面量 |
| `J-07` | 程序缺 `budget`；程序里有 `ask` / `escalate` 而 `budget.escalate` 是 0 |
| `E7` | `map` / `filter`（`for…yield`）的体内含 `loop` / `stop` |
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

诊断编号按 `12` 的 J 表走：预算相关一律 `J-07`，有界循环相关一律 `J-06`；`11` §诊断的 `E10` / `E12`
作为历史编号不再发出。`E7` 暂时还留在 `11` 的编号上（`12` 的 J 表里没有对应条目），见 §七。

**运行期的错**（`RtError`，也带 `Span` 与规则号）：J-02 禁自指、J-05 的 `consumed` 返回前核、
**J-05 的 unsure 臂销账核**（臂体没把责任交出去、臂收不下责任、`otherwise` 想兜 Unsure、责任被当材料）、
J-06 键重复即停与调用深度超限、J-11 动作未登记、类型不符、固定观察未命中。运行期的提示进
`Trace.warnings`：`W-header`、`W-bound`、`W-noprogress`、`W-drop-vs-escalate`、`returned_unsure`。

检查器的口径是**宁可漏报也不误报**：静态判不准的一律交给运行期，`Type` 里 core 认不得的名字不参与
`E-type`。

**效应推断有两条纪律，它们是 `E-effect` 能在高阶处判出东西的原因。**

*创建方法不等于执行方法。* 只有落在已知高阶位上的方法体才算会发生——`map` / `filter` 的第 2 位、
`fold` / `loop` 的第 3 位、`transform` 的第 1 位、`handle` 的臂，以及当场造当场调。被创建、被返回、
被存进记录的 lambda 一律不算进外层：`ε` 是调用时的潜在效应，创建的效应是 ∅。少了这一条，
`packet` 里那个 `fn(strategy) { … next(current, strategy) … }` 会把 `advance` 的效应算到每个调用者头上。

*解析不了的被调者只让推断变成下界。* 「标注少了 X」只要 X 确实看得见就报——这个方向上漏报是安全的。
只有反方向的「标了却看不到」（`W-effect`）需要完整信息，所以函数体里一旦有解析不了的被调者，
那条提示就不发。

在这两条之上，参数的效应行按**调用点实例化**：扫全程序的调用点，把实参的效应行灌给形参，取并集。
`solve(mat({target: 731}), step)` 于是把 `solve` 的 `method` 参数实例化成 `{judge}`，
`fn solve(…) !{}` 这种错标注在高阶处也拦得住。类型标注（`Type::Method` 的效应行）是作者给的上界，
压过调用点实例化。

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

## 七、待定项

以下几条 core 按当前团队约定落地了，但与依据文本或简报有出入，需要 Nature / 总控裁定。core 这边改
一个常量就能换口径，不会返工。

1. **诊断编号已按 `12` 统一（2026-09-21 总控裁定）。** 预算缺失与「`escalate` 出现而预算为 0」都发
   `J-07`，有界循环的 bound 发 `J-06`；`11-语言规范-v1.md` §诊断的 `E10` / `E12` 作为历史编号不再
   发出。这条已经定了，留在这里只是记明改动，CLI 里还写着 `E12` 的地方要跟着改。

2. **「静态可估的最省计划超预算」没做**（`11` 原来的 E12 本义）。它要 J-07 的符号成本签名，而且与
   账本重放相互作用——重放命中的调用不花钱，静态的直线段调用计数会变成假错。列为后续，不假称已有。

3. **E7 的范围。** `11-语言规范-v1.md` §4 写的是「`for … yield` 体内不得赋值（E7）」，并**明确允许**
   体内含 `do`。core 没有赋值语法，所以 E7 落成「`map` / `filter` 体内禁 `loop` / `stop`」。简报里
   「体内禁副作用」比依据严，core 没有按简报收紧（`examples/partial.jpp` 的 `advance` 正是在 `map`
   体内做 `judge` 与 `transform`）。要不要收紧，请裁定。

4. **J-05 在源码语言下的构造。** `12` §J-05 的 v0.1.1 修订说「Python 里由返回注解构造」，并注明
   「Rust/OCaml 类宿主可静态得到同一纪律」。core 现在是两半：静态判三种确定情形（绑定后从未被提到、
   字面臂表缺 `unsure`、函数结果就是出口名而返回类型没提 `Exit`），其余仍由运行期的 `consumed` 标记
   在返回前核。要不要把「静态得到同一纪律」写成更强的要求，请裁定。

5. **core 本地诊断码。** `E-name` / `E-arity` / `E-type` / `E-effect` / `W-shadow` / `W-bound` /
   `W-seq-const` / `W-effect` 都不在 11 的 E 表与 12 的 J 表里。它们带命名空间前缀，不占用共享编号。
   要不要收进依据文本，请裁定。效应标注一致性尤其需要一个正式编号：J-07 是预算/成本签名，不是标注。

   **效应扫描不是效应推断。** `check.rs` 走 AST 收集调用点，遇到解析不了的被调者退成 ⊤ 并跳过标注
   校验。创建或传递一个方法不等于执行它，真正执行由被调者的高阶签名决定。所以这份扫描结果只能当
   **提示**，不能当融合证明；融合器要的是另一件东西（判断站点、状态/题的输入依赖、结果需求点、分支、
   顺序、循环与屏障的结构摘要），core 目前没有产出它。

   **`E-effect` 在高阶处判得动了，而且是多态的。** 效应行是 `Row { concrete, vars, opaque }`：
   `concrete` 是确定会发生的效应（下界），`vars` 是**效应变量**——被调者是本函数的方法参数时留下的
   `(函数 id, 参数下标)`，`opaque` 是连名字都拿不到的被调者。用户**不写** ε；显式 `!{…}` 是要求检查
   的上界，缺省表示推断。rank-1 + 受限泛化，不追求任意阶完全推断（Codex 答 (b)：
   `map : ∀ A B ε. (Fnω(A ⊸ε B) ⊗ List<A>) ⊸ε List<B>`）。

   **两件事分开算，这是多态与单态并集的分水岭**：效应行**按调用点实例化**往上传（`apply(m, plain)`
   这个调用点不背 `apply(m, peek)` 那个调用点的 judge）；而核 `!{…}` 时取**所有调用点的并集**
   ——标注是上界，必须盖住这个函数的所有用法——诊断落在**定义处**的 Span。混作一谈就会出现
   「一处传纯方法、一处传带 judge 的方法，纯的那处被误报少标 judge」。

   四条纪律：创建方法 ≠ 执行方法（只有落在已知高阶位上的方法体才算会发生：`map`/`filter` 第 2 位、
   `fold`/`loop` 第 3 位、`transform` 第 1 位、`handle` 的臂、当场造当场调）；解析不了的被调者只让
   推断变成下界、不压住「标注少了 X」；效应变量按调用点实例化；标注核验取并集。

   **多态只在高阶函数自己不标注时生效。** 作者一旦写了 `fn apply(m, f) -> Record !{judge} { f(m) }`，
   那就是它对所有调用者声明的上界，`fn pure_user(m) !{} { apply(m, plain) }` 仍会被报「少了 judge」。
   类型上讲得通（声明的上界就是上界），实际后果是**效应多态的高阶助手应当不写标注、让它推断**。
   要既写标注又保住多态，得能在标注里写效应变量（`!{ε}`）——那要前端文法与依据文本一起定，
   不是 core 单方面能加的，列为待定。

   现在核得住而以前核不住的路径（都有测试钉着，`tests/effect_rows.rs` 里那四条在上一版基线上是红的）：
   同一个高阶函数两处用法互不污染（以前是**误报**）；方法经**返回值**传递（`maker()(m)`）；
   方法经**函数结果记录的字段**传递（`boxed().go(m)`）；**递归**把方法参数自己传回去
   （以前递归那次认不出的调用点会把整个形参的行毒成未知，把顶层那次真实参的实例化一起冲掉）。
   加上原本就核得住的 `solve` / `advance` / `probe` / `search` / `supplement` / `grade` / `validate`。

   **仍然退成未知的情形**（实测五类，全是漏报方向，不会误报）：具名方法经中间绑定传递
   （`let g = peek; apply(m, g)`）；`let` 绑定的记录再取字段调用（`let r = boxed(); r.go(m)`）；
   记录字面量先绑名字再取字段调用（`let r = {go: peek}; r.go(m)`）；方法存进列表再按下标取出调用
   （`xs[0](m)`）；形参上的记录字段（`box_.go(m)`）。共同的缺口是一样的：静态解析只跟
   **直接的函数结果**（`f()(x)`、`f().字段(x)`）与**名字**走，不把方法值沿 `let` 绑定与容器传播。
   代价是这些地方的 `W-effect`（标了却看不到）提示也不发。`examples/partial.jpp` 的
   `first.more(supplement)` 属于第二类；就算把它解析出来，`more` 那个 lambda 调的是它自己的参数
   `strategy`，行本身仍是 `opaque`，所以那条路上的 ε 补不齐**不只是**解析的问题。

   误报方向上修过一处：形参被块内同名的 `let` / `fn` 盖住时（`fn f(cb) !{} { let cb = fn() {…}; cb() }`
   配 `f(peek)`），调用点实例化出来的效应行会顺着名字被算到那个其实是纯的函数头上，发出一条挡程序的
   假 `E-effect`。现在 `row_of_block` 进块先摘掉本块重新绑定的名字，名字落回已知函数表；
   `tests/effect_rows.rs` 的「形参被同名局部绑定盖住时不算它的效应」钉着，同一条测试里配了没遮蔽时
   仍被拦下的对照。代价写明：遮蔽物是**具名方法的别名**时（`let f = peek`，不是函数字面量，
   `push_function` 没登记它）整条退成未知——并进上面第一类，仍是只漏报不误报。

   **前端还 lower 不出 `Type::Method`**（`!{…}` 不在类型文法里，`Fn(A) -> B` 一律 lower 成
   `Type::Function`，按「效应行未知」处理）。所以作者想在**类型位**上显式写效应行、让方法经任意路径
   传递时都带着契约，这条还差前端一步，已写进 `COORDINATION.md` 请 Codex 做。推断这一侧不依赖它。

6. **`Client::gen` 改名 `generate`。** edition 2024 的保留字问题，J++ 内置名 `"gen"` 不变。

7. **`cut` 的 taint 继承。** `12` §2.11 说出口 taint 继承状态 taint，但 `Reading` 里现在没带状态的
   taint，首包一律按 `Trusted`。要把它做实，`Reading` 要加一个字段——属于 core 内部改动，等 J-08
   的守卫检查一起做。

8. **J-02 / J-04 / J-08 / J-09 / J-10 / J-15 / J-16 / J-17 没有静态面。** J-02 的一部分与 J-06、
   J-07、J-12、J-18 在运行期有；其余这几条首包没做，不假称已等价于 Python 检查器。

9. **未决责任的记账只到臂边界。** `unsure` 臂把责任包进返回值就算交出去了，core 记
   `consumed_by = "handle:unsure(包装返回)"` 收工。**之后责任再被丢掉，core 不再追**：
   `filter` 删元素、`slice` 切掉前后段、`record.field` 取一部分丢其余、`stop` / Fail 短路 / 预算退出
   都能把它扔了（Codex 陷阱 4）。要堵住这些，含责任的容器需要消费式拆分或保留余项，普通丢元素的
   API 要求 `Drop(A)`——那是完整线性系统的活，v1 不做。

10. **`collect_exit_ids` 不遍历函数捕获环境**（`interp.rs`）。所以「把未决责任装进续接方法返回给
    调用者」这条**合法**路径现在表达不出来：责任只在列表、记录、`stop` 里能被看见。要做实，检查器
    还得按实际自由变量确定捕获责任，不能把共享环境链里所有可达名字都当成捕获。

11. **函数哈希只来自 AST，而 `transform` 的账本键用它**（`interp.rs`）。相同函数体捕获不同环境，
    结果可能不同；动态方法工厂会直接撞上。结构摘要可以按代码身份复用，执行缓存必须再含相关环境依赖。

12. **`Pending` 没有承接尚存的责任。** 预算耗尽、`ask` 未答、显式 `pending` 都是绕过函数返回检查
    直接挂起，挂起状态里没有「还欠哪些未决」。可重放的挂起要把它们一起带上。

13. **`E7` 还挂在 `11` 的编号上。** 其余诊断已按 `12` 的 J 表走（预算 → `J-07`，有界循环 → `J-06`），
    但「`for…yield` 体内含 `loop` / `stop`」在 `12` 的 J 表里没有对应条目，硬塞一个 J 号比留着
    `E7` 更糟。请裁定给它一个 J 号还是保留 `E7`。

14. **`escalate` / `literalize` / `unsure_cause` 是 core 为这套责任协议新加的内置**，`11` 的标准库表
    和 `12` 的 §6 速查里都没有。名字与参数顺序按 Codex 给的概念签名定
    （`escalate : U(q) ⊗ HumanRequest(q) ⊸{ask} Exit(q)`、`literalize : U(q) ⊗ LiteralPlan(q,q') ⊸{judge} Exit(q')`）。
    要不要收进依据文本、要不要改名，请裁定。另外「问题确实更字面」core 只保证走了 `literalize` 这条
    受检路径，**不**保证新题真的更字面——那要构造约束加校准依据，不是包一层 `Text` 就算证明。
