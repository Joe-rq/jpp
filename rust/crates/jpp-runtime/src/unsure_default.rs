//! J-05 默认链（主控板 B0492 S2）：作者没给去向的未决，语言替他给——问缺哪类信息 → 取来补进材料 → 再判；
//! 补不到或补了仍拿不准就记账放弃。缺席类与别的原因不补，转交给程序结果。
//!
//! 依据：意图汇编 7c、30；主会话裁定六（「补不到或补了仍拿不准就记账放弃」）；J-05 草案第三稿 (1)–(5)
//! （`地基/附注/2026-09-29-J-05默认去向-条文改法草案.md`，末端按裁定六，见过程记录 5.3）；主控板 Z0207 第 4、5、9 条。
//! 触发点只有一处：`handle` 收到未决出口、臂表没有 `unsure`、没开 `--guard`（`duty.rs::handle`）。

use super::*;
use crate::duty::{去向, 责任键};

/// 判过而拿不准、补信息再判有意义的原因（草案 (1)）：`band`、`tie`、`insufficient`。
/// 步 36 G3 前出口原因带槽名（`insufficient:<槽>`），这张表从没匹配上 `insufficient`；G3 把原因收成成员名后暂时
/// 显式排除，Z0589（过程记录 5.28）开启：取到的材料补进它缺的那个证据槽（见 `补进的槽`）
fn 可补(e: &Exit) -> bool {
    match e.why().map(|w| w.cause) {
        Some(UnsureCause::Band | UnsureCause::Tie) => true,
        // Z0589 返修（5.28 附录）：只有缺 ref、ctx 能靠取材料补上；缺 over（非 K 选一题恒判缺，B155）或 on
        // 取什么都补不上，不进轮次
        Some(UnsureCause::Insufficient) => matches!(e.detail(), Some("ref" | "ctx")),
        _ => false,
    }
}

/// 取来的材料补进哪个槽（Z0589）：`insufficient` 出口的细节是缺的决定性证据槽（J-09），补进它（`ref` 或 `ctx`）；
/// 否则（band、tie，或细节不是这两个槽）补进 `ctx`——补进 `ctx` 对 `insufficient:ref` 没用，再判仍缺 `ref`
fn 补进的槽(e: &Exit) -> &'static str {
    match (e.why().map(|w| w.cause), e.detail()) {
        (Some(UnsureCause::Insufficient), Some("ref")) => "ref",
        _ => "ctx",
    }
}
/// 补信息轮次的结果：最后那个出口、补过没有（至少再判过一次）、问过取过的类别
pub(crate) struct 轮果 {
    last: Rc<Exit>,
    补过: bool,
    asked: Vec<String>,
    fetched: Vec<String>,
    missed: Vec<String>,
    why: Option<&'static str>,
    /// 候选取自哪一级：`unsure_source` / `lacks` / `generic`（裁定五十一、五十二 (b)）
    source: Option<&'static str>,
    /// 路 C（裁定五十五）：没有任何取法，问出缺的类别就停，末端转交
    needed: Vec<String>,
    /// 与 `fetched` 一一对应：材料由哪一级给的（`fetch` / `store` / `host`，过程记录 5.20）
    fetched_by: Vec<&'static str>,
    /// insufficient 链末缺的证据槽（Z0589 返修，5.28 附录补）
    slot: Option<String>,
}

/// 「为什么拿不准」那道 select 在类别之外的两个候选（草案 (2)）
const 两可: &str = "不缺信息，事情本身两可";
const 题不清: &str = "题问得不清或前提不成立";

impl<'a> Interp<'a> {
    /// `unsure_source({need?: [类别…], fetch?: fn(q, need, m) { … }, companions?: [题式…]})`：程序声明默认链的候选类别、
    /// 取材料函数与伴随题式（Z0207 第 4 条路 A；Z0398 起三项都可省、至少给一项，过程记录 5.19）。
    /// 没给 `need` 时候选按题 `lacks`、通用表取；没给 `fetch` 时取法落到下一级（都没有走路 C，裁定五十五）。
    /// 后写的覆盖先写的。
    pub(crate) fn b_unsure_source(
        &mut self,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let 用法 = "unsure_source({need?: [\"类别\", …], fetch?: fn(q, need, m) { … }, companions?: [题式…]})：need 是可取的信息类别，fetch 按类别取材料（取不到返回 fail(…)），至少给一项";
        if args.len() != 1 {
            return err(
                Some("E-rt-arity"),
                format!("{name} 需要 1 个参数。{用法}"),
                sp,
            );
        }
        let 有 = |k: &str| !matches!(args[0].get(k), None | Some(Value::Unit));
        if !有("need") && !有("fetch") && !有("companions") {
            return err(Some("E-rt-arg"), 用法, sp);
        }
        let mut 类别 = vec![];
        match args[0].get("need") {
            None | Some(Value::Unit) => {}
            Some(Value::List(need)) => {
                for x in need.iter() {
                    let Value::Text(t, _) = x else {
                        return err(Some("E-rt-arg"), format!("need 里要是文本。{用法}"), sp);
                    };
                    if !类别.contains(&t.to_string()) {
                        类别.push(t.to_string());
                    }
                }
                if 类别.is_empty() {
                    return err(Some("E-rt-arg"), format!("need 不能是空列表。{用法}"), sp);
                }
            }
            Some(_) => return err(Some("E-rt-arg"), 用法, sp),
        }
        let fetch = match args[0].get("fetch") {
            None | Some(Value::Unit) => None,
            Some(Value::Fn(f)) if f.function.parameters.len() == 3 => Some(f.clone()),
            Some(_) => return err(Some("E-rt-arg"), 用法, sp),
        };
        self.未决来源 = Some((类别, fetch));
        // 伴随题（B0492 S5）：给了 companions 就换成作者的题式
        if let Some(Value::List(cs)) = args[0].get("companions") {
            if cs.iter().any(|x| !matches!(x, Value::Form(_))) {
                return err(
                    Some("E-rt-arg"),
                    "unsure_source 的 companions 要是题式（form）的列表",
                    sp,
                );
            }
            self.伴随题式 = Some(cs.iter().cloned().collect());
        }
        Ok(Value::Unit)
    }

