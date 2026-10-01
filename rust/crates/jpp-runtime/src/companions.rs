//! 伴随题（主控板 B0492 S5；J-05 默认去向草案第三稿 (0a)；意图汇编 30；主会话裁定四十一及补充 B0639）。
//!
//! 没有可信记录的题（第一版：题级与题式级都没有「上岗」记录），在登记它的同一处、同一状态上另登记一组伴随题：
//! 题式来自 `unsure_source({companions})`，没给时取程序环境里的 `unsure_companions`（`lib/unsure.jpp`），都没有就不带。
//! 发法由计划的 `companions` 定：`Same` 与原题同组融合成一次调用；`Parallel` 在同一刷新时刻另成一组、另发一次调用
//! （`flush.rs` 分组键加伴随标记）。读数不进出口：只给默认链选路，并进报告 `improve` 段。
//! 伴随题本身、默认链的「为什么拿不准」与再判不带伴随题（`伴随中`、`链中`）。

use super::*;
use jpp_ir::plan::CompanionMode;

/// 默认链按伴随题读数选路（过程记录 5.9）：认得的模块按校准键找
const 前提: &str = "unsure-companion-premise";
/// 「最缺哪类」select（裁定五十一；过程记录 5.23）：候选是原题的四级候选类别，另加 [`材料不缺`]
const 材料: &str = "unsure-companion-material";
pub(crate) const 材料不缺: &str = "材料不缺";
const 谓词: &str = "diag-two-judgments";
const 参照: &str = "unsure-companion-reference";

/// 一道原题带的伴随题：原题读数键、题面、各伴随题（校准键、读数）
pub(crate) struct 伴随组 {
    pub key: String,
    pub q: String,
    pub items: Vec<(String, Rc<Reading>)>,
    /// 「最缺哪类」那道的候选类别（不含「材料不缺」）；没登记那道为空
    pub 候选: Vec<String>,
}

impl<'a> Interp<'a> {
    /// 这道题有没有可信记录（第一版：题级或题式级有「上岗」记录）
    fn 有可信记录(&self, q: &Question) -> bool {
        let c = self.calib.chain(&q.calib, q.form_hash.as_deref());
        c.question.rec.status == "上岗" || c.form.is_some_and(|f| f.rec.status == "上岗")
    }

    /// 伴随题的题式：`unsure_source` 给的，否则程序环境里的 `unsure_companions`
    fn 伴随题式(&self) -> Vec<Value> {
        if let Some(fs) = &self.伴随题式 {
            return fs.clone();
        }
        match self
            .顶层环境
            .as_ref()
            .and_then(|e| env_lookup(e, "unsure_companions"))
        {
            // 程序自己的 unsure_companions 里有题式才用它；不是题式列表时回落到序言（检查器另报 N-unsure-companions）
            Some(Value::List(l)) if l.iter().any(|x| matches!(x, Value::Form(_))) => l
                .iter()
                .filter(|x| matches!(x, Value::Form(_)))
                .cloned()
                .collect(),
            _ => self.序言伴随.clone().unwrap_or_default(),
        }
    }

    /// 默认链取材料的第二级：程序自己的料库（裁定五十五，B0593）
    pub fn with_category_store(mut self, s: Rc<dyn jpp_effects::CategoryStore>) -> Self {
        self.料库 = Some(s);
        self
    }

    /// 默认链取材料的第三级：宿主取材料端口（裁定五十五）
    pub fn with_material_source(mut self, s: Rc<dyn jpp_effects::MaterialSource>) -> Self {
        self.宿主取材料 = Some(s);
        self
    }

    /// 伴随题序言（主控 2026-09-30 路 A）：宿主交来 `lib/unsure.jpp` 的降级结果
    pub fn with_prelude(mut self, p: Program) -> Self {
        self.序言 = Some(p);
        self
    }

