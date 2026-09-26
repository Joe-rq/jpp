//! `fit` 桥。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use crate::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_fit(
        &mut self,
        caps: &Caps,
        name: &'static str,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let n = args.len();
        let arity = |k: usize| -> R<()> {
            if n == k {
                Ok(())
            } else {
                err(
                    Some("E-rt-arity"),
                    format!("{name} 需要 {k} 个参数，收到 {n}"),
                    sp,
                )
            }
        };
        // 声明式拟合（B153 (2)，步 20j-4）：第一个实参是带 `declare` 的记录
        if matches!(&args.first(), Some(Value::Record(fs)) if fs.iter().any(|(k, _)| k == "declare"))
        {
            return self.b_fit_declare(caps, args, sp);
        }
        arity(2)?;
        self.flush("fit")?;
        let Value::Text(name, _) = &args[0] else {
            return err(Some("J-16"), "fit(名字: Text, [读数…])", sp);
        };
        let rs = self.readings_of(&args[1], "fit", sp)?;
        // 依据：12 §3 J-16（fit 只认注册表签名，读数个数与特征数一致）
        let Some(rec) = self.fits.get(name.as_ref()).cloned() else {
            return err(
                Some("J-16"),
                format!(
                    "fit {name} 未注册：fit 只认注册表签名（12:274）。修法：用训练过程注册，或改用 cut"
                ),
                sp,
            );
        };
        if rs.len() != rec.features.len() {
            return err(
                Some("J-16"),
                format!(
                    "fit {name} 期望 {} 个读数，收到 {}",
                    rec.features.len(),
                    rs.len()
                ),
                sp,
            );
        }
        // J-04：输入指纹必须与注册特征**逐项相同**
        for (r, (ck, fk)) in rs.iter().zip(&rec.features) {
            if &r.calib != ck || r.op.phys() != fk {
                return err(
                    Some("J-04"),
                    format!(
                        "fit {name} 的输入指纹（{}, {}）与注册特征（{ck}, {fk}）不同：跨题读数不可比，喂错题就是错",
                        r.calib,
                        r.op.phys()
                    ),
                    sp,
                );
            }
        }
        let ps: Vec<f64> = {
            let ans = &*caps.read_answer().answers(self);
            rs.iter()
                .map(|r| rank_value(ans, r).unwrap_or(0.0))
                .collect()
        };
        let score = (rec.f)(&ps).clamp(0.0, 1.0);
        // 结果是读数：calib 位先留 fit 的名字，真正的线由 cut 的第二参给
        let r = Rc::new(Reading {
            q_hash: format!("fit:{name}"),
            state_hash: rs.first().map(|r| r.state_hash.clone()).unwrap_or_default(),
            op: Op::Test,
            calib: format!("fit:{name}"),
            id: caps.issue_reading().new_reading_id(self),
            fail: None,
            model_id: self.model_id.clone(),
            ledger_key: format!("fit:{name}"),
            over_len: 0,
            scale: vec![],
            perms: std::cell::Cell::new(0),
            mode_share: std::cell::Cell::new(None),
            missing_evidence: vec![],
            state_taint: rs
                .iter()
                .fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint)),
            form_hash: None,
            fp: None,
        });
        // 写答案只经 fill_answer（fit 的结果是一个已答的读数）
        caps.issue_reading()
            .fill_answer(self, &r, Answer::Noul(score));
        Ok(Value::Reading(r))
    }

    /// 声明式拟合 `fit({declare: f, tie?: ε}, rs: [Reading], extra: [Value] = [])`（B153 (2)，步 20j-4）：
    /// 按位给闭包每条读数的统计量记录（经 `stat_of`，B167）与 `extra` 的值，闭包在桥内求值（只见参数与内置），
    /// 结果是不透明的 `Score`。输入读数不可用（J-09 证据不足、Fail、缺席）时不求值，`Score` 带未决原因。
    /// 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一）；`12` §2.9 B153 段
    pub(crate) fn b_fit_declare(&mut self, caps: &Caps, args: Vec<Value>, sp: Span) -> R<Value> {
        let 形式 = "fit({declare: fn(记录…, extra…) { 数 }, tie?: 数}, [读数…], [extra…]?)";
        if !(2..=3).contains(&args.len()) {
            return err(
                Some("E-rt-arity"),
                format!("声明式拟合要 2 或 3 个参数：{形式}"),
                sp,
            );
        }
        let Value::Record(fs) = &args[0] else {
            unreachable!("分派处已核")
        };
        if let Some((k, _)) = fs
            .iter()
            .find(|(k, _)| !matches!(k.as_str(), "declare" | "tie"))
        {
            return err(
                Some("E-rt-arg"),
                format!("声明式拟合的记录只收 declare 与 tie；收到字段 {k}。{形式}"),
                sp,
            );
        }
        let Some(Value::Fn(闭包)) = args[0].get("declare") else {
            return err(
                Some("E-rt-arg"),
                format!("declare 要是一个 .jpp 函数：{形式}"),
                sp,
            );
        };
        let tie = match args[0].get("tie") {
            None => 0.0,
            Some(Value::Int(i, _)) if i >= 0 => i as f64,
            Some(Value::Float(f, _)) if f.is_finite() && f >= 0.0 => f,
            Some(_) => {
                return err(
                    Some("E-rt-arg"),
                    "tie 要是非负数：order 按它并档（相邻差 ≤ tie 同档），缺省 0",
                    sp,
                );
            }
        };
        self.flush("fit")?;
        let rs = self.readings_of(&args[1], "fit", sp)?;
        if rs.is_empty() {
            return err(Some("E-rt-arg"), "声明式拟合至少要一条读数", sp);
        }
        let extra: Vec<Value> = match args.get(2) {
            None => vec![],
            Some(Value::List(l)) => l.iter().cloned().collect(),
            Some(other) => {
                return err(
                    Some("E-rt-arg"),
                    format!("extra 要是列表，收到 {}", other.type_name()),
                    sp,
                );
            }
        };
        // 依据：B153 (2)、推翻条件 (2)（extra 只收数据，闭包必然是纯计算；步 20j-4 预注册第 0 节第 5 条）
        for x in &extra {
            if let Some(t) = 非数据(x) {
                return err(
                    Some("E-rt-arg"),
                    format!(
                        "extra 只收数据（数、文本、布尔、unit 及其列表与记录），收到 {t}：读数、出口、材料等要先各自 cut 或 content 成数据再传入"
                    ),
                    sp,
                );
            }
        }
        let taint = rs
            .iter()
            .fold(Taint::Trusted, |t, r| Taint::join(t, r.state_taint));
        let taint = extra
            .iter()
            .fold(taint, |t, x| Taint::join(t, x.prov().taint));
        let inputs: Vec<String> = rs.iter().map(|r| r.calib.clone()).collect();
        let mut 哈希项: Vec<&str> = vec!["fit-declare", 闭包.hash.as_str()];
        哈希项.extend(inputs.iter().map(|s| s.as_str()));
        let fit_hash = hash_of(&哈希项);
        let 状态: Vec<&str> = rs.iter().map(|r| r.state_hash.as_str()).collect();
        let state_hash = hash_of(&[&["fit-states"][..], &状态[..]].concat());
        // 输入不可用：判序与读数的 cut 同（J-09 证据不足 → Fail → 缺席），取第一条
        let fail = rs.iter().find_map(|r| {
            if let Some(m) = r.missing_evidence.first() {
                Some(format!("insufficient:{m}"))
            } else if let Some(f) = &r.fail {
                Some(format!("fail:{f}"))
            } else {
                caps.read_answer().absent_of(self, r)
            }
        });
        let value = match &fail {
            Some(_) => None,
            None => {
                let mut 实参 = vec![];
                for r in &rs {
                    let a = caps
                        .read_answer()
                        .answer_of(self, r)
                        .expect("刷新之后答案必然在");
                    let c = self.置信(r, &a);
                    实参.push(统计量记录(&a, c, r.state_taint));
                }
                实参.extend(extra.iter().cloned());
                Some(self.拟合闭包求值(&闭包, 实参, sp)?)
            }
        };
        let s = caps.issue_reading().issue_score(
            self,
            Score {
                id: 0,
                fit_hash,
                inputs,
                input_keys: rs.iter().map(|r| r.ledger_key.clone()).collect(),
                taint,
                tie,
                state_hash,
                fail,
                site: sp,
            },
            value,
        );
        Ok(Value::Score(s))
    }
}