    /// `refine(u, 新出口, {need, round, asked_by?})`：库代码（`lib/compose/tree.jpp` 的补信息）记一轮补信息与细化：
    /// `Enrich{got: true}` 与 `Refine{how: default, to: 新出口的键}`，并消费 `u`。
    pub(crate) fn b_refine(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        let 用法 = "refine(u, 新出口, {need: 类别, round: 第几轮, asked_by?: 选类别那道题的出口})，或取不到时 refine(u, unit, {need, round, got: false})";
        if args.len() != 3 {
            return err(
                Some("E-rt-arity"),
                format!("{name} 需要 3 个参数。{用法}"),
                sp,
            );
        }
        let (Value::Exit(u) | Value::Duty(u)) = &args[0] else {
            return err(
                Some("E-rt-arg"),
                format!("第一个参数要是未决出口或责任。{用法}"),
                sp,
            );
        };
        // 取不到的一轮（主控复核 2026-09-30）：refine(u, unit, {need, round, got: false}) 只记 Enrich{got: false}，
        // 责任不动（作者随后照常转交或放弃）
        if let (Value::Unit, Some(Value::Bool(false, ..))) = (&args[1], args[2].get("got")) {
            let (Some(Value::Text(need, _)), Some(Value::Int(round, _))) =
                (args[2].get("need"), args[2].get("round"))
            else {
                return err(Some("E-rt-arg"), 用法, sp);
            };
            let u = u.clone();
            self.记补信息(&u, None, &need, round.max(0) as u32, false, None, sp);
            return Ok(Value::Unit);
        }
        let Value::Exit(e2) = &args[1] else {
            return err(
                Some("E-rt-arg"),
                format!("第二个参数要是接替它的出口。{用法}"),
                sp,
            );
        };
        let (Some(Value::Text(need, _)), Some(Value::Int(round, _))) =
            (args[2].get("need"), args[2].get("round"))
        else {
            return err(Some("E-rt-arg"), 用法, sp);
        };
        if !u.is_unsure() {
            return err(
                Some("E-rt-arg"),
                format!("refine 只收未决出口，收到 {}", u.label()),
                sp,
            );
        }
        let asked_by = match args[2].get("asked_by") {
            Some(Value::Exit(x)) => Some(x.ledger_key.borrow().clone()).filter(|k| !k.is_empty()),
            _ => None,
        };
        let (u, e2) = (u.clone(), e2.clone());
        if !u.consumed.get() || crate::duty::已被吸收(&u) {
            self.记补信息(&u, None, &need, round.max(0) as u32, true, asked_by, sp);
            let to = Some(e2.ledger_key.borrow().clone()).filter(|k| !k.is_empty());
            self.记去向(&u, 去向::Refine { how: "default", to }, "refine", sp);
        }
        u.consumed.set(true);
        *u.consumed_by.borrow_mut() = "refine:default".into();
        Ok(Value::Unit)
    }

    /// 默认链再判要用的来历：`judge` 登记时按读数账本键记下状态与题
    pub(crate) fn 记判断来历(&mut self, s: &Rc<State>, qs: &[Rc<Question>], rs: &[Value]) {
        for (q, r) in qs.iter().zip(rs) {
            if let Value::Reading(r) = r {
                self.判断来历
                    .insert(r.ledger_key.clone(), (s.clone(), q.clone()));
                self.读数表.insert(r.ledger_key.clone(), r.clone());
            }
        }
    }