    /// 运行入口（程序体之前）：在独立环境里求值序言（只有 `form` 与 `let`，不发效应），取 `unsure_companions`
    /// 与通用类别表 `unsure_lacks`。Z0398 起不论发法都求值（通用类别表不随伴随题开关变，过程记录 5.19）；
    /// 伴随题登不登记仍由发法定。求值完把 `顶层环境` 复位，程序体进块时再记它自己的
    pub(crate) fn 求值序言(&mut self) -> R<()> {
        let Some(p) = self.序言.take() else {
            return Ok(());
        };
        // Z0622（过程记录 5.32）：序言另行降级，节点号与源码偏移都从 0 起、与用户程序重叠。序言只许是常量定义
        // （题式、列表、`let`），出现效应、cut、fit、loop、handle、consume 或内核构造就拒，不求值——它若发判断，
        // 同偏移同材料同题会与用户程序串键
        if let Some((sp, 什么)) = 序言越界(&p) {
            return err(
                Some("E-prelude"),
                format!(
                    "伴随题序言里出现了{什么}：序言只许题式、列表、let 与纯数据内置这类常量定义（它与用户程序共用从 0 起的节点号与源码偏移，\
                     发判断会与用户程序同偏移的判断串键；unsure_source 这类改运行时状态的内置会静默改写用户程序的默认链，Z0622）。\
                     修法【需接线人】：把它移出序言"
                ),
                sp,
            );
        }
        let env = env_child(&root_env());
        // Z0613：计划只属于用户程序。序言另行降级，节点号与用户程序重叠，按节点号查计划会查到用户程序的条目
        // （曾拿用户程序的提升下标索引序言的块而越界 panic）。序言只有题式与 `let`，没有判断，求值时用空计划，求值完换回。
        // Z0622：按偏移索引的站点表与默认链站点同样换空表
        let 计划 = std::mem::replace(&mut self.plan, jpp_ir::plan::Plan::empty());
        let 站点表 = std::mem::take(&mut self.站点表);
        let 链站点 = std::mem::take(&mut self.默认链站点);
        let 结果 = self.eval_block(&p.body, &env);
        self.plan = 计划;
        self.站点表 = 站点表;
        self.默认链站点 = 链站点;
        结果?;
        let 顶层 = self.顶层环境.take();
        self.序言伴随 = match 顶层
            .as_ref()
            .and_then(|e| env_lookup(e, "unsure_companions"))
        {
            Some(Value::List(l)) => Some(
                l.iter()
                    .filter(|x| matches!(x, Value::Form(_)))
                    .cloned()
                    .collect(),
            ),
            _ => None,
        };
        self.序言类别 = 文本表(
            顶层
                .as_ref()
                .and_then(|e| env_lookup(e, "unsure_lacks"))
                .as_ref(),
        );
        Ok(())
    }

    /// `judge` 登记原题之后调：为没有可信记录的题在同一状态上登记伴随题
    pub(crate) fn 登记伴随(
        &mut self,
        state: &Rc<State>,
        qs: &[Rc<Question>],
        rs: &[Value],
        sp: Span,
    ) -> R<()> {
        // 审计重放：带不带伴随题只看账本（主控复核 2026-09-30）——不看发法开关，也不看这趟给的校准记录；
        // 首跑带了的，重放照带（`登记一组` 按账本查），首跑没带的不带
        if (!self.audit.on && self.plan.companions == CompanionMode::Off)
            || self.伴随中
            || self.裂变块中 > 0
            || self.链中 > 0
            || self.拟合中 > 0
        {
            return Ok(());
        }
        let 题式 = self.伴随题式();
        if 题式.is_empty() {
            return Ok(());
        }
        for (q, r) in qs.iter().zip(rs) {
            let Value::Reading(r) = r else { continue };
            if r.ledger_key.is_empty()
                || (!self.audit.on && self.有可信记录(q))
                || self.伴随.iter().any(|g| g.key == r.ledger_key)
            {
                continue;
            }
            self.伴随中 = true;
            let 结果 = self.登记一组(state, q, &题式, sp);
            self.伴随中 = false;
            let (items, 候选) = 结果?;
            if !items.is_empty() {
                self.伴随.push(伴随组 {
                    key: r.ledger_key.clone(),
                    q: q.text.clone(),
                    items,
                    候选,
                });
            }
        }
        Ok(())
    }

    fn 登记一组(
        &mut self,
        state: &Rc<State>,
        q: &Question,
        题式: &[Value],
        sp: Span,
    ) -> R<(Vec<(String, Rc<Reading>)>, Vec<String>)> {
        let mut cqs = vec![];
        // K 选一的伴随题（「最缺哪类」）另登在换了 over 槽的状态上：候选 = 原题的四级候选 + 「材料不缺」（裁定五十一）
        let mut 选题 = vec![];
        for f in 题式 {
            match self.builtin(
                "fill",
                vec![
                    f.clone(),
                    Value::record(vec![("q".into(), Value::text(&q.text))]),
                ],
                sp,
            )? {
                Value::Question(c) if c.op == Op::Select => 选题.push(c),
                Value::Question(c) => cqs.push(c),
                _ => return err(Some("E-rt-question"), "伴随题式没有填出题", sp),
            }
        }
        let (候选, _) = self.候选类别(q);
        // 审计重放：带不带伴随题照账本（首跑按那时的校准记录定的；重放时记录由账本补回得晚，按记录判会与首跑分叉）
        if self.audit.on
            && let Some(c) = cqs.first()
        {
            let k = self.judge_key_of(&state.hash, &c.hash, c.op.phys(), sp.start);
            // 首跑这道回答形状不符被丢掉时账本里是 Absent（Z0556）
            if self.账本查(&k).is_none() && self.账本查(&format!("absent:{k}")).is_none() {
                return Ok((vec![], vec![]));
            }
        }
        let mut 登记: Vec<(Rc<State>, Vec<Rc<Question>>)> = vec![(state.clone(), cqs)];
        if !选题.is_empty() && !候选.is_empty() {
            let mut over: Vec<Value> = 候选.iter().map(|c| Value::text(c)).collect();
            over.push(Value::text(材料不缺));
            let s2 = self.换槽(state, None, Some(over), sp)?;
            登记.push((s2, 选题));
        }
        let mut out = vec![];
        for (st, qs) in 登记 {
            let rs = self.judge(&st, &qs, sp)?;
            for (c, r) in qs.iter().zip(rs) {
                if let Value::Reading(r) = r {
                    if self.plan.companions == CompanionMode::Parallel {
                        self.伴随键.insert(r.ledger_key.clone());
                    }
                    out.push((c.calib.clone(), r));
                }
            }
        }
        Ok((out, 候选))
    }

