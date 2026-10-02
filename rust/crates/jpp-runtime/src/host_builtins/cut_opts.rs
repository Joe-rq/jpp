//! `cut` 与拟合选项的解析与核对（自由函数）。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改；兄弟模块要用的私有函数改为 `pub(super)`，`super::bridge` 改为 `crate::bridge`）。

use super::*;

/// 策略记录里的数：Int 或 Float（`cut` 的 `declare`/`cost`/`alpha`）
/// `cut` 的 `stat`（B153、B154、B167；步 20j-3）：`"max"`、`"expect"`、`"confidence"`、`"argmax"`（`cut` 另拒，
/// 见 [`核选项`]）或 `{mass: [单元下标…]}`（去重升序）。
pub(crate) fn 解析统计量(v: &Value) -> Result<Stat, String> {
    let 形式 =
        "stat 只认 \"max\" / \"expect\" / \"confidence\" / {mass: [单元下标…]}（B153、B154）";
    match v {
        Value::Text(s, _) => match &**s {
            "max" => Ok(Stat::Max),
            "argmax" => Ok(Stat::Argmax),
            "expect" => Ok(Stat::Expect),
            "confidence" => Ok(Stat::Confidence),
            other => Err(format!("{形式}；收到 \"{other}\"")),
        },
        Value::Record(f) => {
            if f.len() != 1 || f[0].0 != "mass" {
                return Err(format!("{形式}；记录只收 mass 一个字段"));
            }
            let Value::List(l) = &f[0].1 else {
                return Err("mass 要是单元下标的列表：{mass: [0, 1]}".into());
            };
            let mut cells = vec![];
            for x in l.iter() {
                match x {
                    Value::Int(i, _) if *i >= 0 => cells.push(*i as usize),
                    _ => return Err("mass 的单元要是非负整数（候选下标或档位下标）".into()),
                }
            }
            if cells.is_empty() {
                return Err("mass 的单元不能为空".into());
            }
            cells.sort_unstable();
            cells.dedup();
            Ok(Stat::Mass(cells))
        }
        _ => Err(形式.into()),
    }
}

/// `cut` 的 `declare`（B128；`cuts` 为 B153，`closed` 为 B165；步 20j-1、20j-3）：`{hi, lo?, closed?: {hi?, lo?}}`
/// 或 `{cuts: [c₁ < c₂ < …], closed?: {cuts}}`。数的范围随统计量与读数定，在 [`核选项`] 里逐条读数核。
pub(super) fn 解析声明(d: &Value) -> Result<DeclaredLine, String> {
    let 形式 = "declare 要写成 {hi: 数, lo?: 数}：act 当且仅当读数 ≥ hi，ignore 当且仅当读数 ≤ lo，其间 unsure(band)；只给 hi 时 lo = hi（B128）；stat: \"expect\" 另可写 {cuts: [c₁, c₂, …]} 分桶出 at（B153）";
    let Value::Record(f) = d else {
        return Err(形式.into());
    };
    if let Some((k, _)) = f
        .iter()
        .find(|(k, _)| !matches!(k.as_str(), "hi" | "lo" | "cuts" | "closed"))
    {
        return Err(format!(
            "declare 只收 hi、lo、cuts、closed：{{declare: {{hi: 0.7, lo: 0.3}}}}；收到字段 {k}"
        ));
    }
    let mut line = match (d.get("hi"), d.get("cuts")) {
        (Some(_), Some(_)) => return Err("declare 的 hi 与 cuts 只给一个（B153）".into()),
        (None, None) => return Err(形式.into()),
        (None, Some(c)) => {
            if d.get("lo").is_some() {
                return Err("declare 的 cuts 不与 lo 同给：分桶线没有上下侧（B153）".into());
            }
            let cuts: Vec<f64> = match &c {
                Value::List(l) => l.iter().filter_map(数值).collect(),
                _ => vec![],
            };
            let 个数 = match &c {
                Value::List(l) => l.len(),
                _ => 0,
            };
            if cuts.is_empty() || cuts.len() != 个数 || cuts.windows(2).any(|w| w[0] >= w[1]) {
                return Err(
                    "declare 的 cuts 要是严格递增的数列：{cuts: [0.5, 1.5, 2.5]}（B153）".into(),
                );
            }
            DeclaredLine::with_cuts(cuts)
        }
        (Some(h), None) => {
            let Some(hi) = 数值(&h) else {
                return Err(形式.into());
            };
            let lo = match d.get("lo") {
                None => hi,
                Some(v) => 数值(&v).ok_or("declare 的 lo 要是数")?,
            };
            DeclaredLine::two_sided(hi, lo, d.get("lo").is_some())
        }
    };
    if let Some(c) = d.get("closed") {
        // 依据：B165 (2)(3)（地基/附注/2026-09-26-批6裁定.md §十三）
        let Value::Record(cf) = &c else {
            return Err("closed 要写成 {hi: Bool, lo: Bool} 或 {cuts: Bool}（B165）".into());
        };
        for (k, v) in cf.iter() {
            // B176（步 20j-3 追加 (7)）：cuts 线的 closed.cuts 收 Bool 或与 cuts 等长的 [Bool]，逐切点定开闭
            if k == "cuts" && line.is_cuts() {
                line.closed_cuts = match v {
                    Value::Bool(b, ..) => vec![*b; line.cuts.len()],
                    Value::List(xs) => {
                        let bs: Vec<bool> = xs
                            .iter()
                            .filter_map(|x| match x {
                                Value::Bool(b, ..) => Some(*b),
                                _ => None,
                            })
                            .collect();
                        if bs.len() != xs.len() || bs.len() != line.cuts.len() {
                            return Err(format!(
                                "closed.cuts 要是 true / false，或与 cuts 等长（{} 个）的布尔列表，逐切点定开闭（B176）",
                                line.cuts.len()
                            ));
                        }
                        bs
                    }
                    _ => {
                        return Err(
                            "closed.cuts 要是 true / false，或与 cuts 等长的布尔列表（B176）"
                                .into(),
                        );
                    }
                };
                continue;
            }
            let Value::Bool(b, ..) = v else {
                return Err(format!("closed.{k} 要是 true 或 false（B165）"));
            };
            match (k.as_str(), line.is_cuts()) {
                ("hi", false) => line.closed_hi = *b,
                ("lo", false) if line.lo_given => line.closed_lo = *b,
                ("lo", false) => {
                    return Err(
                        "只给 hi 的单线（lo = hi）不收 closed.lo：写出 lo 再定下端开闭（B165 (3)）"
                            .into(),
                    );
                }
                ("cuts", true) => unreachable!("上面已处理"),
                ("cuts", false) => return Err("closed.cuts 只配 cuts 线（B165）".into()),
                ("hi" | "lo", true) => {
                    return Err(
                        "cuts 线的端位写 closed: {cuts: false}，不收 closed.hi / lo（B165）".into(),
                    );
                }
                (other, _) => {
                    return Err(format!(
                        "closed 只收 hi、lo（或 cuts 线的 cuts）；收到字段 {other}（B165）"
                    ));
                }
            }
        }
    }
    Ok(line)
}

