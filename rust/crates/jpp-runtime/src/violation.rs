//! 违规的单次形态与守卫下推迟的不可逆 `do`（G2，步 35）。
//!
//! 依据：`12` §2.13 R9（欠账与违规：有结论时还欠着的每笔记一条 `Violation`，这次结论为「未决（violation）」、
//! 值照带；一次输入一次返回的程序是它的单次形态）；裁定五十九第 3 条；裁定六十一 B200（`--guard` 下不可逆 `do`
//! 推迟到尝试结算之后：到达执行点只记 `Intent`，无违规才执行，有违规记 `Withheld`）；主控 Z0507、Z0508、Z0519。
//! 预注册 `地基/过程记录/工程-G2-违规单次形态.md`。
//!
//! 欠账记号沿用 `jpp_ledger::DebtMark`（帧种类、主人、过桥种类、第几次、题内容键、原因），摘要只调
//! [`DebtMark::token`]，主语言不另写一份（Z0519）。

use super::*;
use jpp_ledger::{DebtMark, FrameKind, Via};
use std::cell::RefCell;

/// 一笔违规（报告 `violations` 段、CLI 的一行诊断）
#[derive(Clone, Debug)]
pub struct ViolationReport {
    pub mark: DebtMark,
    /// 出口站点
    pub site: Span,
    /// 与今天的 J-05 报文同形（含修法）
    pub message: String,
    /// 记号摘要（[`DebtMark::token`]）
    pub token: String,
    /// 同一判断（同键）或同记号的其余视图（Z0593，B162：同一判断的责任只计一次）；没有合并时为空
    pub also: Vec<ViolationView>,
    /// 这一笔的判断账本键（Z0593 附录一：合并单位；与记号里的题内容键 `key` 并列）；按记号合并的那笔为空
    pub judge_key: Option<String>,
}

/// 并进一笔违规的另一个视图：站点与记号
#[derive(Clone, Debug)]
pub struct ViolationView {
    pub mark: DebtMark,
    pub site: Span,
    pub token: String,
}

fn 记号json(mark: &DebtMark, site: Span, token: &str) -> Json {
    serde_json::json!({
        "frame": mark.frame, "owner": mark.owner, "via": mark.via,
        "nth": mark.nth, "key": mark.key, "cause": mark.cause,
        "site": site.start, "token": token,
    })
}

impl ViolationReport {
    pub fn to_json(&self) -> Json {
        let 带键 = |mut x: Json| {
            if let Some(k) = &self.judge_key {
                x["judge_key"] = Json::from(k.as_str());
            }
            x
        };
        let mut j = 带键(记号json(&self.mark, self.site, &self.token));
        // Z0593：只在有合并时写 `also`
        if !self.also.is_empty() {
            j["also"] = Json::Array(
                self.also
                    .iter()
                    .map(|v| 带键(记号json(&v.mark, v.site, &v.token)))
                    .collect(),
            );
        }
        j
    }
}

/// 段末记下的一笔违规（Z0593）：报告，加上已并进来的出口号、这一笔的判断账本键、报文主干
pub(crate) struct 违规笔 {
    pub report: ViolationReport,
    出口: Vec<usize>,
    /// 这一笔的判断账本键（恰好一个；没有判断键、按记号合并的为 `None`）
    键: Option<String>,
    主干: String,
}

/// 出口建出时记下的记号成分（帧种类、主人、过桥种类、帧内第几次）
#[derive(Clone, Debug)]
pub(crate) struct 记号成分 {
    pub frame: FrameKind,
    pub owner: String,
    pub via: Via,
    pub nth: u32,
}

/// 守卫下推迟的一次不可逆 `do`（B200）
pub(crate) struct 推迟动作 {
    pub id: u64,
    pub name: String,
    pub args: Vec<Value>,
    pub key: String,
    pub 意向键: String,
    pub taint: Taint,
    pub site: Span,
    pub spec: &'static jpp_effects::EffectSpec,
    /// 动作登记的费用（Z0565：还没结算时计入预算的「已用」预留）
    pub cost: f64,
    /// 给程序的「已推迟」值的句柄：结算后填上动作的产出（或扣下的失败值），随返回值交出的那份因此看得到结果
    pub handle: Rc<PendingGen>,
}

