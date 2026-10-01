//! 运行期诊断闸门（步 26，B47）：[`jpp_ir::diag_gate::QuestionGate`] 的实现，转调同一份 B13 规则。
//!
//! B47：「诊断层是一个闸门函数 `diagnose_question`……写程序时对字面题直接调用；运行时由……宿主变换
//! `diagnose` 调用」；推翻条件「若静态七条在写时与运行时需要不同口径……拆成两个函数并登记差异」——
//! 所以这里不另写规则，只决定**哪些题**在运行期诊断。
//!
//! 范围 =「检查期没见过的题」。构造时按 [`super::diagnose`] 同一趟遍历收下检查器诊断过的字面量：
//! 字面题面（`test` / `select` / `measure` 的首参）、字面题式模板、全字面的填法。运行期遇到的题：
//! - 不是由题式填出的：题面不在字面题面集合里才跑 `diagnose_question`；
//! - 由题式填出的：模板检查期见过，就只用真实填入值跑 `diagnose_fill`（检查期非字面槽记作 `None`，
//!   `W-diag-abstract-direct` 只有运行期补得上）；填法也全字面、检查期见过，则什么也不报；
//!   模板没见过（运行期拼出的模板），模板与填法都跑。
//!
//! 已知与检查期的一处差异：运行期没有源码里的请求与槽声明，题类按 `op` 缺省推（`QuestionLit::new`）。
//! 这只影响「提及类」由题类触发的那一支，而它要到 `accepts`（B23）落地才会出现（`b13.rs` `mention_scope` 注）。

use std::collections::BTreeSet;

use jpp_ir::diag_gate::{GateNote, GateQuestion, QuestionGate};

use super::b13::{DiagCx, QuestionLit, diagnose_fill, diagnose_question};
use crate::*;

/// 运行期闸门。`RuntimeGate::default()` 不带字面量集合：每道题都诊断（测试与没有程序可读的宿主用）。
#[derive(Clone, Debug, Default)]
pub struct RuntimeGate {
    /// (题型, 题面)
    texts: BTreeSet<(String, String)>,
    /// (题型, 模板)
    templates: BTreeSet<(String, String)>,
    /// (模板, 按槽名排好的全字面填法)
    fills: BTreeSet<(String, Vec<(String, String)>)>,
}

impl RuntimeGate {
    /// 收下检查器对这个程序诊断过的字面量（与 [`super::diagnose`] 同一趟遍历、同一套认法）。
    pub fn new(p: &Program) -> RuntimeGate {
        let mut g = RuntimeGate::default();
        let mut 绑定: HashMap<String, Expr> = HashMap::new();
        收let绑定(&p.body, &mut 绑定);
        walk_block(&p.body, &mut |e| {
            let args = call_args(e);
            match call_name(e) {
                Some(op @ ("test" | "select" | "measure")) => {
                    if let Some(ExprKind::Text(t)) = args.first().map(|a| a.kind()) {
                        g.texts.insert((op.to_string(), t.clone()));
                    }
                }
                Some("form") => {
                    if let (Some(ExprKind::Text(op)), Some(ExprKind::Text(t))) = (
                        args.first().map(|a| a.kind()),
                        args.get(1).map(|a| a.kind()),
                    ) {
                        g.templates.insert((op.clone(), t.clone()));
                    }
                }
                Some("fill") => {
                    let (Some(f), Some(r)) = (args.first(), args.get(1)) else {
                        return;
                    };
                    let f = match f.kind() {
                        ExprKind::Name(n) => 绑定.get(n).unwrap_or(f),
                        _ => f,
                    };
                    if call_name(f) != Some("form") {
                        return;
                    }
                    let Some(ExprKind::Text(t)) = call_args(f).get(1).map(|a| a.kind()) else {
                        return;
                    };
                    let ExprKind::Record(fields) = r.kind() else {
                        return;
                    };
                    let mut lits = vec![];
                    for (k, v) in fields {
                        match v.kind() {
                            ExprKind::Text(s) => lits.push((k.clone(), s.clone())),
                            ExprKind::Integer(i) => lits.push((k.clone(), i.to_string())),
                            _ => return,
                        }
                    }
                    lits.sort();
                    g.fills.insert((t.clone(), lits));
                }
                _ => {}
            }
        });
        g
    }

    fn run(&self, q: &GateQuestion<'_>, only_unseen: bool) -> Vec<GateNote> {
        let cx = DiagCx::default();
        let sp = Span::default();
        let mut out: Vec<Diagnostic> = vec![];
        match (q.template, q.fill) {
            (Some(t), Some(f)) => {
                let mut sorted: Vec<(String, String)> = f.to_vec();
                sorted.sort();
                let 模板见过 = self.templates.contains(&(q.op.to_string(), t.to_string()));
                if only_unseen && 模板见过 && self.fills.contains(&(t.to_string(), sorted)) {
                    return vec![];
                }
                if !(only_unseen && 模板见过) {
                    out.extend(diagnose_question(&QuestionLit::new(q.op, t, true, sp), &cx));
                }
                let fills: Vec<(String, Option<String>)> = f
                    .iter()
                    .map(|(k, v)| (k.clone(), Some(v.clone())))
                    .collect();
                out.extend(diagnose_fill(q.op, t, &fills, sp, &cx));
            }
            _ => {
                if only_unseen && self.texts.contains(&(q.op.to_string(), q.text.to_string())) {
                    return vec![];
                }
                out.extend(diagnose_question(
                    &QuestionLit::new(q.op, q.text, false, sp),
                    &cx,
                ));
            }
        }
        out.into_iter().map(note_of).collect()
    }
}

impl QuestionGate for RuntimeGate {
    fn gate(&self, q: &GateQuestion<'_>) -> Vec<GateNote> {
        self.run(q, true)
    }
    fn diagnose(&self, q: &GateQuestion<'_>) -> Vec<GateNote> {
        self.run(q, false)
    }
}

/// 报文按「说错在哪。修法：…」写（`b13.rs` 头注），拆成两段；没有修法的整段作说明。
fn note_of(d: Diagnostic) -> GateNote {
    let (message, fix) = match d.message.rsplit_once("修法：") {
        Some((m, f)) => (
            m.trim_end_matches(['。', '；', ' ']).to_string(),
            f.trim().to_string(),
        ),
        None => (d.message.clone(), String::new()),
    };
    GateNote {
        code: d.rule,
        message,
        fix,
    }
}