/// 一条读数交给声明式拟合闭包的统计量记录（B153 (2)；数只经 `stat_of` 取，B167）：`test` 为 `{p}`；
/// `select` 为 `{probs, max, argmax}`；`measure` 另有 `expect`。判断器报了自报置信度时另有 `confidence`（B154）。
/// 叶子带读数所在状态的 taint。
fn 统计量记录(a: &Answer, confidence: Option<f64>, t: Taint) -> Value {
    use jpp_value::stat::{Stat, stat_of};
    let 数 = |x: f64| Value::Float(x, t.into());
    let 取 = |s: &Stat| stat_of(a, s, None).unwrap_or(f64::NAN);
    let mut fs: Vec<(String, Value)> = vec![];
    match a {
        Answer::Noul(_) => fs.push(("p".into(), 数(取(&Stat::Max)))),
        Answer::Choice(v) | Answer::Score(v) => {
            fs.push((
                "probs".into(),
                Value::list(v.iter().map(|x| 数(*x)).collect()),
            ));
            fs.push(("max".into(), 数(取(&Stat::Max))));
            fs.push((
                "argmax".into(),
                Value::Int(取(&Stat::Argmax) as i64, t.into()),
            ));
            if matches!(a, Answer::Score(_)) {
                fs.push(("expect".into(), 数(取(&Stat::Expect))));
            }
        }
    }
    if let Some(c) = confidence {
        fs.push(("confidence".into(), 数(c)));
    }
    Value::Record(Rc::new(fs))
}

/// `extra` 里不是数据的第一个值的类型名（句柄一律不收）
fn 非数据(v: &Value) -> Option<&'static str> {
    match v {
        Value::Unit | Value::Int(..) | Value::Float(..) | Value::Bool(..) | Value::Text(..) => None,
        Value::List(l) => l.iter().find_map(非数据),
        Value::Record(fs) => fs.iter().find_map(|(_, x)| 非数据(x)),
        other => Some(other.type_name()),
    }
}

/// 读数的 p（`test`）或 p_max（K 元题），`fit` 的特征值；`None` = 失败或没答（J-12）
fn rank_value(ans: &dyn Answers, r: &Reading) -> Option<f64> {
    if r.fail.is_some() {
        return None;
    }
    // 步 15k（B167 (1)）：统计量全仓只在 `stat_of` 算。`fit` 要的是特征概率（p、p_max），取 `max`，
    // 值与改前逐位相同；`order` 的排序键不再经这里（`bridge.rs::order_tiers`）
    jpp_value::stat::stat_of(&ans.answer_of(r)?, &jpp_value::stat::Stat::Max, None).ok()
}
