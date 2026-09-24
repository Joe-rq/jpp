//! 把分析结果写进标注表（`20` §2.3 `jpp-check`：「产物写 `AnnotTable`」；`21` 步 12b）。
//!
//! 本步写三样，全部取自已有分析，不新增判定：
//!
//! - **效应行** `Annot.effect`：效应节点写它自己的效应（站点表同样写一份）；分析过的函数节点写
//!   不动点后的行，含效应变量或看不透的被调者时 `open = true`（按「可能有效应」对待）。
//! - **结构摘要** `Annot.summary`（Φ 第一版，只写函数节点）：`may_touch_world` = 行里有
//!   `side_effecting` 的效应或行是开的；`may_refresh` = 行里有 `sched = Immediate` 的效应或行是开的。
//!   口径与诊断相反，看不透的一律按有（`20` §2.3「判不准时的口径」）。步 13b 扩成可组合的过程间摘要。
//! - **责任种类** `Annot.duty_kind`：`let` 带方法类型标注、值是函数字面量时，`Fn¹` 记 `Linear`，
//!   其余方法类型记 `Omega`。没有标注的函数不推断，留空。
//!
//! `refresh_point` 本步不写（步 13 由规划侧产出）。

use crate::*;
use jpp_effects::{ALL, SchedClass, spec};
use jpp_ir::ir::{AnnotTable, DutyKind, EffectRow, Host, Node, Phi};

impl Checker<'_> {
    pub(crate) fn annotate(&self, p: &Program) -> AnnotTable {
        let mut t = AnnotTable::default();
        jpp_ir::ir::walk(&p.body, &mut |e| {
            if let Node::Effect { effect, site, .. } = &e.node {
                let row = EffectRow::of([spec(*effect).name.to_string()]);
                t.node_mut(e.id).effect = Some(row.clone());
                t.sites.entry(*site).or_default().effect = Some(row);
            }
        });
        for f in &self.functions {
            let row = EffectRow {
                known: f.row.concrete.clone(),
                open: f.row.open(),
            };
            let a = t.node_mut(f.node);
            a.summary = Some(phi(&row));
            a.effect = Some(row);
        }
        let mut duties = vec![];
        let_duties(&p.body, &mut duties);
        for (id, d) in duties {
            t.node_mut(id).duty_kind = Some(d);
        }
        t
    }
}

fn phi(row: &EffectRow) -> Phi {
    let specs: Vec<_> = ALL
        .iter()
        .map(|e| spec(*e))
        .filter(|s| row.known.contains(s.name))
        .collect();
    Phi {
        may_touch_world: row.open || specs.iter().any(|s| s.side_effecting),
        may_refresh: row.open || specs.iter().any(|s| s.sched == SchedClass::Immediate),
    }
}

fn let_duties(b: &Block, out: &mut Vec<(jpp_ir::key::NodeId, DutyKind)>) {
    for s in &b.statements {
        match s {
            Statement::Let {
                annotation, value, ..
            } => {
                if let (Some(t), Node::Host(Host::Function(f))) = (annotation, &value.node) {
                    if let Some((_, linear)) = t.as_method() {
                        let d = if linear {
                            DutyKind::Linear
                        } else {
                            DutyKind::Omega
                        };
                        out.push((f.id, d));
                    }
                }
                expr_duties(value, out);
            }
            Statement::Function { function, .. } => let_duties(&function.body, out),
            Statement::Expr(e) => expr_duties(e, out),
        }
    }
    if let Some(r) = &b.result {
        expr_duties(r, out);
    }
}

fn expr_duties(e: &Expr, out: &mut Vec<(jpp_ir::key::NodeId, DutyKind)>) {
    for c in e.children() {
        expr_duties(c, out);
    }
    for b in e.blocks() {
        let_duties(b, out);
    }
}