impl<'a> Interp<'a> {
    /// 出口建出时记下记号成分（`new_exit_from` 调）：帧 0 是程序顶层（`program`，主人取程序单元身份），其余是函数帧
    /// （`code`，主人取 `<函数名>#<实参哈希>`，G2 附录三）；帧内第几次过桥从 1 起。
    pub(crate) fn 记记号(&mut self, exit_id: usize, 帧: usize) {
        let via = self.当前桥;
        let (frame, owner) = if 帧 == 0 {
            (FrameKind::Program, self.尝试().program)
        } else {
            (
                FrameKind::Code,
                self.frames
                    .get_mut(帧)
                    .map(|f| f.主人.取())
                    .unwrap_or_default(),
            )
        };
        let nth = match self.frames.get_mut(帧) {
            Some(f) => {
                f.过桥 += 1;
                f.过桥
            }
            None => 0,
        };
        self.记号表.insert(
            exit_id,
            记号成分 {
                frame,
                owner,
                via,
                nth,
            },
        );
    }

    /// 一个出口的欠账记号
    pub(crate) fn 记号(&self, e: &Exit) -> DebtMark {
        let c = self.记号表.get(&e.id).cloned().unwrap_or(记号成分 {
            frame: FrameKind::Program,
            owner: self.尝试().program,
            via: Via::Cut,
            nth: 0,
        });
        // 步 36 G3：记号的原因只写成员名（与 jpp-cell 的 `cause.name()` 同口径）
        let cause = match &e.kind {
            ExitKind::Unsure(w) => w.cause.name().to_string(),
            _ => String::new(),
        };
        DebtMark {
            frame: c.frame,
            owner: c.owner,
            via: c.via,
            nth: c.nth,
            key: e.q_hash.clone(),
            cause,
        }
    }

    /// 程序结束时这一笔记违规（取代运行期 J-05）：报文与改前的 J-05 同形，函数返回时丢过的另说明最初所在
    pub(crate) fn 记一笔违规(&mut self, e: &Rc<Exit>) {
        // Z0593（B162、裁定五十七；附录一，主控定）：一笔违规对应恰好一个判断账本键。没人接的出口按它的责任键逐个归：
        // 每个键归进那个判断的一笔（没有就新开），合成出口吸收了几个判断就在各自那一笔里各列一次，不把两个判断连成
        // 一笔，笔数也不取决于结算顺序。没有判断键的出口按记号合并。同记号、同站点的视图不重复列
        let 键 = crate::duty::责任键(e);
        let mark = self.记号(e);
        let token = mark.token();
        let 归处: Vec<Option<String>> = if 键.is_empty() {
            vec![None]
        } else {
            键.into_iter().map(Some).collect()
        };
        for k in 归处 {
            let 找 = self.违规.iter().position(|v| match (&v.键, &k) {
                (Some(a), Some(b)) => a == b,
                (None, None) => v.report.token == token,
                _ => false,
            });
            match 找 {
                Some(i) => {
                    let 笔 = &mut self.违规[i];
                    if 笔.出口.contains(&e.id) {
                        continue;
                    }
                    笔.出口.push(e.id);
                    let 已有 = (笔.report.token == token && 笔.report.site == e.site)
                        || 笔
                            .report
                            .also
                            .iter()
                            .any(|x| x.token == token && x.site == e.site);
                    if !已有 {
                        笔.report.also.push(ViolationView {
                            mark: mark.clone(),
                            site: e.site,
                            token: token.clone(),
                        });
                        笔.report.message = format!(
                            "{}（同一判断另有 {} 处视图没人接，见报告 violations[].also；B162 只记一笔）",
                            笔.主干,
                            笔.report.also.len()
                        );
                    }
                }
                None => {
                    let 丢于 = match self.丢失处.get(&e.id) {
                        Some(f) => {
                            format!("（它在 {f} 返回前就没有去向，随调用者一路交到程序结束）")
                        }
                        None => String::new(),
                    };
                    let message = format!(
                        "程序结束时有未消费的 {}（题 {}）{}。{}",
                        e.label(),
                        头(&e.q_hash, 8),
                        丢于,
                        self.j05_fix(e)
                    );
                    self.违规.push(违规笔 {
                        report: ViolationReport {
                            mark: mark.clone(),
                            site: e.site,
                            message: message.clone(),
                            token: token.clone(),
                            also: vec![],
                            judge_key: k.clone(),
                        },
                        出口: vec![e.id],
                        键: k,
                        主干: message,
                    });
                }
            }
        }
    }

