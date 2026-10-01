//! 解释器的自由函数：题、题式、契约值的字段读取，声明项解析，来源与谱系，JSON 与效应值互转，出口遍历。
//! 步 41（C2a）从 `lib.rs` 机械拆出，只搬代码、不改逻辑（`lib.rs` 超 A8 的 1,500 行）。

use super::*;

/// 函数体里引用到的名字（含嵌套 lambda 与参数名）。**宁可多收**——多收只是少复用一点缓存，
/// 少收会把别人的结果当成自己的。语言形式与效应节点的名字也收（与步 12c 前按源码树收集同口径）。
pub(crate) fn referenced_names(f: &Function) -> BTreeSet<String> {
    fn go_block(b: &Block, out: &mut BTreeSet<String>) {
        for s in &b.statements {
            match s {
                Stmt::Let { value, .. } => go(value, out),
                Stmt::Function { function, .. } => go_block(&function.body, out),
                Stmt::Expr(e) => go(e, out),
            }
        }
        if let Some(r) = &b.result {
            go(r, out);
        }
    }
    fn go(e: &Expr, out: &mut BTreeSet<String>) {
        match kind(e) {
            K::Name(n) => {
                out.insert(n.to_string());
            }
            K::List(items) => items.iter().for_each(|x| go(x, out)),
            K::Record(fields) => fields.iter().for_each(|(_, x)| go(x, out)),
            K::Function(inner) => go_block(&inner.body, out),
            K::Call { callee, args } => {
                match callee {
                    Callee::Name(n) => {
                        out.insert(n.to_string());
                    }
                    Callee::Expr(c) => go(c, out),
                }
                args.iter().for_each(|x| go(x, out));
            }
            K::Field { value, .. } => go(value, out),
            K::Index { value, index } => {
                go(value, out);
                go(index, out);
            }
            K::Unary { value, .. } => go(value, out),
            K::Binary { left, right, .. } => {
                go(left, out);
                go(right, out);
            }
            K::If { condition, yes, no } => {
                go(condition, out);
                go_block(yes, out);
                go_block(no, out);
            }
            K::Block(b) => go_block(b, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    go_block(&f.body, &mut out);
    out
}

/// 同题跨运行合并：noul / score 取均值，choice 取众数（`12`:134）
pub(crate) fn merge_runs(
    ans: &dyn Answers,
    rs: &[Rc<Reading>],
    method: &str,
    sp: Span,
) -> R<Answer> {
    let answers: Vec<Answer> = rs.iter().filter_map(|r| ans.answer_of(r)).collect();
    if answers.is_empty() {
        return err(
            Some("J-12"),
            "repeat 收到的读数全是失败或未答，没有可合并的",
            sp,
        );
    }
    // 逐分量取均值或中位数（B28）。choice / score 都是概率向量，逐分量合并，不投票。
    let 合 = |xs: &mut Vec<f64>| -> f64 {
        if xs.is_empty() {
            return 0.0;
        }
        if method == "median" {
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let m = xs.len() / 2;
            if xs.len() % 2 == 1 {
                xs[m]
            } else {
                (xs[m - 1] + xs[m]) / 2.0
            }
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let 向量 = |pick: &dyn Fn(&Answer) -> Option<Vec<f64>>, len: usize| -> Vec<f64> {
        (0..len)
            .map(|i| {
                let mut xs: Vec<f64> = answers
                    .iter()
                    .filter_map(|a| pick(a).and_then(|v| v.get(i).copied()))
                    .collect();
                合(&mut xs)
            })
            .collect()
    };
    Ok(match &answers[0] {
        Answer::Noul(_) => {
            let mut ps: Vec<f64> = answers
                .iter()
                .filter_map(|a| {
                    if let Answer::Noul(p) = a {
                        Some(*p)
                    } else {
                        None
                    }
                })
                .collect();
            Answer::Noul(合(&mut ps))
        }
        Answer::Score(v0) => Answer::Score(向量(
            &|a| {
                if let Answer::Score(v) = a {
                    Some(v.clone())
                } else {
                    None
                }
            },
            v0.len(),
        )),
        Answer::Choice(v0) => Answer::Choice(向量(
            &|a| {
                if let Answer::Choice(v) = a {
                    Some(v.clone())
                } else {
                    None
                }
            },
            v0.len(),
        )),
    })
}

/// 这道题声明了、而状态里空着的证据槽（J-09）。与 Python `runtime.py:1133` 同口径：
/// 只查「槽不存在或为空」，不查内容。
pub(crate) fn missing_evidence(state: &State, q: &Question) -> Vec<String> {
    q.evidence
        .iter()
        .filter(|slot| {
            let v = match slot.as_str() {
                "on" => &state.on,
                "ctx" => &state.ctx,
                "ref" => &state.r#ref,
                // B155（步 15i）：`over` 只随 `select` 题作 `criteria` 发出，是非题与打分题线上看不到它——
                // 声明 `over` 为决定性证据的非 `select` 题按缺证据处理（不信任 p），不因状态里有 `over` 放行。
                // 依据：B155、J-09
                "over" if q.op != Op::Select => return true,
                "over" => &state.over,
                _ => return false,
            };
            v.is_empty()
        })
        .cloned()
        .collect()
}

/// `test`/`select`/`measure` 的可选第三参：`{evidence: [槽名…]}`（J-09）
pub(crate) fn evidence_of(v: Option<&Value>, sp: Span) -> R<Vec<String>> {
    let Some(v) = v else { return Ok(vec![]) };
    let Value::Record(fields) = v else {
        return err(
            Some("E-rt-question"),
            "题的第三个参数要是记录：{evidence: [\"ctx\", …]}",
            sp,
        );
    };
    let Some((_, slots)) = fields.iter().find(|(k, _)| k == "evidence") else {
        return Ok(vec![]);
    };
    let Value::List(l) = slots else {
        return err(Some("E-rt-question"), "evidence 要是槽名的列表", sp);
    };
    let mut out = vec![];
    for s in l.iter() {
        let Value::Text(t, _) = s else {
            return err(Some("E-rt-question"), "evidence 里要是槽名（文本）", sp);
        };
        if !matches!(t.as_ref(), "on" | "ctx" | "ref" | "over") {
            return err(
                Some("E-rt-question"),
                format!("evidence 里的 {t} 不是槽名；状态只有 on / ctx / ref / over 四个槽"),
                sp,
            );
        }
        out.push(t.to_string());
    }
    Ok(out)
}

/// 题上可读的字段（只读）。静态检查（check.rs）用同一张表核字段名。
pub const QUESTION_FIELDS: &[&str] = &[
    "text",
    "op",
    "calib",
    "scale",
    "evidence",
    "hash",
    "subject",
    "predicate",
    "partition",
    "request",
    "presupposition",
    "form",
    "template",
    "fill",
    "kind",
    // 步 23b：超窗裂变的声明（未声明为 unit）
    "fission",
    // 裁定五十一：拿不准时可能缺的信息类别（取自题式，没有为空列表）
    "lacks",
];
/// 题式上可读的字段（只读）。
pub const FORM_FIELDS: &[&str] = &[
    "template",
    "op",
    "slots",
    "calib",
    "scale",
    "evidence",
    "presupposition",
    "request",
    "partition",
    "subject",
    "hash",
    "on",
    // 步 23b：超窗裂变的声明（未声明为 unit）
    "fission",
    // 裁定五十一：拿不准时可能缺的信息类别（没声明为空列表）
    "lacks",
];

/// 组合封闭性契约（B17，施工件 i）的字段。每个构造（`sieve` / `pair` / `tally` / `first_k` /
/// `iterate` / `outcome`）返回同一形状的记录，检查器据此核字段名。
/// - `kind`：产生它的构造；
/// - `value`：产出；
/// - `pending`：未决清单，每项 `{element, exit, cause}`，`exit` 承担责任（J-05 / 13 §3）；
/// - `evidence`：账本键（Text），不存读数或材料的副本；
/// - `resume`：续接——停在哪里、为什么、可选的继续方法 `next`；
/// - `spent`：本构造新增的调用与费用 `{calls, usd}`；
/// - `detail`：构造特有的已决信息（例如 sieve 的 `question`、`ignore`）；
/// - `purpose`：可选的可读目的，供诊断。
pub const OUTCOME_FIELDS: &[&str] = &[
    "kind", "value", "pending", "evidence", "resume", "spent", "detail", "purpose",
];

/// 这个值是不是一个契约值（字段集合与 `OUTCOME_FIELDS` 一致）
pub fn is_outcome(v: &Value) -> bool {
    match v {
        Value::Record(r) => {
            r.len() == OUTCOME_FIELDS.len()
                && OUTCOME_FIELDS.iter().all(|f| r.iter().any(|(k, _)| k == f))
        }
        _ => false,
    }
}

/// 证据列表去重追加（证据都是账本键 Text）
pub(crate) fn push_key(v: &mut Vec<Value>, k: Value) {
    let same = |a: &Value| matches!((a, &k), (Value::Text(x, _), Value::Text(y, _)) if x == y);
    if !v.iter().any(same) {
        v.push(k);
    }
}

pub(crate) fn list_of(v: Option<Value>) -> Vec<Value> {
    match v {
        Some(Value::List(l)) => l.iter().cloned().collect(),
        _ => vec![],
    }
}

pub(crate) fn texts(v: &[String]) -> Value {
    Value::list(v.iter().map(|x| Value::text(x)).collect())
}
pub(crate) fn opt_text(v: &Option<String>) -> Value {
    v.as_deref().map(Value::text).unwrap_or(Value::Unit)
}

/// B1 五件与题的元数据。`predicate` 就是题面：主体（被判断的对象）在状态里，不在题面里，
/// 题面说的是对它判断什么。由题式填出的题另有 `template`（带槽的谓词）与 `fill`（填法）。
pub(crate) fn question_field(q: &Question, field: &str) -> Option<Value> {
    Some(match field {
        "text" | "predicate" => Value::text(&q.text),
        "op" => Value::text(q.op.fixture_name()),
        "calib" => Value::text(&q.calib),
        "scale" => texts(&q.scale),
        "evidence" => texts(&q.evidence),
        "hash" => Value::text(&q.hash),
        "subject" => Value::text(q.subject()),
        "partition" => Value::text(q.partition()),
        "request" => Value::text(&q.request()),
        "presupposition" => opt_text(&q.presupposition),
        "form" => opt_text(&q.form_hash),
        "template" => opt_text(&q.template),
        // 基础题类（B76，只读、不进任何哈希）：名字与报告 questions 表的 kind 相同（小写英文）。
        // 派生库按题类拆题要读它（步 28，K2）
        "kind" => Value::text(
            serde_json::to_value(q.kind())
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default()
                .as_str(),
        ),
        "fill" => match &q.fill {
            Some(f) => Value::Record(Rc::new(
                f.iter().map(|(k, v)| (k.clone(), Value::text(v))).collect(),
            )),
            None => Value::Unit,
        },
        "fission" => fission_field(&q.fission),
        "lacks" => texts(&q.lacks),
        _ => return None,
    })
}

pub(crate) fn form_field(f: &jpp_value::value::Form, field: &str) -> Option<Value> {
    Some(match field {
        "template" => Value::text(&f.template),
        "op" => Value::text(f.op.fixture_name()),
        "slots" => texts(&f.slots),
        "calib" => Value::text(&f.calib),
        "scale" => texts(&f.scale),
        "evidence" => texts(&f.evidence),
        "presupposition" => opt_text(&f.presupposition),
        "request" => Value::text(
            f.request
                .as_deref()
                .unwrap_or(jpp_value::value::default_request(f.op)),
        ),
        "partition" => Value::text(match f.op {
            Op::Test => "binary",
            Op::Select => "k_ary",
            Op::Measure => "ordered",
        }),
        "subject" => Value::text(match f.op {
            Op::Select => "over",
            _ => "on",
        }),
        "hash" => Value::text(&f.hash),
        // 裁定十九、B192（步 28）：答案块上的签名，没声明是 unit
        "on" => f.on.as_ref().map_or(Value::Unit, |s| (*s.0).clone()),
        "fission" => fission_field(&f.fission),
        "lacks" => texts(&f.lacks),
        _ => return None,
    })
}

/// 可读字段 `fission`（步 23b）：未声明为 unit，声明了为 `{mode: "approx", merge: "exists" | "all"}`
pub(crate) fn fission_field(d: &Option<jpp_value::value::FissionDecl>) -> Value {
    match d {
        None => Value::Unit,
        Some(d) => Value::record(vec![
            ("mode".into(), Value::text("approx")),
            (
                "merge".into(),
                Value::text(if d.all { "all" } else { "exists" }),
            ),
        ]),
    }
}

/// 超窗裂变的声明 `{fission: "approx"}` 与是非题的全称合回 `{merge: "all"}`（步 23b）：`fission` 只收 `"approx"`
/// （V8 一致率 0.897 落在近似档，`21` 步 23b 注）；`merge` 只收 `"exists"`（缺省）或 `"all"`、只用于 test、要与 `fission`
/// 同写。依据：`11` §5.3「test → exists（或按声明 all）」；主控 Z0208 Q-F4、Q-F5
pub(crate) fn fission_of(
    v: Option<&Value>,
    op: Op,
    sp: Span,
) -> R<Option<jpp_value::value::FissionDecl>> {
    let f = v.and_then(|v| v.get("fission"));
    let m = v.and_then(|v| v.get("merge"));
    let declared = match f {
        None | Some(Value::Unit) => false,
        Some(Value::Text(t, _)) if t.as_ref() == "approx" => true,
        Some(other) => {
            let got = match &other {
                Value::Text(t, _) => format!("「{t}」"),
                o => o.type_name().to_string(),
            };
            return err(
                Some("E-rt-question"),
                format!(
                    "fission 只收 \"approx\"（超窗裂变是近似策略：V8 实测整篇与合回出口一致率 0.897），收到 {got}"
                ),
                sp,
            );
        }
    };
    let all = match m {
        None | Some(Value::Unit) => false,
        Some(Value::Text(t, _)) if t.as_ref() == "exists" || t.as_ref() == "all" => {
            if !declared {
                return err(
                    Some("E-rt-question"),
                    "merge 是超窗裂变的合回方式，要与 {fission: \"approx\"} 同写",
                    sp,
                );
            }
            if op != Op::Test {
                return err(
                    Some("E-rt-question"),
                    format!(
                        "merge 只用于 test：{} 题的合回方式由题型定（select 分块选后再一层 noul-argmax，measure 按出口计数；11 §5.3）",
                        op.fixture_name()
                    ),
                    sp,
                );
            }
            t.as_ref() == "all"
        }
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!(
                    "merge 只收 \"exists\" 或 \"all\"，收到 {}",
                    other.type_name()
                ),
                sp,
            );
        }
    };
    Ok(declared.then_some(jpp_value::value::FissionDecl { all }))
}

/// 题式的 `{lacks: [类别…]}`（裁定五十一）：文本列表，去重。题值只从题式取，`test`/`select` 直接写报错
pub(crate) fn lacks_of(v: Option<&Value>, 题式: bool, sp: Span) -> R<Vec<String>> {
    let 用法 = "lacks 要是信息类别的文本列表，如 {lacks: [\"材料\", \"语境\"]}（裁定五十一）";
    match v.and_then(|v| v.get("lacks")) {
        None | Some(Value::Unit) => Ok(vec![]),
        Some(_) if !题式 => err(
            Some("E-rt-question"),
            "lacks 写在题式上：form(…, {lacks: [类别…]})，题值只从题式取（裁定五十一）",
            sp,
        ),
        Some(Value::List(l)) => {
            let mut out: Vec<String> = vec![];
            for x in l.iter() {
                let Value::Text(t, _) = x else {
                    return err(Some("E-rt-question"), 用法, sp);
                };
                if !out.iter().any(|y| y == t.as_ref()) {
                    out.push(t.to_string());
                }
            }
            Ok(out)
        }
        Some(_) => err(Some("E-rt-question"), 用法, sp),
    }
}

/// 题与题式的置换声明 `{permute: true}`（B64，步 15f）：只对 K 选一（`select`）有意义，值须为布尔。
/// 依据：B64（地基/附注/2026-09-24-探针首轮裁定.md，I-1(b)）
pub(crate) fn permute_of(v: Option<&Value>, op: Op, sp: Span) -> R<bool> {
    let declared = match v.and_then(|v| v.get("permute")) {
        None | Some(Value::Unit) => return Ok(false),
        Some(Value::Bool(b, _, _)) => b,
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!(
                    "permute 要是布尔（{{permute: true}}），收到 {}（依据：B64）",
                    other.type_name()
                ),
                sp,
            );
        }
    };
    if declared && op != Op::Select {
        return err(
            Some("E-rt-question"),
            format!(
                "permute 只用于 select（K 选一）：{} 题没有候选顺序可换（依据：B64）",
                op.fixture_name()
            ),
            sp,
        );
    }
    Ok(declared)
}

/// 是非题的答案标签 `{labels: {yes: Text, no: Text}}`（B155，步 15i）：线上作 `criteria: {true, false}`，
/// 进题哈希与题式哈希。只用于 `test`（`select` 的候选标签写在候选材料上：`{label, text}`）。
/// 依据：B155（地基/附注/2026-09-26-批6裁定.md §三）
pub(crate) fn labels_of(
    v: Option<&Value>,
    op: Op,
    sp: Span,
) -> R<Option<jpp_value::value::TestLabels>> {
    let l = match v.and_then(|v| v.get("labels")) {
        None | Some(Value::Unit) => return Ok(None),
        Some(l) => l,
    };
    if op != Op::Test {
        return err(
            Some("E-rt-question"),
            format!(
                "labels 只用于 test（是非题的答案标签）：{} 题的候选标签写在候选材料上 {{label, text}}（依据：B155）",
                op.fixture_name()
            ),
            sp,
        );
    }
    let (yes, no) = (l.get("yes"), l.get("no"));
    match (&l, yes, no) {
        (Value::Record(fs), Some(Value::Text(y, _)), Some(Value::Text(n, _))) if fs.len() == 2 => {
            Ok(Some(jpp_value::value::TestLabels {
                yes: y.to_string(),
                no: n.to_string(),
            }))
        }
        _ => err(
            Some("E-rt-question"),
            format!(
                "labels 要是 {{yes: Text, no: Text}}，收到 {}（依据：B155）",
                l.type_name()
            ),
            sp,
        ),
    }
}

/// 题的声明项：前提（可选文本）与请求（本版只接受各题型的缺省请求，见下）。
pub(crate) fn question_decl_of(
    v: Option<&Value>,
    op: Op,
    sp: Span,
) -> R<(Option<String>, Option<String>)> {
    let Some(v) = v else { return Ok((None, None)) };
    let presupposition = match v.get("presupposition") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t, _)) => Some(t.to_string()),
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!("presupposition 要是文本，收到 {}", other.type_name()),
                sp,
            );
        }
    };
    let request = match v.get("request") {
        None | Some(Value::Unit) => None,
        Some(Value::Text(t, _)) => {
            // 本版 `cut` 只实现每个题型的缺省请求。「K 选一、选出全部」（all）要由三路过滤
            // 与子集判断承担（施工件 c），在那之前声明它只会被静默当成 one——所以拒绝，而不是收下不管。
            if t.as_ref() != jpp_value::value::default_request(op) {
                let hint = if op == Op::Select && t.as_ref() == "all" {
                    "；「选出全部」待三路过滤（施工件 c）实现后可用，现在用 map + test 逐个判"
                } else {
                    ""
                };
                return err(
                    Some("E-rt-question"),
                    format!(
                        "request 「{t}」不适用于 {} 题：本版只支持缺省请求 {}{hint}",
                        op.fixture_name(),
                        jpp_value::value::default_request(op)
                    ),
                    sp,
                );
            }
            Some(t.to_string())
        }
        Some(other) => {
            return err(
                Some("E-rt-question"),
                format!("request 要是文本，收到 {}", other.type_name()),
                sp,
            );
        }
    };
    Ok((presupposition, request))
}