    /// 默认链再判用同一条线：`cut` 时按读数账本键记下校准键与切法（同一读数切多次时后切的覆盖）
    pub(crate) fn 记切法来历(
        &mut self,
        v: &Value,
        calib: Option<&str>,
        opts: &bridge::CutOpts,
    ) {
        let rs: Vec<&Rc<Reading>> = match v {
            Value::Reading(r) => vec![r],
            Value::List(l) => l
                .iter()
                .filter_map(|x| match x {
                    Value::Reading(r) => Some(r),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        };
        for r in rs {
            self.切法来历.insert(
                r.ledger_key.clone(),
                (calib.map(String::from), opts.clone()),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn 记补信息(
        &mut self,
        e: &Exit,
        原因: Option<&str>,
        need: &str,
        round: u32,
        got: bool,
        asked_by: Option<String>,
        sp: Span,
    ) {
        self.写去向(Entry::Enrich {
            of: 责任键(e),
            cause: match 原因 {
                Some(c) if !e.is_unsure() => c.into(),
                _ => e.cause(),
            },
            site: sp.start,
            need: need.into(),
            round,
            got,
            asked_by,
        });
    }

    /// `handle` 缺 unsure 臂（S2）：走链；再判已决进作者对应的臂，仍未决时 `handle` 的值是这份责任
    /// （`Value::Duty`，已记账放弃或已转交）。
    pub(crate) fn 默认链(&mut self, e: &Rc<Exit>, arms: &Value, sp: Span) -> R<Value> {
        let f = self.补信息链(e, sp)?;
        if f.is_unsure() {
            Ok(Value::Duty(f))
        } else {
            self.handle(&f, arms, sp)
        }
    }

    /// `unsure_default(出口)`（Z0514）：对一个已有的未决出口走默认链，与 `handle` 缺 `unsure` 臂同一份实现与记账
    /// （`补信息链`）。回 `{exit: 最后那个出口, end}`，`end` 是 `decided`（再判成已决）、`drop`（末端记账放弃）、
    /// `handoff`（末端转交，已登记解除）、`carry`（缺席类，随值走，Z0602）或 `none`（已决、已消费、开了 `--guard`、
    /// 原因不可补：原样不动，什么也不记）
    pub(crate) fn b_unsure_default(&mut self, args: Vec<Value>, sp: Span) -> R<Value> {
        if args.len() != 1 {
            return err(Some("E-rt-arity"), "unsure_default 需要 1 个参数", sp);
        }
        let e = match &args[0] {
            Value::Duty(e) | Value::Exit(e) => e.clone(),
            other => {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "unsure_default 只收未决责任或出口，收到 {}",
                        other.type_name()
                    ),
                    sp,
                );
            }
        };
        let 回 = |x: Rc<Exit>, end: &str| {
            Value::record(vec![
                ("exit".into(), Value::Exit(x)),
                ("end".into(), Value::text(end)),
            ])
        };
        if self.guard || !e.is_unsure() || e.consumed.get() || !可补(&e) {
            return Ok(回(e, "none"));
        }
        let f = self.补信息链(&e, sp)?;
        let end = if !f.is_unsure() {
            "decided"
        } else if !f.consumed.get() {
            "carry"
        } else if f.consumed_by.borrow().as_str() == "default:drop" {
            "drop"
        } else {
            "handoff"
        };
        Ok(回(f, end))
    }

    /// 「无作者去向」站点（S2c）：这个站点的 `cut` 当场走链，下游拿到的是最后那个出口（已决，或已记账的未决）
    pub(crate) fn 站点走链(&mut self, v: Value, sp: Span) -> R<Value> {
        if self.guard || !self.默认链站点.contains(&sp.start) {
            return Ok(v);
        }
        match self.检视(v)? {
            Value::Exit(e) if e.is_unsure() && !e.consumed.get() => {
                Ok(Value::Exit(self.补信息链(&e, sp)?))
            }
            Value::List(l) => {
                let mut out = Vec::with_capacity(l.len());
                for x in l.iter() {
                    out.push(match x {
                        Value::Exit(e) if e.is_unsure() && !e.consumed.get() => {
                            Value::Exit(self.补信息链(e, sp)?)
                        }
                        other => other.clone(),
                    });
                }
                Ok(Value::list(out))
            }
            other => Ok(other),
        }
    }

    /// 默认链本体（草案 (2)–(5)）：问、取、同线再判，直到已决或停止；仍未决的在末端记账放弃（可补组）或转交（其余）。
    /// 返回最后那个出口。
    pub(crate) fn 补信息链(&mut self, e: &Rc<Exit>, sp: Span) -> R<Rc<Exit>> {
        let 原键 = e.ledger_key.borrow().clone();
        let 果 = self.补轮(
            e,
            None,
            &|_me: &Self, x: &Exit, _r: &Reading| x.is_unsure() && 可补(x),
            sp,
        )?;
        let mut 果 = 果;
        let cur = 果.last.clone();
        if !cur.is_unsure() {
            self.记默认链(&原键, e, sp, &果, "decided");
            return Ok(cur);
        }
        // 配置了取材料、各级都取不到（一类也没取到）：照裁定五十五走路 C，取不到的类别进 needed、转交，不记放弃
        // （主控板 Z0502 默认读法，过程记录 5.23）
        if 果.fetched.is_empty() && !果.missed.is_empty() {
            let missed = 果.missed.clone();
            果.needed.extend(missed);
        }
        // Z0589 返修（5.28 附录及其补）：insufficient 链末一律转交（取不到、补后仍缺、候选用完、第一轮就停、不可补的槽
        // 都在此），报告行另写 slot（缺的证据槽）；needed 照裁定五十五只写信息类别——前提派生（B0470）落地前，
        // 缺哪个槽是交给调用方的信息，不当拿不准放弃
        let 缺槽 = cur
            .why()
            .is_some_and(|w| w.cause == UnsureCause::Insufficient);
        if 缺槽 {
            果.slot = Some(cur.detail().unwrap_or("evidence").to_string());
        }
        // 末端：判过而拿不准的记账放弃（裁定六）；缺席类与别的原因转交给程序结果（B95：不能丢）；
        // 路 C（没有任何取法，问出了缺哪类）转交，类别进报告 needed（裁定五十五）
        let end = if 果.needed.is_empty() && 可补(&cur) && !缺槽 {
            self.记去向(&cur, 去向::Drop, "默认链", sp);
            "drop"
        } else if 果.needed.is_empty() && cur.why().is_some_and(|w| w.cause.is_absent_class()) {
            // Z0602（主控板 Z0601，过程记录 5.30）：缺席类没观察到、不能放弃（B95），也不当场写转交——转交只在值里
            // 确实带着那一项时才销账（裁定五十七备案 (2)）。这份未决随值往下走：随返回值离开程序时由程序结束的
            // 记转交写 Handoff，被丢掉则段末记违规（G2）。不置已消费、不登记解除
            self.记默认链(&原键, e, sp, &果, "carry");
            return Ok(cur);
        } else {
            // Z0577（过程记录 5.27）：转交也按 B162 在键上登记已解除，同键其余视图随之解除——否则程序结束时它们会
            // 再补记一条 Handoff，或被当成没人接。键已由另一视图解除（它先走完了链）时报 W-duty-twice、不再写
            let 键 = 责任键(&cur);
            if !键.is_empty() && 键.iter().all(|k| self.解除.contains_key(k)) {
                self.登记解除(&cur, "默认链：转交", sp);
            } else {
                self.记转交(&[cur.clone()]);
                self.登记解除(&cur, "默认链：转交", sp);
            }
            "handoff"
        };
        cur.consumed.set(true);
        *cur.consumed_by.borrow_mut() = format!("default:{end}");
        self.记默认链(&原键, e, sp, &果, end);
        Ok(cur)
    }

    /// 补信息的轮次（默认链与读数触发共用）：`继续(当前出口, 当前读数)` 为真就问、取、同线再判一轮。
    /// 每轮记 `Enrich`（`原因` 给了就用它，否则用出口的原因）与被取代出口的 `Refine{how: default}`。
    /// 候选用完、没有来源或来历、「为什么」选了两可 / 题不清 / 自己拿不准时停。期间不再触发读数触发（`链中`）。
    fn 补轮(
        &mut self,
        e: &Rc<Exit>,
        原因: Option<&str>,
        继续: &dyn Fn(&Self, &Exit, &Reading) -> bool,
        sp: Span,
    ) -> R<轮果> {
        self.链中 += 1;
        let r = self.补轮内(e, 原因, 继续, sp);
        self.链中 -= 1;
        r
    }

    fn 补轮内(
        &mut self,
        e: &Rc<Exit>,
        原因: Option<&str>,
        继续: &dyn Fn(&Self, &Exit, &Reading) -> bool,
        sp: Span,
    ) -> R<轮果> {
        let mut cur = e.clone();
        let mut 果 = 轮果 {
            last: e.clone(),
            补过: false,
            asked: vec![],
            fetched: vec![],
            missed: vec![],
            why: None,
            source: None,
            needed: vec![],
            fetched_by: vec![],
            slot: None,
        };
        let mut 已取: Vec<String> = vec![];
        let mut round = 0u32;
        // 读数触发（原因给了）：没有取法时不发任何调用，只记 near_boundary（过程记录 5.19 第 9 条）
        let 触发 = 原因.is_some();
        let mut 伴随类别: Option<String> = None;
        loop {
            let fetch = self.未决来源.as_ref().and_then(|(_, f)| f.clone());
            let key = cur.ledger_key.borrow().clone();
            let Some((st, q)) = self.判断来历.get(&key).cloned() else {
                break;
            };
            let Some(当前读数) = self.读数表.get(&key).cloned() else {
                break;
            };
            if !继续(self, &cur, &当前读数) {
                break;
            }
            let (calib, opts) = self.切法来历.get(&key).cloned().unwrap_or_default();
            let (类别, 来源) = self.候选类别(&q);
            果.source.get_or_insert(来源);
            let 候选: Vec<String> = 类别.into_iter().filter(|c| !已取.contains(c)).collect();
            // 账本里记过这道读数取来的材料（Z0494）也算有取法：重放不带料库与端口时照账本取
            let 有取法 = fetch.is_some()
                || self.料库.is_some()
                || self.宿主取材料.is_some()
                || self.账本记过取来材料(&format!("fetch/{key}/"));
            if 候选.is_empty() || (触发 && !有取法) {
                break;
            }
            // 伴随题（B0492 S5，草案 (2)「已有伴随题读数的，直接用」）：第一轮按伴随题读数选路；题不清、两可即停
            if round == 0 {
                match self.伴随路由(&key) {
                    Some(("unclear", _)) => {
                        果.why = Some("unclear");
                        break;
                    }
                    Some(("ambiguous", _)) => {
                        果.why = Some("ambiguous");
                        break;
                    }
                    Some(("enrich", Some(c))) => 伴随类别 = Some(c),
                    _ => {}
                }
            }
            // 问：伴随题「最缺哪类」第一轮已选出、且仍在候选里，直接用（裁定五十一，过程记录 5.23）；
            // 候选多于一类才问判断器（只有一类时代码能定，意图汇编 7a）
            let 已选 = 伴随类别.take().filter(|c| 候选.contains(c));
            let (need, asked_by) = if let Some(c) = 已选 {
                (c, None)
            } else if 候选.len() == 1 {
                (候选[0].clone(), None)
            } else {
                果.asked.extend(候选.iter().cloned());
                let mut over: Vec<Value> = 候选.iter().map(|c| Value::text(c)).collect();
                over.push(Value::text(两可));
                over.push(Value::text(题不清));
                let ws = self.换槽(&st, None, Some(over), sp)?;
                let wq = self.builtin(
                    "select",
                    vec![
                        Value::text(&format!(
                            "要把这道题判得更明确：「{}」。它为什么拿不准？",
                            q.text
                        )),
                        Value::text("unsure-why"),
                    ],
                    sp,
                )?;
                let Value::Question(wq) = wq else {
                    return err(Some("E-rt-arg"), "默认链：「为什么拿不准」没造出题", sp);
                };
                // 「为什么拿不准」是语言自己发的元题：回答形状不符时降级（Z0562，过程记录 5.26）
                self.元题登记 += 1;
                let we = self.判一次(&ws, &wq, None, bridge::CutOpts::default(), sp);
                self.元题登记 -= 1;
                let we = we?;
                we.consumed.set(true);
                *we.consumed_by.borrow_mut() = "default:why".into();
                match we.kind {
                    ExitKind::Pick(k) if k < 候选.len() => {
                        let wk = we.ledger_key.borrow().clone();
                        (候选[k].clone(), Some(wk).filter(|k| !k.is_empty()))
                    }
                    ExitKind::Pick(k) if k == 候选.len() => {
                        果.why = Some("ambiguous");
                        break;
                    }
                    ExitKind::Pick(_) => {
                        果.why = Some("unclear");
                        break;
                    }
                    _ => {
                        // 语言自己的诊断题拿不准：记账放弃这道题，停止补
                        self.记去向(&we, 去向::Drop, "默认链：为什么拿不准", sp);
                        果.why = Some("unsure");
                        break;
                    }
                }
            };
            // 路 C（裁定五十五）：没有任何取法，问出缺哪类就停，末端转交、类别进报告
            if !有取法 {
                self.重放取不到(&cur, &need, round + 1, sp)?;
                果.needed.push(need);
                break;
            }
            // 取：逐级往下（裁定五十五，过程记录 5.20）——作者 fetch > 料库 > 宿主端口；没配置或取不到就试下一级
            round += 1;
            已取.push(need.clone());
            let 原料 = st.on.first().cloned();
            let mut 取得: Option<(Mat, &'static str)> = None;
            if let Some(fetch) = &fetch {
                let m0 = match &原料 {
                    Some(m) => Value::Mat(Rc::new(m.clone())),
                    None => Value::Unit,
                };
                let got = self.call_closure(
                    fetch,
                    vec![Value::Question(q.clone()), Value::text(&need), m0],
                    sp,
                )?;
                let got = self.检视(got)?;
                if !matches!(got, Value::Fail(..)) {
                    let m = match &got {
                        Value::Mat(m) => (**m).clone(),
                        other => self.as_mat(
                            &Value::record(vec![(need.clone(), other.clone())]),
                            "ctx",
                            sp,
                        )?,
                    };
                    取得 = Some((m, "fetch"));
                }
            }
            // Z0494（过程记录 5.24）：料库与宿主端口取来的材料是不定输入，记成 V5 `Opaque`；续跑与审计重放先查账本，
            // 已取过的照账本用，不再调；审计重放只凭账本（B35），不调这两级
            let 材料键 = format!("fetch/{key}/{need}/{round}");
            if 取得.is_none() {
                取得 = self.账本里的材料(&材料键);
            }
            let 已记 = 取得.is_some();
            if 取得.is_none()
                && !self.audit.on
                && let (Some(s), Some(m)) = (self.料库.clone(), 原料.as_ref())
            {
                取得 = s.take(&need, m).map(|x| (x, "store"));
            }
            if 取得.is_none()
                && !self.audit.on
                && let (Some(s), Some(m)) = (self.宿主取材料.clone(), 原料.as_ref())
            {
                取得 = s.fetch(&q, &need, m).map(|x| (x, "host"));
            }
            if !已记 && let Some((m, 由 @ ("store" | "host"))) = &取得 {
                self.登记记账(Entry::Opaque {
                    key: 材料键.clone(),
                    world_epoch: 0,
                    decl: jpp_ledger::OpaqueDecl::Opaque,
                    value: json!({"mat": m, "by": 由}),
                });
            }
            let Some((m, 由)) = 取得 else {
                // 止血（复核 Z0398、主控板 Z0502）：审计重放时首跑这一轮取到过材料，本趟取不到——料库或宿主端口取来的
                // 材料还没进账本（Z0494 用 V5 Opaque 事件记），不改走路 C，报错
                self.重放取不到(&cur, &need, round, sp)?;
                self.记补信息(&cur, 原因, &need, round, false, asked_by, sp);
                果.missed.push(need);
                continue;
            };
            self.记补信息(&cur, 原因, &need, round, true, asked_by, sp);
            果.fetched.push(need.clone());
            果.fetched_by.push(由);
            let st2 = if 补进的槽(&cur) == "ref" {
                let mut r: Vec<Value> = st
                    .r#ref
                    .iter()
                    .map(|m| Value::Mat(Rc::new(m.clone())))
                    .collect();
                r.push(Value::Mat(Rc::new(m)));
                self.换槽_参照(&st, r, sp)?
            } else {
                let mut ctx: Vec<Value> = st
                    .ctx
                    .iter()
                    .map(|m| Value::Mat(Rc::new(m.clone())))
                    .collect();
                ctx.push(Value::Mat(Rc::new(m)));
                self.换槽(&st, Some(ctx), None, sp)?
            };
            // 再判：同一道题（来源指回当前键）、补过的材料、同一条线
            let mut q2 = q.clone();
            if !key.is_empty() {
                Rc::make_mut(&mut q2).from_key.insert(key.clone());
            }
            let r2 = self.judge(&st2, &[q2.clone()], sp)?.remove(0);
            let Value::Reading(r2) = r2 else {
                return err(Some("E-rt-arg"), "默认链：再判没有给出读数", sp);
            };
            self.判断来历
                .insert(r2.ledger_key.clone(), (st2.clone(), q2.clone()));
            self.切法来历
                .insert(r2.ledger_key.clone(), (calib.clone(), opts.clone()));
            self.读数表.insert(r2.ledger_key.clone(), r2.clone());
            let to = Some(r2.ledger_key.clone()).filter(|k| !k.is_empty());
            let 原 = if cur.is_unsure() {
                None
            } else {
                Some(原因.unwrap_or("near_boundary"))
            };
            self.记细化(&cur, 原, to, sp);
            if cur.is_unsure() {
                cur.consumed.set(true);
                *cur.consumed_by.borrow_mut() = "default:refine".into();
            }
            self.flush("inspect")?;
            let v = self.cut(&r2, calib.as_deref(), opts, sp)?;
            let Value::Exit(e2) = v else {
                return err(Some("E-rt-arg"), "默认链：再判没有切出出口", sp);
            };
            cur = e2;
            果.补过 = true;
        }
        果.last = cur;
        Ok(果)
    }

    /// 账本里有没有以 `前缀` 开头的取来材料（Z0494）
    fn 账本记过取来材料(&self, 前缀: &str) -> bool {
        self.ledger
            .view()
            .entries
            .iter()
            .any(|e| matches!(e, Entry::Opaque { key, .. } if key.starts_with(前缀)))
    }

    /// 账本里记过的取来材料（Z0494，过程记录 5.24）：同键的 `Opaque` 取 `{mat, by}`
    fn 账本里的材料(&self, 键: &str) -> Option<(Mat, &'static str)> {
        let l = self.ledger.view();
        let v = l.entries.iter().rev().find_map(|e| match e {
            Entry::Opaque { key, value, .. } if key == 键 => Some(value.clone()),
            _ => None,
        })?;
        let m: Mat = serde_json::from_value(v.get("mat")?.clone()).ok()?;
        let 由 = match v.get("by").and_then(|b| b.as_str()) {
            Some("host") => "host",
            _ => "store",
        };
        Some((m, 由))
    }

    /// 止血（复核 Z0398、主控板 Z0502，过程记录 5.23；Z0494 起只在账本里没有对应 `Opaque` 时起作用）：审计重放时首跑这一轮取到过材料（账本里同一责任键、同一类别、
    /// 同一轮次有 `Enrich{got: true}`），本趟却取不到——料库或宿主端口取来的材料还没进账本（Z0494 用 V5 Opaque 记），
    /// 报 `E-replay`，不改走路 C
    fn 重放取不到(&self, cur: &Exit, need: &str, round: u32, sp: Span) -> R<()> {
        if !self.audit.on {
            return Ok(());
        }
        let of = 责任键(cur);
        let 首跑取到 = self.ledger.view().entries.iter().any(|x| {
            matches!(x, Entry::Enrich { of: o, need: n, round: k, got: true, .. } if o == &of && n == need && *k == round)
        });
        if 首跑取到 {
            return err(
                Some("E-replay"),
                format!(
                    "默认链：首跑这里取到了「{need}」类的材料，审计重放取不到。料库或宿主端口取来的材料还没进账本（Z0494），\
                     重放要带同一个料库或宿主端口"
                ),
                sp,
            );
        }
        Ok(())
    }

    /// 被再判取代的出口记 `Refine{how: default}`。未决出口经 `记去向`（登记解除）；已决出口（读数触发）不是责任，
    /// 直接写事件，原因记 `原因`
    fn 记细化(&mut self, e: &Exit, 原因: Option<&str>, to: Option<String>, sp: Span) {
        match 原因 {
            None => self.记去向(e, 去向::Refine { how: "default", to }, "默认链", sp),
            Some(c) => self.写去向(Entry::Refine {
                of: 责任键(e),
                cause: c.into(),
                site: sp.start,
                how: "default".into(),
                to,
            }),
        }
    }

    /// 读数触发（B0492 S2b，草案第三稿改法 3；Z0398 C 步改带宽）：`cut` 出了已决出口、等级是 `Answer`（无线，按判断器
    /// 的回答）、是非题读数离 0.5 的距离大于 0 小于画像中段 δ（`delta.noul.mid`，裁定五十二 (a)），且没开 `--guard`、
    /// 不在链内：有取法时问、取、同线再判，直到读数离边界不小于带宽或候选用完，出口按最后一次读数定；补不了按原出口
    /// （多数块），报告 `exits` 行记 `near_boundary`。画像测了尾段而中段不全报 `E-delta-mid`（裁定四十五）；画像完全
    /// 没有 δ 时不触发，出口带 `delta_unknown`（裁定五十六，过程记录 5.21）。
    pub(crate) fn 读数触发(
        &mut self,
        v: Value,
        r: &Reading,
        calib: Option<&str>,
        opts: &bridge::CutOpts,
        行: usize,
        sp: Span,
    ) -> R<Value> {
        let Value::Exit(e) = &v else { return Ok(v) };
        if e.is_unsure() || e.grade.get() != Some(LineGrade::Answer) || r.op != Op::Test {
            return Ok(v);
        }
        let 画像 = self.calib.profile();
        let Some(band) = 画像.delta_prior(Op::Test) else {
            if 画像.delta_mid_missing() {
                if self.guard || self.链中 > 0 {
                    return Ok(v);
                }
                return err(
                    Some("E-delta-mid"),
                    format!(
                        "@{} 无线是非题的读数要和画像中段 δ 比、看是不是离多数块边界太近（裁定五十二 (a)），而画像只有尾段 δ、\
                         没有完整的中段 δ（delta.<题型>.mid）。尾段 δ 不能代用（裁定四十五）。修法【需接线人】：用覆盖中段读数的材料重测，\
                         写进画像 delta.<题型>.mid（jpp profile check 列缺项）",
                        sp.start
                    ),
                    sp,
                );
            }
            // 画像完全没有 δ（裁定五十六）：不触发、不编 0；出口带 delta_unknown，--guard 下报一次
            e.delta_unknown.set(true);
            if let Some(row) = self.exit_grades.get_mut(行) {
                row["delta_unknown"] = json!(true);
            }
            if self
                .unknown_reported
                .insert(format!("W-delta-unknown\u{1f}answer:{}", r.ledger_key))
            {
                self.线等级告警(format!(
                    "W-delta-unknown: @{} 无线是非题按判断器的回答出了多数块，画像没有 δ（两段都没测或没加载画像）：分不出读数离边界近不近，\
                     不补信息，出口带 delta_unknown，不作放行不可逆 do 的可信合取项（裁定五十六）。修法【需接线人】：给画像测 delta.noul.mid",
                    sp.start
                ));
            }
            return Ok(v);
        };
        let 近 = |me: &Self, r: &Reading| match me.answer_of(r) {
            Some(Answer::Noul(p)) => {
                let d = (p - 0.5).abs();
                d > 0.0 && d < band
            }
            _ => false,
        };
        // 补信息轮内部的 cut 不置位，由外层看最后一次读数定（Z0497）
        if self.链中 > 0 {
            return Ok(v);
        }
        if self.guard {
            // --guard 下不补（与今天同）；读数在带内照实记一位（Z0497，只记录）
            if 近(self, r) {
                e.near_boundary.set(true);
            }
            return Ok(v);
        }
        if !近(self, r) {
            return Ok(v);
        }
        let e = e.clone();
        let key = r.ledger_key.clone();
        self.切法来历
            .insert(key.clone(), (calib.map(String::from), opts.clone()));
        let 果 = self.补轮(&e, Some("near_boundary"), &|me: &Self, x: &Exit, r: &Reading| {
            !x.is_unsure()
                && matches!(me.answer_of(r), Some(Answer::Noul(p)) if { let d = (p - 0.5).abs(); d > 0.0 && d < band })
        }, sp)?;
        if !果.补过 {
            if let Some(row) = self.exit_grades.get_mut(行) {
                row["near_boundary"] = json!(true);
            }
        }
        let end = if 果.补过 {
            "decided"
        } else {
            "near_boundary"
        };
        let last = 果.last.clone();
        // 最后一次读数仍在带内（没补、补不到、补了仍近）就记一位（Z0497）；补到带外不记
        if !last.is_unsure() {
            let k = last.ledger_key.borrow().clone();
            let 仍近 = if 果.补过 {
                self.读数表
                    .get(&k)
                    .cloned()
                    .map(|r2| 近(self, &r2))
                    .unwrap_or(false)
            } else {
                true
            };
            last.near_boundary.set(仍近);
        }
        self.记默认链(&key, &e, sp, &果, end);
        if last.is_unsure() {
            // 补过以后反而落进并列：按默认链的末端处理（判过而拿不准，记账放弃）
            self.记去向(&last, 去向::Drop, "默认链", sp);
            last.consumed.set(true);
            *last.consumed_by.borrow_mut() = "default:drop".into();
        }
        Ok(Value::Exit(last))
    }

    fn 记默认链(&mut self, key: &str, e: &Exit, sp: Span, 果: &轮果, end: &str) {
        let mut row = json!({
            "key": key, "cause": if e.is_unsure() { e.cause() } else { "near_boundary".into() }, "site": sp.start,
            "asked": 果.asked, "fetched": 果.fetched, "missed": 果.missed, "why": 果.why, "end": end,
        });
        if let Some(s) = 果.source {
            row["source"] = json!(s);
        }
        if !果.needed.is_empty() {
            row["needed"] = json!(果.needed);
        }
        if !果.fetched_by.is_empty() {
            row["fetched_by"] = json!(果.fetched_by);
        }
        if let Some(s) = &果.slot {
            row["slot"] = json!(s);
        }
        self.默认链记录.push(row);
    }

    /// 默认链的候选类别，四级（裁定五十一、五十二 (b)）：作者 `unsure_source.need` > 题 `lacks`（取自题式，手写的或
    /// 守则填的）> 通用表（程序顶层的文本列表 `unsure_lacks`，否则序言的）。都没有为空
    pub(crate) fn 候选类别(&self, q: &Question) -> (Vec<String>, &'static str) {
        if let Some((need, _)) = &self.未决来源
            && !need.is_empty()
        {
            return (need.clone(), "unsure_source");
        }
        if !q.lacks.is_empty() {
            return (q.lacks.clone(), "lacks");
        }
        let 程序的 = self
            .顶层环境
            .as_ref()
            .and_then(|e| env_lookup(e, "unsure_lacks"))
            .and_then(|v| crate::companions::文本表(Some(&v)));
        (
            程序的.or_else(|| self.序言类别.clone()).unwrap_or_default(),
            "generic",
        )
    }

    /// 同一状态换 `ctx` 或 `over` 槽（经 `make_state`，与程序里写 `state(…)` 同一条路）
    pub(crate) fn 换槽(
        &mut self,
        st: &State,
        ctx: Option<Vec<Value>>,
        over: Option<Vec<Value>>,
        sp: Span,
    ) -> R<Rc<State>> {
        let mats =
            |ms: &[Mat]| Value::list(ms.iter().map(|m| Value::Mat(Rc::new(m.clone()))).collect());
        let on = mats(&st.on);
        let rec = Value::record(vec![
            (
                "ctx".into(),
                ctx.map(Value::list).unwrap_or_else(|| mats(&st.ctx)),
            ),
            ("ref".into(), mats(&st.r#ref)),
            (
                "over".into(),
                over.map(Value::list).unwrap_or_else(|| mats(&st.over)),
            ),
        ]);
        match self.make_state(&[on, rec], sp)? {
            Value::State(s) => Ok(s),
            _ => err(Some("E-rt-arg"), "默认链：没造出状态", sp),
        }
    }

    /// 同一状态换 `ref` 槽（Z0589：`insufficient:ref` 补进参照槽）
    fn 换槽_参照(&mut self, st: &State, r#ref: Vec<Value>, sp: Span) -> R<Rc<State>> {
        let mats =
            |ms: &[Mat]| Value::list(ms.iter().map(|m| Value::Mat(Rc::new(m.clone()))).collect());
        let rec = Value::record(vec![
            ("ctx".into(), mats(&st.ctx)),
            ("ref".into(), Value::list(r#ref)),
            ("over".into(), mats(&st.over)),
        ]);
        match self.make_state(&[mats(&st.on), rec], sp)? {
            Value::State(s) => Ok(s),
            _ => err(Some("E-rt-arg"), "默认链：没造出状态", sp),
        }
    }

    /// 登记一道题、刷新、切出出口（默认链自己的题，不经惰性出口）
    fn 判一次(
        &mut self,
        st: &Rc<State>,
        q: &Rc<Question>,
        calib: Option<&str>,
        opts: bridge::CutOpts,
        sp: Span,
    ) -> R<Rc<Exit>> {
        let r = self.judge(st, &[q.clone()], sp)?.remove(0);
        let Value::Reading(r) = r else {
            return err(Some("E-rt-arg"), "默认链：没有给出读数", sp);
        };
        self.flush("inspect")?;
        match self.cut(&r, calib, opts, sp)? {
            Value::Exit(e) => Ok(e),
            _ => err(Some("E-rt-arg"), "默认链：没有切出出口", sp),
        }
    }
}