    /// 「最缺哪类」那道的回答：`Some(Some(类别))` 选出了某一类；`Some(None)` 选了「材料不缺」或最大项并列（没选出）；
    /// 没登记那道为 `None`
    fn 最缺哪类(&self, g: &伴随组) -> Option<Option<String>> {
        let (_, r) = g.items.iter().find(|(c, _)| c == 材料)?;
        let Some(Answer::Choice(ps)) = self.answer_of(r) else {
            return Some(None);
        };
        Some(match self.选中项(&ps) {
            Some(k) if k < g.候选.len() => Some(g.候选[k].clone()),
            _ => None,
        })
    }

    /// 「最缺哪类」的选中项（Z0556，过程记录 5.26；与裁定五十 select 并列同一判法）：最大两个概率相差 ≤ 画像
    /// `delta.choice_prob_chosen.mid` + `BOUNDARY_EPS` 为没选出；δ 未知（画像没有 δ，或缺中段）退化为恰好相等才并列，
    /// 元题不为缺 mid 中止程序
    pub(crate) fn 选中项(&self, ps: &[f64]) -> Option<usize> {
        let (mut k, mut 最大, mut 次大) = (None, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (i, &p) in ps.iter().enumerate() {
            if p > 最大 {
                次大 = 最大;
                最大 = p;
                k = Some(i);
            } else if p > 次大 {
                次大 = p;
            }
        }
        let k = k?;
        let 并列 = match self.calib.profile().delta_prior(Op::Select) {
            Some(δ) => 最大 - 次大 <= δ + jpp_value::stat::BOUNDARY_EPS,
            None => 最大 == 次大,
        };
        (!并列).then_some(k)
    }

    /// 默认链按伴随题读数选路：`("unclear", _)` 题不清；`("ambiguous", _)` 两可；`("enrich", 类别)` 去补——「最缺哪类」
    /// 选出了类别就带上它（第一轮直接用，不再发「为什么」），没选出而参照不够时为 `None`（照旧发「为什么」）。
    /// 没有伴随题或读数不全时 `None`（过程记录 5.23）
    pub(crate) fn 伴随路由(&self, key: &str) -> Option<(&'static str, Option<String>)> {
        let g = self.伴随.iter().find(|g| g.key == key)?;
        let p = |k: &str| -> Option<f64> {
            let (_, r) = g.items.iter().find(|(c, _)| c == k)?;
            match self.answer_of(r) {
                Some(Answer::Noul(p)) => Some(p),
                _ => None,
            }
        };
        // 切法那道不进选路（主控 2026-09-30：H2 真机 34 条全在 0.72–0.83，没有区分力），只进 improve
        let (前, 谓, 参) = (p(前提)?, p(谓词)?, p(参照)?);
        let 缺 = self.最缺哪类(g).flatten();
        Some(if 前 < 0.5 || 谓 > 0.5 {
            ("unclear", None)
        } else if let Some(c) = 缺 {
            ("enrich", Some(c))
        } else if 参 < 0.5 {
            ("enrich", None)
        } else {
            ("ambiguous", None)
        })
    }

    /// 「最缺哪类」的唯一最大项是「材料不缺」
    fn 选了材料不缺(&self, g: &伴随组) -> bool {
        let Some((_, r)) = g.items.iter().find(|(c, _)| c == 材料) else {
            return false;
        };
        let Some(Answer::Choice(ps)) = self.answer_of(r) else {
            return false;
        };
        self.选中项(&ps) == Some(g.候选.len())
    }

    /// 报告 `improve` 段（「这道题怎样能更拿得准」）：每道带了伴随题的原题一行
    pub(crate) fn 伴随报告(&self) -> Vec<Json> {
        self.伴随
            .iter()
            .map(|g| {
                let cs: Vec<Json> = g
                    .items
                    .iter()
                    .map(|(c, r)| match self.answer_of(r) {
                        Some(Answer::Noul(p)) => json!({"kind": c, "key": r.ledger_key, "p": p}),
                        // 「最缺哪类」：选中项下标与它的概率
                        // 「最缺哪类」：选中项下标（没选出为 null，Z0556）与最大概率
                        Some(Answer::Choice(ps)) => {
                            let p = ps.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                            json!({"kind": c, "key": r.ledger_key, "pick": self.选中项(&ps), "p": p})
                        }
                        // 没有读数（形状不符被丢掉，Z0556）
                        _ if matches!(r.op, Op::Select) => json!({"kind": c, "key": r.ledger_key, "pick": Json::Null, "p": Json::Null}),
                        _ => json!({"kind": c, "key": r.ledger_key, "p": Json::Null}),
                    })
                    .collect();
                // 「最缺哪类」选中的类别（裁定五十一）：类别、「材料不缺」，没选出为 null
                let lacks = match self.最缺哪类(g) {
                    Some(Some(c)) => json!(c),
                    Some(None) if self.选了材料不缺(g) => json!(材料不缺),
                    _ => Json::Null,
                };
                json!({"key": g.key, "q": g.q, "companions": cs, "lacks": lacks})
            })
            .collect()
    }
}

/// 非空的文本列表（通用类别表）；别的形状为 `None`
pub(crate) fn 文本表(v: Option<&Value>) -> Option<Vec<String>> {
    let Some(Value::List(l)) = v else { return None };
    let out: Vec<String> = l
        .iter()
        .filter_map(|x| match x {
            Value::Text(t, _) => Some(t.to_string()),
            _ => None,
        })
        .collect();
    (out.len() == l.len() && !out.is_empty()).then_some(out)
}

/// Z0622（过程记录 5.32）：序言里第一处不是常量定义的节点（效应、cut、fit、loop、handle、consume，以及题与题式之外的内核构造）
/// 序言里可用的内置：只造值、不读写运行时状态、没有输出（过程记录 5.34）。新增内置默认不在此列
fn 序言纯内置(n: &str) -> bool {
    matches!(
        n,
        "state"
            | "mat"
            | "content"
            | "form"
            | "fill"
            | "test"
            | "select"
            | "measure"
            | "len"
            | "map"
            | "filter"
            | "fold"
            | "range"
            | "append"
            | "concat"
            | "slice"
            | "contains"
            | "sum"
            | "reverse"
            | "keys"
            | "with"
            | "has"
            | "text"
            | "join"
            | "min"
            | "max"
            | "abs"
            | "floor"
            | "split"
            | "lower"
            | "upper"
            | "trim"
            | "replace"
            | "starts_with"
            | "ends_with"
            | "index_of"
            | "chars"
            | "regex_match"
            | "regex_find"
            | "sort"
            | "sort_by"
            | "parse_json"
            | "to_json"
            | "hash"
            | "date_parse"
            | "date_format"
            | "date_add"
            | "rand"
            | "rand_int"
            | "shuffle"
    )
}

pub fn 序言越界(p: &Program) -> Option<(Span, String)> {
    use jpp_ir::ir::Node;
    let mut 坏: Option<(Span, String)> = None;
    jpp_ir::ir::walk(&p.body, &mut |e| {
        if 坏.is_some() {
            return;
        }
        let 什么 = match &e.node {
            Node::Effect { effect, .. } => Some(format!("效应 {effect:?}")),
            Node::Cut { .. } => Some("cut".into()),
            Node::Fit { .. } => Some("fit".into()),
            Node::Loop { .. } => Some("loop".into()),
            Node::Handle { .. } => Some("handle".into()),
            Node::Consume { .. } => Some("consume / escalate / literalize".into()),
            // 题与题式的构造是常量（不发效应、不登记站点）；其余内核构造（sieve、pair 等）拒
            Node::Construct { name, .. }
                if !matches!(
                    name.as_str(),
                    "form" | "fill" | "test" | "select" | "measure"
                ) =>
            {
                Some(format!("内核构造 {name}"))
            }
            // Z0622 补漏（过程记录 5.34）：内置按名字引用查（别名、当实参传都算），只放行纯数据内置；
            // unsure_source 等会改运行时全局状态，序言调它会静默改写用户程序的默认链
            Node::Host(jpp_ir::ir::Host::Name(n))
                if crate::BUILTINS.contains(&n.as_str()) && !序言纯内置(n) =>
            {
                Some(format!("内置 {n}"))
            }
            _ => None,
        };
        if let Some(w) = 什么 {
            坏 = Some((e.span.clone(), w));
        }
    });
    坏
}