/// 一组实参里各材料的来源出口键（`Mat.from_key`）的并（B59，步 17a；容器递归看，与 `derived_of` 同）。
/// 步 17c（B84）起取值级标签的 sources：标量叶子、材料、出口、题都算。
/// 步 18c（B92）起带边的种类：效应输出承接输入边，种类不变。
pub(crate) fn from_keys_of(args: &[Value]) -> Sources {
    args.iter()
        .fold(Provenance::trusted(), |p, a| prov_join(&p, &a.prov()))
        .sources
}

/// 出口交给 `pick`/`at` 臂的标签（B84）：出口 taint，sources = {出口键}，值依赖边（B92：k 由读数算出）。
pub(crate) fn 出口标签(e: &Exit) -> Provenance {
    Provenance::new(e.taint, Sources::value(&e.ledger_key.borrow(), &e.q_hash))
}

/// 元素记录的直接来源（B59，步 17a，结构通道）：`sieve` 元素有 `exit` 且账本键非空 → 该键；
/// `pair` 元素（无 `exit`、有 `left`/`right`）→ 两侧来源的并；其余为空。
/// 只取直接来源，更早的祖先经它们自己的 `hop` 计入。经普通值（`e.item` 取出）的依赖不在此列（候选 B84）。
pub(crate) fn element_lineage(it: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if !matches!(it, Value::Record(_)) {
        return out;
    }
    match it.get("exit") {
        Some(Value::Exit(e)) | Some(Value::Duty(e)) => {
            let k = e.ledger_key.borrow().clone();
            if !k.is_empty() {
                out.insert(k);
            }
        }
        _ => {
            for side in ["left", "right"] {
                if let Some(v) = it.get(side) {
                    out.extend(element_lineage(&v));
                }
            }
        }
    }
    out
}