    /// 违规写进账本（每笔一条；审计重放不写）
    pub(crate) fn 写违规账(&mut self) {
        if self.audit.on || self.违规.is_empty() {
            return;
        }
        let attempt = self.尝试();
        let marks: Vec<(DebtMark, Vec<DebtMark>)> = self
            .违规
            .iter()
            .map(|v| {
                (
                    v.report.mark.clone(),
                    v.report.also.iter().map(|x| x.mark.clone()).collect(),
                )
            })
            .collect();
        for (mark, also) in marks {
            self.登记记账(Entry::Violation {
                attempt: attempt.clone(),
                mark,
                also,
            });
        }
    }

    /// 程序结束前预判有没有违规（不改任何状态）：顶层帧里未消费的未决，不在返回值里、也没有同键的去向。
    /// 守卫下推迟的不可逆 `do` 据此决定执行还是扣下（B200）
    pub(crate) fn 预判违规(&self, v: &Value) -> bool {
        let Some(frame) = self.frames.last() else {
            return false;
        };
        let mut in_value = HashSet::new();
        collect_exit_ids(v, &mut in_value);
        let 值键 = crate::duty::值里的键(v);
        frame
            .exits
            .iter()
            .filter(|e| e.is_unsure() && !e.consumed.get())
            .any(|e| !in_value.contains(&e.id) && self.键的去向(e, &值键).is_none())
    }

    /// `--guard` 下到达不可逆 `do`：只写意向（带尝试引用），不执行，给一个「已推迟」的值（B200）
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn 推迟do(
        &mut self,
        spec: &'static jpp_effects::EffectSpec,
        name: &str,
        args: Vec<Value>,
        key: String,
        意向键: String,
        taint: Taint,
        sp: Span,
        cost: f64,
    ) -> R<Value> {
        // B55 按位置判（G2 附录三）：没有意向，或最后一条意向已被扣下（上一趟没执行），这一趟写自己的意向
        let 要写 = {
            let l = self.ledger.view();
            l.get(&意向键).is_none() || l.intent_withheld(&意向键)
        };
        if !self.audit.on && 要写 {
            self.收层()?;
            let at = self.ledger.view().len() as u64;
            let attempt = Some(self.尝试());
            self.即刻记账(
                Entry::Intent {
                    key: 意向键.clone(),
                    at,
                    attempt,
                },
                sp,
            )?;
        }
        let id = self.新生成号();
        let handle = Rc::new(PendingGen {
            id,
            mark: self.next_reading.get(),
            site: sp,
            resolved: RefCell::new(None),
        });
        self.推迟.push(推迟动作 {
            handle: handle.clone(),
            id,
            name: name.to_string(),
            args,
            key,
            意向键,
            taint,
            site: sp,
            spec,
            cost,
        });
        Ok(Value::Gen(handle))
    }

    /// 读到了推迟的不可逆 `do` 的值：同一趟里消费它的结果，报 `E-guard-irreversible-midway`（B200；本步只在运行期报，
    /// 静态检查另登 Z0508）
    pub(crate) fn 推迟被读(&self, g: &PendingGen) -> Option<RtError> {
        let d = self.推迟.iter().find(|d| d.id == g.id)?;
        // 依据：B200（`--guard` 下不能一边动世界一边接着算：先出结论，再由下游程序去动）
        Some(RtError::new(
            Some("E-guard-irreversible-midway"),
            format!(
                "--guard 下不可逆 do「{}」推迟到这次运行有结论之后才执行，同一次运行里不能读它的结果。修法：先出结论（把要做的事写进返回值），再由下游程序去执行这个动作；或不开 --guard",
                d.name
            ),
            d.site,
        ))
    }