/// `cut` 的策略记录（第二或第三位的记录参数）解析成 [`CutOpts`](crate::bridge::CutOpts)：B129 三式 `declare` /
/// `cost` / `alpha` 与 20j-3 的 `stat`。`sieve` 的 `{line: …}` 原样走这里，字段、取值与报错与 `cut` 相同
/// （B128 补齐，过程记录 `地基/过程记录/工程-sieve声明线.md`）。逐读数的搭配另由 [`核选项`] 核。
pub(crate) fn 解析策略(a: &Value, sp: Span) -> R<crate::bridge::CutOpts> {
    // 依据：B128、B129（策略三式都在 cut 上；sieve 的 line 原样走这里）
    let mut opts = crate::bridge::CutOpts::default();
    let 选项错 = |msg: String| -> R<crate::bridge::CutOpts> { err(Some("E-cut-options"), msg, sp) };
    let Value::Record(fields) = a else {
        return err(
            Some("E-rt-arg"),
            format!("cut 的策略参数要是记录，收到 {}", a.type_name()),
            sp,
        );
    };
    if let Some((k, _)) = fields.iter().find(|(k, _)| {
        !matches!(
            k.as_str(),
            "declare" | "cost" | "alpha" | "stat" | "feasible"
        )
    }) {
        return err(
            Some("E-rt-arg"),
            format!(
                "cut 的记录参数只认 declare / cost / alpha / stat / feasible：{{declare: {{hi, lo?}}}}、{{cost: [fp, fn]}}、{{alpha: a}}、{{stat: \"expect\", declare: {{…}}}}、{{feasible: fn(k) {{ 布尔 }}}}；收到字段 {k}"
            ),
            sp,
        );
    }
    if let Some(c) = a.get("cost") {
        let nums: Vec<f64> = match &c {
            Value::List(l) => l.iter().filter_map(数值).collect(),
            _ => vec![],
        };
        // 写成 `!(x > 0.0)` 是有意的：NaN 也要拒，改成 `x <= 0.0` 会放过它
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        let 不合 = nums.len() != 2 || nums.iter().any(|x| !(*x > 0.0));
        if 不合 {
            return err(
                Some("E-rt-arg"),
                "cost 要是两个正数 [fp, fn]：放错一条（假放行）与漏掉一条（假拒绝）的代价",
                sp,
            );
        }
        opts.cost = Some((nums[0], nums[1]));
    }
    if let Some(x) = a.get("alpha") {
        match 数值(&x) {
            Some(v) if v > 0.0 && v < 1.0 => opts.alpha = Some(v),
            _ => {
                return 选项错(
                    "alpha 要是 (0, 1) 之间的数：可接受的假放行率上界，cut 按它在同键证书里选 α ≤ alpha 的一张（B129）".into(),
                );
            }
        }
    }
    if let Some(s) = a.get("stat") {
        // 依据：B153 (1)、B154 (2)（地基/附注/2026-09-26-批6裁定.md §一、§二）
        opts.stat = match 解析统计量(&s) {
            Ok(s) => s,
            Err(m) => return 选项错(m),
        };
    }
    if let Some(f) = a.get("feasible") {
        // 依据：C-4（`规划/骨架候选.md` 2.0.2；world 港城线回报第二节第 2 条）：已决 pick 之后按代码谓词改选
        opts.feasible = match f {
            Value::Fn(c) if c.function.parameters.len() == 1 => Some(c.clone()),
            Value::Fn(c) => {
                return 选项错(format!(
                    "feasible 要是一个参数的函数 fn(k) {{ 布尔 }}（k 是候选在 over 里的下标），收到 {} 个参数",
                    c.function.parameters.len()
                ));
            }
            other => {
                return 选项错(format!(
                    "feasible 要是一个 .jpp 函数 fn(k) {{ 布尔 }}（k 是候选在 over 里的下标），收到 {}",
                    other.type_name()
                ));
            }
        };
    }
    if let Some(d) = a.get("declare") {
        // 依据：B128、B129（地基/附注/2026-09-25-作者主权与策略表达裁定.md §一、§二）
        if opts.cost.is_some() || opts.alpha.is_some() {
            return 选项错(
                "declare 不能与 cost / alpha 同给：声明线没有错误率保证，cost / alpha 无消费者（B129）"
                    .into(),
            );
        }
        opts.declare = match 解析声明(&d) {
            Ok(l) => Some(l),
            Err(m) => return 选项错(m),
        };
    }
    if !opts.stat.is_max() && (opts.cost.is_some() || opts.alpha.is_some()) {
        return 选项错(format!(
            "stat: {} 不收 cost / alpha：二者在 p_max 上的证书里选线，对别的统计量无效；按这个统计量切写 declare（B153）",
            opts.stat.to_json()
        ));
    }
    // B187（批 9 第 3 格）：统计量上没有现成的回答——`mass` 以外的统计量不写线是缺分档参数（形状错）。
    // 放在共用解析里，`cut`、`sieve` 与 `literalize` 的 `{line}` 同一口径
    if opts.declare.is_none() && !opts.stat.is_max() && !matches!(opts.stat, Stat::Mass(_)) {
        return 选项错(format!(
            "cut 的 stat: {} 没有写线：这个统计量上没有现成的回答，要给分档参数——{{stat: {}, declare: {{hi: …, lo: …}}}} 或 {{declare: {{cuts: […]}}}}（B153、B187）",
            opts.stat.to_json(),
            opts.stat.to_json()
        ));
    }
    Ok(opts)
}