/// 一组实参里各材料的 `derived_from` 的并（容器要递归看，与 `taint_of` 同）
pub(crate) fn derived_of(args: &[Value]) -> BTreeSet<String> {
    fn go(v: &Value, out: &mut BTreeSet<String>) {
        match v {
            Value::Mat(m) => out.extend(m.derived_from.iter().cloned()),
            Value::List(l) => l.iter().for_each(|x| go(x, out)),
            Value::Record(fs) => fs.iter().for_each(|(_, x)| go(x, out)),
            Value::Stop(x) => go(x, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    args.iter().for_each(|a| go(a, &mut out));
    out
}

/// 不在分派处做「输出 ∨ 输入」的内置（B33 第 3 点）。两类：
/// 1. **效应边界与自带规则**：taint 按 `12` §2.11 表在这里赋值（`state`/`mat`/`content`/`judge`/
///    `cut`/`do`/`gen`/`ask`/`transform`…），或输出本身就带着该有的位（出口、材料、契约值）；
/// 2. **只搬运元素**：输出的元素就是输入的元素（或用户函数的返回值），各带自身的位；
///    整体 ∨ 会把一个不可信元素的位抹到所有元素上（取字段 / 下标返回叶子自身的位，同一原则）。
/// 不在表上的内置（含将来新增的）一律按 ∨ 输入处理——**兜底往拒绝那边倒**。
pub(crate) const 不做数据流合取的内置: &[&str] = &[
    // 效应边界与自带规则
    "state",
    "test",
    "select",
    "measure",
    "form",
    "fill",
    "judge",
    "cut",
    "handle",
    "consume",
    "do",
    "gen",
    "ask",
    "transform",
    "mat",
    "content",
    "sieve",
    "pair",
    "tally",
    "first_k",
    "iterate",
    "outcome",
    "repeat",
    "agg",
    "allocate",
    "unsure_bound",
    "fit",
    "order",
    // 步 25-8a：元素构造自带来源规则（item 只加选择边，B92）；整体 ∨ 会给 item 加出口的值依赖边，
    // 把「读数只选中了它」变成「内容派生自这道题」（J-02 会误拦同题再问）
    "element",
    "escalate",
    "literalize",
    "unsure",
    "pending",
    "print",
    "stop",
    "fail",
    // 只搬运元素
    "map",
    "filter",
    "fold",
    "loop",
    "append",
    "concat",
    "slice",
    "reverse",
    "with",
];

/// 一个值携带的 taint（B33：标量自带位，容器递归 ∨）
pub(crate) fn taint_of(v: &Value) -> Taint {
    v.taint()
}

/// 13 §6 的运行错误：说清是哪一步越界、越的是哪个界，并带 `.jpp` 的 Span
pub(crate) fn overflow(what: &str, a: i64, b: i64, sp: Span) -> Fault {
    Fault::Error(RtError::new(
        Some("E-rt-int"),
        format!(
            "Int {what}溢出：{a} 与 {b} 的结果超出有符号 64 位范围（{} … {}）。Int 是 64 位有符号整数，溢出是错误不是回绕",
            i64::MIN,
            i64::MAX
        ),
        sp,
    ))
}

pub fn json_to_value(j: &Json) -> Value {
    match j {
        Json::Null => Value::Unit,
        Json::Bool(b) => Value::Bool(*b, Taint::Trusted.into(), GuardEv::EMPTY),
        Json::Number(n) => n
            .as_i64()
            .map(Value::int)
            .unwrap_or_else(|| Value::Float(n.as_f64().unwrap_or(0.0), Taint::Trusted.into())),
        Json::String(s) => Value::text(s),
        Json::Array(a) => Value::list(a.iter().map(json_to_value).collect()),
        Json::Object(o) => Value::record(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect(),
        ),
    }
}

/// 效应输出写进账本（账本 v3，步 18a）：`(output, output_mat)`。材料输出的内容进 `output`，
/// 地址、来源链、taint 与来源边（带种类，B92）进 `output_mat`；`derived_from` 不写（B84：值依赖边的投影，
/// 读回时重算）。失败值写 `{"__fail", "taint"}`，其余写值本身、`output_mat` 为空。
pub fn effect_value_to_entry(v: &Value) -> (Json, Option<MatMeta>) {
    match v {
        Value::Fail(s, t) => (json!({"__fail": s.as_ref(), "taint": t.taint}), None),
        Value::Mat(m) => {
            let sources = m
                .prov()
                .sources
                .edges()
                .map(|(k, e)| SourceEdge {
                    key: k.clone(),
                    kind: e.kind,
                    q: e.q.clone(),
                })
                .collect();
            (
                m.content.clone(),
                Some(MatMeta {
                    addr: m.addr.clone(),
                    origin: m.origin.clone(),
                    taint: m.taint,
                    sources,
                }),
            )
        }
        other => (other.to_json(), None),
    }
}

/// 从账本读回效应输出（账本 v3）。`output_mat` 在即为材料：来源边按种类还原，`derived_from` 由值依赖边重算。
pub fn entry_to_effect_value(output: &Json, output_mat: Option<&MatMeta>) -> Value {
    if let Some(meta) = output_mat {
        let edges = meta
            .sources
            .iter()
            .map(|e| {
                (
                    e.key.clone(),
                    jpp_value::prov::Edge {
                        kind: e.kind,
                        q: e.q.clone(),
                    },
                )
            })
            .collect();
        return Value::Mat(Rc::new(
            Mat::new(
                output.clone(),
                &meta.addr,
                meta.origin.clone(),
                meta.taint,
                BTreeSet::new(),
            )
            .with_sources(&Sources::from_map(edges)),
        ));
    }
    if let Some(f) = output.get("__fail").and_then(|x| x.as_str()) {
        // 失败值的 taint 缺了或坏了：兜底往拒绝那边倒（untrusted），与材料的反序列化同一纪律
        let t = output
            .get("taint")
            .and_then(|x| serde_json::from_value::<Taint>(x.clone()).ok())
            .unwrap_or(Taint::Untrusted);
        return Value::Fail(Rc::from(f), t.into());
    }
    json_to_value(output)
}

/// 返回值里带着哪些出口 / 未决责任（可达性核，`20` v2 §3.5）。
pub(crate) fn collect_exit_ids(v: &Value, out: &mut HashSet<usize>) {
    visit_exits(v, &mut |e| {
        out.insert(e.id);
    });
}

/// 按可达性走一个值里的出口与未决责任，逐个交给 `f`（同一出口可能经多条路径被交多次）。
pub(crate) fn visit_exits(v: &Value, f: &mut dyn FnMut(&Rc<Exit>)) {
    match v {
        Value::Exit(e) | Value::Duty(e) => f(e),
        // 惰性出口（B94）：解析了按出口算；未解析的只会在它自己那一帧还活着时出现，帧返回前必解析
        Value::Cut(c) => {
            if let Some(e) = c.exit() {
                f(&e)
            }
        }
        Value::List(l) => l.iter().for_each(|x| visit_exits(x, f)),
        Value::Record(r) => r.iter().for_each(|(_, x)| visit_exits(x, f)),
        Value::Stop(x) => visit_exits(x, f),
        // 方法的**捕获环境**里也可能装着责任。`13` §3 明列「随返回值/继续方法交给调用者」
        // 是合法去向，而类型侧早就用 `captures_responsibility` 认了方法能捕获责任——
        // 扫描侧不进环境，就成了内核两半打架：合法的续接方法被判成「责任丢了」。
        Value::Fn(c) => visit_closure(c, 3, f),
        _ => {}
    }
}

/// 经闭包捕获环境可达的责任（B52：可达性延伸进闭包捕获）。
/// 已用掉的 Fn¹（判为唯一路径后调用过一次）不再是它那几条责任的路径：责任在那次调用里已按
/// 实际去向处置，闭包不能再调用，经它「可达」只是字面上的（依据：B52、`13` §3）。
pub(crate) fn visit_closure(c: &Closure, depth: u32, f: &mut dyn FnMut(&Rc<Exit>)) {
    let spent: Vec<usize> = if c.linear_called.get() {
        c.linear.borrow().clone()
    } else {
        vec![]
    };
    let mut g = |e: &Rc<Exit>| {
        if !spent.contains(&e.id) {
            f(e)
        }
    };
    visit_env(&c.env, &referenced_names(&c.function), depth, &mut g);
}

/// 从捕获环境里找责任。只看方法体**实际引用到**的名字，不把共享环境链里所有可达名字都算成捕获
/// （Codex 陷阱 5 的后半句）；`depth` 防递归环境链无限展开。
pub(crate) fn visit_env(
    env: &Env,
    names: &BTreeSet<String>,
    depth: u32,
    f: &mut dyn FnMut(&Rc<Exit>),
) {
    if depth == 0 {
        return;
    }
    for n in names {
        let Some(v) = env_lookup(env, n) else {
            continue;
        };
        match &v {
            Value::Fn(c) => {
                if depth > 1 {
                    visit_closure(c, depth - 1, f)
                }
            }
            other => visit_exits(other, f),
        }
    }
}

/// 闭包创建时经捕获环境可达的未销账未决责任（`Closure.captures`，B52，步 21）
pub(crate) fn captured_duties(f: &Function, env: &Env) -> Vec<usize> {
    let mut out = vec![];
    visit_env(env, &referenced_names(f), 3, &mut |e| {
        if e.is_unsure() && !e.consumed.get() && !out.contains(&e.id) {
            out.push(e.id);
        }
    });
    // 未解析的惰性出口（B94）：种类还不知道，按可能是未决记下它预分配的出口号（多记只让 Fn¹ 判定
    // 多一个候选，是否真是未决在帧返回时按出口核）
    for n in referenced_names(f) {
        if let Some(v) = env_lookup(env, &n) {
            未解析出口号(&v, &mut out);
        }
    }
    out
}

pub(crate) fn 未解析出口号(v: &Value, out: &mut Vec<usize>) {
    match v {
        Value::Cut(c) if c.exit().is_none() => {
            if !out.contains(&c.id) {
                out.push(c.id)
            }
        }
        Value::List(l) => l.iter().for_each(|x| 未解析出口号(x, out)),
        Value::Record(r) => r.iter().for_each(|(_, x)| 未解析出口号(x, out)),
        Value::Stop(x) => 未解析出口号(x, out),
        _ => {}
    }
}

/// 返回值里每条责任的可达路径（B52 的 Fn¹ 判定用）：`direct` 是不经任何闭包可达的出口 id；
/// `via` 是经闭包可达的出口 id → 途经的**值层**闭包（按身份去重；嵌在闭包环境里的闭包算作外层那个）。
pub(crate) fn exit_paths(
    v: &Value,
    direct: &mut HashSet<usize>,
    via: &mut HashMap<usize, Vec<Rc<Closure>>>,
) {
    match v {
        Value::Exit(e) | Value::Duty(e) => {
            direct.insert(e.id);
        }
        Value::Cut(c) => {
            direct.insert(c.id);
        }
        Value::List(l) => l.iter().for_each(|x| exit_paths(x, direct, via)),
        Value::Record(r) => r.iter().for_each(|(_, x)| exit_paths(x, direct, via)),
        Value::Stop(x) => exit_paths(x, direct, via),
        Value::Fn(c) => visit_closure(c, 3, &mut |e| {
            let cs = via.entry(e.id).or_default();
            if !cs.iter().any(|x| Rc::ptr_eq(x, c)) {
                cs.push(c.clone());
            }
        }),
        _ => {}
    }
}

/// 一个元素交给判断器的那份材料与来路：
/// 过滤或配对的产物（带 `item` 与 `trail` 的记录）取 `item` 当材料，`trail` 接上上一次的出口；
/// 其余值原样当材料、来路为空。产物与输入同形，可再过滤、再配对（组合封闭）。
/// （输出元素的构造 `element_out`、`is_element` 在步 25-2b 搬进 `constructs/element.rs`，B133。）
pub(crate) fn element_parts(it: &Value) -> (Value, Value) {
    let is_elem =
        matches!(it, Value::Record(_)) && it.get("item").is_some() && it.get("trail").is_some();
    if !is_elem {
        return (it.clone(), Value::list(vec![]));
    }
    let mut t: Vec<Value> = match it.get("trail") {
        Some(Value::List(l)) => l.iter().cloned().collect(),
        _ => vec![],
    };
    if let Some(e) = it.get("exit") {
        if !matches!(e, Value::Unit) {
            t.push(e);
        }
    }
    (it.get("item").unwrap_or(Value::Unit), Value::list(t))
}