    /// 结算推迟的不可逆 `do`（B200）：没有违规，按到达顺序执行、写 `Effect`；有违规，句柄填扣下的失败值，动作留在
    /// 推迟表里，等违规写进账本之后由 [`Interp::扣下推迟`] 写 `Withheld`（G2 附录三：`Violation` 在引用它的
    /// `Withheld` 之前）。审计重放不执行：账本里有 `Effect` 照记录计入，有 `Withheld` 照样不执行。
    /// 逐条从表头取：中途出错时没轮到的仍在表里，出错路径给它们写 `Withheld(error)`
    pub(crate) fn 结算推迟(&mut self, 有违规: bool) -> R<()> {
        if 有违规 && !self.audit.on {
            for d in &self.推迟 {
                *d.handle.resolved.borrow_mut() = Some(扣下值(&d.name));
            }
            return Ok(());
        }
        while !self.推迟.is_empty() {
            // 动作没登记：不取出（没有执行），出错路径照样扣下它
            if !self.audit.on && !self.actions.actions.contains_key(&self.推迟[0].name) {
                let d = &self.推迟[0];
                return Err(Fault::Error(RtError::new(
                    Some("J-11"),
                    format!("动作 {} 未登记", d.name),
                    d.site,
                )));
            }
            let d = self.推迟.remove(0);
            if self.audit.on {
                let 记录 = match self.ledger.view().get(&d.key) {
                    Some(Entry::Effect {
                        cost,
                        output,
                        output_mat,
                        ..
                    }) => Some((*cost, entry_to_effect_value(output, output_mat.as_deref()))),
                    _ => None,
                };
                match 记录 {
                    Some((cost, v)) => {
                        self.audit.usd += cost;
                        self.audit.calls += 1;
                        self.trace
                            .push(d.spec.name, &d.key, true, 0.0, d.site, d.name.clone());
                        self.记结算失败(d.site, &d.name, &v);
                        *d.handle.resolved.borrow_mut() = Some(v);
                    }
                    None => *d.handle.resolved.borrow_mut() = Some(扣下值(&d.name)),
                }
                continue;
            }
            let action = self.actions.actions[&d.name].clone();
            let (cost, mat_shape) = (action.cost, action.mat_shape.clone());
            let result = (action.f)(&d.args).and_then(|v| match &mat_shape {
                Some(shape) => crate::effects_exec::形状核对(shape, &v.to_json()).map(|_| v),
                None => Ok(v),
            });
            let out = crate::effects_exec::动作产出(&d.name, &d.key, &d.args, d.taint, result);
            self.cost.usd += cost;
            self.cost.calls += 1;
            self.记请求(d.spec);
            let (output, output_mat) = effect_value_to_entry(&out);
            let 结果 = Entry::Effect {
                key: d.key.clone(),
                ekey: self.effect_keys.get(&d.key).cloned(),
                output_mat: output_mat.map(Box::new),
                kind: d.spec.name.into(),
                output,
                cost,
                reused_from: None,
                wall: None,
            };
            self.即刻记账(结果, d.site)?;
            self.trace
                .push(d.spec.name, &d.key, false, cost, d.site, d.name.clone());
            self.记结算失败(d.site, &d.name, &out);
            *d.handle.resolved.borrow_mut() = Some(out);
        }
        Ok(())
    }

    /// 结算时才失败的推迟动作（Z0565 口径 ②）：不算 J-12（结论已定，失败值随返回值交出）；账本 `Effect` 照记失败，
    /// 这里列进 `Outcome.settle_failed` 并报 `W-settle-failed`，退出码不变。审计重放照记录同样列出（账本里已有
    /// `Effect`，重放在到达处复用它，由 `effects_exec::do_` 调这里）
    pub(crate) fn 记结算失败(&mut self, site: Span, name: &str, out: &Value) {
        let Value::Fail(m, _) = out else {
            return;
        };
        self.trace.warn(format!(
            "W-settle-failed: @{} --guard 下推迟的不可逆 do「{}」结算时执行失败：{m}。结论已定，失败值随返回值交出（不算 J-12）；账本 Effect 记了这次失败",
            site.start, name
        ));
        self.结算失败.push(serde_json::json!({
            "site": site.start, "action": name, "detail": m.as_ref(),
        }));
    }