/// 逐条读数核 `stat` 与 `declare` 的搭配（B153 (1)、B165 (3)；步 20j-3）：统计量与题型、`mass` 下标、`cuts` 只配
/// `expect`、数的范围（`expect` 为 [0, K−1]，其余 [0, 1]）、K 元 `max` 线不收下侧。违者 `E-cut-options`。
pub(crate) fn 核选项(r: &Reading, opts: &crate::bridge::CutOpts) -> Result<(), String> {
    let 档数 = match r.op {
        Op::Test => 0,
        Op::Select => r.over_len,
        Op::Measure => r.scale.len(),
    };
    match &opts.stat {
        Stat::Max | Stat::Confidence => {}
        Stat::Argmax => {
            return Err(
                "stat: \"argmax\" 是胜出单元的下标，cut 不在它上面划线（order 用，B167）".into(),
            );
        }
        Stat::Mass(cells) => {
            if r.op == Op::Test {
                return Err("stat: {mass: …} 只用于 K 元题（select / measure）：test 读数只有 p 一个统计量（B153）".into());
            }
            if let Some(c) = cells.iter().find(|c| **c >= 档数) {
                return Err(format!(
                    "mass 的单元 {c} 越界：这道题只有 {档数} 个单元（下标 0..{档数}）（B153）"
                ));
            }
        }
        Stat::Expect => {
            if r.op != Op::Measure {
                return Err(format!(
                    "stat: \"expect\" 只用于 measure（有序划分的期望档位）；这道题是 {}（B153）",
                    r.op.phys()
                ));
            }
        }
    }
    // C-4：`feasible` 在已决 pick 之后改选，只有 K 选一（select）、统计量缺省（max）才有 pick 可改
    if opts.feasible.is_some() && (r.op != Op::Select || !opts.stat.is_max()) {
        return Err(format!(
            "feasible 只用于 K 选一（select）读数、统计量缺省（max）的 cut：它在已决的 pick 上按代码谓词取最高概率的可行候选；这道题是 {}，stat {}，没有 pick 可改（C-4）。是非题逐项按代码筛，用 filter 即可",
            r.op.phys(),
            opts.stat.to_json()
        ));
    }
    let Some(l) = &opts.declare else {
        return Ok(());
    };
    if l.is_cuts() && !matches!(opts.stat, Stat::Expect) {
        return Err("declare 的 cuts 只配 stat: \"expect\"（期望档位分桶出 at，B153）".into());
    }
    if opts.stat.is_max() && r.op != Op::Test && l.lo_given {
        return Err(
            "select / measure 的声明线只收 hi：K 元划分没有下侧线，p_max ≥ hi 出 pick / at，否则 unsure(band)（B128、B63）".into(),
        );
    }
    if matches!(opts.stat, Stat::Expect) {
        let 上界 = 档数.saturating_sub(1) as f64;
        let 数们: Vec<f64> = if l.is_cuts() {
            l.cuts.clone()
        } else {
            vec![l.hi, l.lo]
        };
        if 数们.iter().any(|x| !(0.0..=上界).contains(x)) || (!l.is_cuts() && l.lo > l.hi) {
            return Err(format!(
                "stat: \"expect\" 的线要在 [0, {上界}] 之间（这道 measure 有 {档数} 档）且 lo ≤ hi；收到 {}（B153）",
                l.describe()
            ));
        }
    } else if !(0.0..=1.0).contains(&l.hi) || !(0.0..=1.0).contains(&l.lo) || l.lo > l.hi {
        return Err(format!(
            "declare 的两个数要在 [0, 1] 之间且 lo ≤ hi；收到 hi={}、lo={}（B128）",
            l.hi, l.lo
        ));
    }
    Ok(())
}