    /// 还没结算的推迟动作的预算预留（Z0565 口径 ①）：调用数每条 1 次、花费按动作登记的费用。与登记了还没交出的
    /// 生成同一办法计入 `charge` 的已用；结算执行后改计入 `cost`，扣下时离开推迟表即释放
    pub(crate) fn 推迟预留(&self) -> (u64, f64) {
        (
            self.推迟.len() as u64,
            self.推迟.iter().map(|d| d.cost).sum(),
        )
    }

    /// 推迟表里还没结算的动作逐条写 `Withheld`，不执行（B200；G2 附录三）：有违规（写在 `Violation` 之后）、
    /// 这一趟挂起、这一趟出错。下一趟到达执行点时 B55 认出「最后一条意向之后有 `Withheld`」＝确知没执行，照常再推迟。
    /// 审计重放不写
    pub(crate) fn 扣下推迟(&mut self, cause: jpp_ledger::WithheldCause) -> R<()> {
        let 推迟 = std::mem::take(&mut self.推迟);
        if self.audit.on {
            return Ok(());
        }
        let 因 = match cause {
            jpp_ledger::WithheldCause::Violation => "这次运行有违规",
            jpp_ledger::WithheldCause::Suspended => "这一趟挂起等人回答，续接时照常再推迟",
            jpp_ledger::WithheldCause::Error => "这一趟运行期出错，修好后再跑时照常再推迟",
        };
        for d in 推迟 {
            let attempt = self.尝试();
            self.即刻记账(
                Entry::Withheld {
                    attempt,
                    key: d.意向键.clone(),
                    cause,
                },
                d.site,
            )?;
            self.trace.warn(format!(
                "W-withheld: @{} --guard 下不可逆 do「{}」没有执行：{因}（B200）",
                d.site.start, d.name
            ));
        }
        Ok(())
    }
}

/// 函数帧主人里的实参哈希（G2 附录三，与 C1 代码单元键同口径：实参个数进键，各实参是独立分量）。
/// 惰性的值按身份取、不按此刻是否已取回：未取回的生成取生成号，出口与惰性出口取出口号与题，不取「是否已消费」——
/// 首跑与审计重放在同一次调用时取回的进度可能不同，记号要一致
pub(crate) fn 实参哈希(args: &[Value]) -> String {
    // Z0882：大实参按身份缓存这一分量（值不可变；子树有函数或句柄的不缓存，结果与重算相同）
    let hs: Vec<String> = args
        .iter()
        .map(|a| {
            jpp_value::value::ident_cache::指纹哈希(a, || {
                jpp_ir::key::hash_of(&[&jpp_ir::key::canon(&指纹(a))])
            })
        })
        .collect();
    let n = hs.len().to_string();
    let mut parts: Vec<&str> = vec!["frame/args", &n];
    parts.extend(hs.iter().map(String::as_str));
    jpp_ir::key::hash_of(&parts)
}

fn 指纹(v: &Value) -> Json {
    jpp_value::value::ident_cache::记访问_容器(v);
    match v {
        Value::List(l) => Json::Array(l.iter().map(指纹).collect()),
        Value::Record(r) => {
            let mut m = serde_json::Map::new();
            for (k, x) in r.iter() {
                m.insert(k.clone(), 指纹(x));
            }
            Json::Object(m)
        }
        Value::Gen(g) => serde_json::json!({"gen": g.id}),
        Value::Exit(e) => serde_json::json!({"exit": e.id, "q": e.q_hash}),
        Value::Cut(c) => serde_json::json!({"cut": c.id, "q": c.reading.q_hash}),
        other => other.to_json(),
    }
}

/// 扣下的不可逆 `do` 的值（有违规没执行，B200）：失败值，走 J-12 的 `Unsure(fail)`
fn 扣下值(name: &str) -> Value {
    Value::Fail(
        Rc::from(format!("withheld: do「{name}」因这次运行有违规没有执行（B200）").as_str()),
        jpp_value::prov::Provenance::trusted(),
    )
}