/// `cut(Score, …)` 的选项（B153 (2)；步 20j-4 预注册第 0 节第 7 条与补记）：只收 `declare`（`{hi, lo?, closed?}`
/// 或 `{cuts, closed?}`）；数的范围不限，只要 `lo ≤ hi`。`stat`、`cost`、`alpha` 报错（`Score` 已是标量、没有证书）。
pub(super) fn 核拟合选项(opts: &crate::bridge::CutOpts) -> Result<(), String> {
    if opts.feasible.is_some() {
        return Err("声明式拟合的结果是一个分，没有候选可选，cut 不收 feasible（C-4）".into());
    }
    if !opts.stat.is_max() {
        return Err("声明式拟合的结果已是一个数，cut 不收 stat（B153）".into());
    }
    if opts.cost.is_some() || opts.alpha.is_some() {
        return Err(
            "声明式拟合没有证书，cut 不收 cost / alpha；按你的数切写 declare（B153）".into(),
        );
    }
    // B187（批 9 第 3 格）：拟合分数不是判断器的回答，不写线是缺分档参数
    if opts.declare.is_none() {
        return Err(
            "声明式拟合的结果是一个分，没有现成的回答：要写线 cut(s, {declare: {hi: …, lo: …}}) 或 {declare: {cuts: […]}}（B153、B187）".into(),
        );
    }
    if let Some(l) = &opts.declare
        && !l.is_cuts()
        && l.lo > l.hi
    {
        return Err(format!(
            "declare 要 lo ≤ hi；收到 hi={}、lo={}（B128）",
            l.hi, l.lo
        ));
    }
    Ok(())
}

/// 声明式拟合的闭包体内不许调用的名字（B153 (2)）：效应、内核构造、出口与责任形式、读答案的刷新点内置。
/// `if`、`loop` 与纯内置放行（步 20j-4 预注册第 0 节第 4 条）。
pub(super) fn 拟合内禁(name: &str) -> bool {
    jpp_effects::by_name(name).is_some()
        || crate::caps::construct(name).is_some()
        || matches!(
            name,
            "state"
                | "cut"
                | "handle"
                | "consume"
                | "escalate"
                | "literalize"
                | "unsure"
                | "pending"
                | "content"
                | "mat"
        )
}

pub(super) fn 数值(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i, _) => Some(*i as f64),
        Value::Float(f, _) => Some(*f),
        _ => None,
    }
}
