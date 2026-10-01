//! J-05 的静态面：未决必须被消费。五处：`handle` 的臂表要有能收下责任的 `unsure` 臂；出口或
//! 契约值绑定后必须再被提到；契约值不能在语句位置丢掉；函数直接带出出口时返回类型要提 Exit；
//! 捕获了责任的 `Fn¹` 不能交给高阶操作。运行期那一半（出口 `consumed` 标记，返回前核）在运行时。
//!
//! 步 24a 追加两条独立编号的 warn（不改上面五处 J-05 本身）：
//! - **`W-pending-unreturned`**（B81(c) 补句、B95）：契约值绑定之后被 `exit_binding` 判定为
//!   「用过」（不然已经是 J-05 error），但用法窄到只碰了已决的一侧（`.value`、`accepted()`、
//!   `ignored()`）、从没碰未决的一侧（`.pending`、`undecided()`、`unobserved()`），也没有整体
//!   转交或显式 `consume`——未决清单可能被悄悄漏掉了。
//! - **`W-drop-then-return`**（B95 静态半，主会话 2026-09-25 确认范围）：运行期 `duty.rs::drop_then_return`
//!   （步 21）自己承认的盲点——`handle(B.exit, {…, unsure: fn(u) { consume(u, "drop"); … } })`
//!   的 `unsure` 臂里，若又把同一个 `B` 的 `.index`/`.pos` 投影进返回值，这个数字不带来源，
//!   运行期看不出它对应哪个被 drop 的出口，只能靠语法面直接看「同一个 `B`」。

#![allow(unused_imports)]
use super::{AnalysisId, CallSite, Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-05（未决必须被消费；静态面见 §5 分工表「字面 match 穷尽」）。
pub(crate) const RULE: Rule = Rule {
    code: "J-05",
    requires: &[AnalysisId::Names],
    hooks: Hooks {
        scan_call: Some(fn1_to_higher_order),
        call: Some(handle_arms),
        bind: Some(exit_binding),
        stmt: Some(dropped_outcome),
        function: Some(exit_returned),
        ..Hooks::NONE
    },
};

/// Fnω 才能交给任意长度的高阶操作：捕获了责任的 Fn¹ 不可重复调用、不可丢弃
fn fn1_to_higher_order(cx: &Cx, n: &str, arguments: &[&Expr]) -> Vec<Diagnostic> {
    let names = cx.names.expect("名字趟的钩子点给出名字视图");
    let mut out = vec![];
    for i in method_positions(n) {
        let Some(arg) = arguments.get(i) else {
            continue;
        };
        let (ExprKind::Name(f), span) = (arg.kind(), &arg.span) else {
            continue;
        };
        let Some(t) = names.annotation(f) else {
            continue;
        };
        if matches!(t.as_method(), Some((_, true))) {
            out.push(Diagnostic::error(
                "J-05",
                format!("{f} 的类型是 Fn¹（捕获了未决责任），不能交给 {n}：它可能被调用任意次、也可能一次都不调，责任会被复制或丢掉。修法：把责任用 accumulator 显式传下去，或让这个方法每次调用自己产生并处理责任（Fnω）"),
                *span,
            ));
        }
    }
    out
}

/// unsure 必须有显式一臂（通配兜不住），而且这一臂要收得下未决责任
fn handle_arms(_cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    if s.name != "handle" {
        return out;
    }
    let Some(a) = s.args.get(1) else { return out };
    let ExprKind::Record(fields) = a.kind() else {
        return out;
    };
    match fields.iter().find(|(n, _)| n == "unsure").map(|(_, v)| v) {
        // J-05 默认链（B0492 S2，Z0207 第 6 条）：缺 unsure 臂不再是错，未决走语言的默认链（问缺哪类信息、取来再判，
        // 仍拿不准记账放弃）；检查器只提示，`--guard` 只在运行期恢复错
        None => out.push(Diagnostic::warning(
            "N-unsure-default",
            "handle 的臂表没有 unsure 去向（otherwise 兜不住未决）：这里拿不准时走语言的默认链——问缺哪类信息、从 unsure_source 声明的来源取来再判，仍拿不准就记账放弃（缺席类转交程序结果），逐条进报告 unsure_default 段。要自己定去向，补 unsure: fn(u) {…}；开 --guard 时缺这一臂在运行期是 J-05 错",
            a.span,
        )),
        Some(arm) => match arm.kind() {
            ExprKind::Function(f) if f.parameters.is_empty() => out.push(Diagnostic::error(
                "J-05",
                "unsure 的臂没有参数，接不到未决责任。修法：写成 unsure: fn(u) { … }",
                arm.span,
            )),
            ExprKind::Integer(_) | ExprKind::Decimal | ExprKind::Bool | ExprKind::Text(_) | ExprKind::Unit | ExprKind::List(_) | ExprKind::Record(_) => {
                out.push(Diagnostic::error(
                    "J-05",
                    "unsure 的臂是个字面量，收不下未决责任：进臂不等于销账。修法：写成 unsure: fn(u) { … }，在体内先考虑转交——把 u 放进返回值（如 {…, exit: u}）；或 escalate / literalize；consume(u, \"drop\") 排最后，只用于不进入任何输出、不参与路由的题",
                    arm.span,
                ));
            }
            _ => {}
        },
    }
    // W-drop-then-return 静态半（B95，步 24a）：与上面 unsure 臂存在性检查同一个 handle 调用。
    out.extend(drop_then_return_static(s));
    out.extend(stat_arms(s));
    out
}

/// 按 `cut` 的字面选项取臂（B153 (1)，步 20j-3）。检查器没有按题型核臂的检查（题型要跨绑定追读数，交运行期
/// `duty.rs::handle`）；写了 `stat` 的声明线不同——出口种类由选项定、与读数题型无关：`declare` 带 `cuts` 为 at 型
/// （`at`、`unsure`），带 `hi` 为 test 型（`act`、`ignore`、`unsure`）。静态面只认直接嵌套
/// `handle(cut(…, {stat: 字面量, declare: {hi… | cuts…}}), {臂})`：报这一族走不到的臂，以及没有 `otherwise` 时缺的
/// 必需臂（`unsure` 由上面的检查管）。经 `let` 绑定、函数传递、`stat` 不是字面量的形状不报，运行期兜底（静态报出 ⊆
/// 运行期拒绝）；没写 `stat` 的站点不看，现有程序不受影响。
fn stat_arms(s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    let (Some(exit), Some(arms)) = (s.args.first(), s.args.get(1)) else {
        return out;
    };
    let ExprKind::Call {
        function,
        arguments,
    } = exit.kind()
    else {
        return out;
    };
    if !matches!(function.kind(), ExprKind::Name(n) if n == "cut") {
        return out;
    }
    let Some(rec) = arguments.iter().skip(1).find_map(|a| match a.kind() {
        ExprKind::Record(f) => Some(f),
        _ => None,
    }) else {
        return out;
    };
    let Some((_, stat)) = rec.iter().find(|(k, _)| k == "stat") else {
        return out;
    };
    let 名 = match stat.kind() {
        ExprKind::Text(t) if t != "max" => format!("\"{t}\""),
        ExprKind::Record(f) if f.iter().any(|(k, _)| k == "mass") => "{mass: …}".to_string(),
        _ => return out,
    };
    let Some((_, d)) = rec.iter().find(|(k, _)| k == "declare") else {
        return out;
    };
    let ExprKind::Record(df) = d.kind() else {
        return out;
    };
    let (族, 臂们): (&str, &[&str]) = if df.iter().any(|(k, _)| k == "cuts") {
        ("at", &["at", "unsure"])
    } else if df.iter().any(|(k, _)| k == "hi") {
        ("test", &["act", "ignore", "unsure"])
    } else {
        return out;
    };
    let ExprKind::Record(af) = arms.kind() else {
        return out;
    };
    let has_other = af.iter().any(|(k, _)| k == "otherwise");
    let 走不到: Vec<&str> = af
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| matches!(*k, "act" | "ignore" | "pick" | "at") && !臂们.contains(k))
        .collect();
    let 缺: Vec<&str> = 臂们
        .iter()
        .copied()
        .filter(|k| *k != "unsure" && !has_other && !af.iter().any(|(n, _)| n == k))
        .collect();
    if 走不到.is_empty() && 缺.is_empty() {
        return out;
    }
    let mut 说 = vec![];
    if !走不到.is_empty() {
        说.push(format!("臂 {} 走不到", 走不到.join(", ")));
    }
    if !缺.is_empty() {
        说.push(format!("缺臂 {}", 缺.join(", ")));
    }
    // 依据：B153 (1)（地基/附注/2026-09-26-批6裁定.md §一：出口种类由 cut 站点的字面选项静态可定，J-05 按选项取臂）
    out.push(Diagnostic::error(
        "J-05",
        format!(
            "handle 的臂与出口种类不合：cut 写了 stat: {名} 的声明线，出口是 {族} 型（{}），{}。修法：按出口种类写臂（{{hi, lo}} 出 act / ignore / unsure，cuts 出 at / unsure）",
            臂们.join(" / "),
            说.join("；")
        ),
        arms.span,
    ));
    out
}

/// `handle(B.exit, {…, unsure: fn(u) { …consume(u, "drop")… } })`：`unsure` 臂自己的函数体里
/// 既 `consume(u, "drop")` 又提到同一个裸名字 `B` 的 `.index`/`.pos`——那个数字不带来源，运行期
/// 看不出它对应哪个被丢的出口（`duty.rs::drop_then_return` 函数头注释自认此盲点）。只查裸名字
/// `B`（不追更深的表达式）与 `unsure` 臂自己的函数体（不跨函数、不跨别的 `handle` 调用）。
fn drop_then_return_static(s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    let Some(exit_expr) = s.args.first() else {
        return out;
    };
    let ExprKind::Field { value: base, field } = exit_expr.kind() else {
        return out;
    };
    if field.as_str() != "exit" {
        return out;
    }
    let ExprKind::Name(base_name) = base.kind() else {
        return out;
    };
    let Some(arms) = s.args.get(1) else {
        return out;
    };
    let ExprKind::Record(fields) = arms.kind() else {
        return out;
    };
    let Some((_, unsure_arm)) = fields.iter().find(|(n, _)| n == "unsure") else {
        return out;
    };
    let ExprKind::Function(f) = unsure_arm.kind() else {
        return out;
    };
    let Some(u_param) = f.parameters.first() else {
        return out;
    };
    let u_name = u_param.name.as_str();
    let mut dropped = false;
    let mut idx_ref = false;
    walk_block(&f.body, &mut |e: &Expr| {
        if let ExprKind::Call {
            function,
            arguments,
        } = e.kind()
            && function.name() == Some("consume")
            && matches!(arguments.first().map(|a| a.kind()), Some(ExprKind::Name(n)) if n == u_name)
            && matches!(arguments.get(1).map(|a| a.kind()), Some(ExprKind::Text(t)) if t.as_str() == "drop")
        {
            dropped = true;
        }
        if let ExprKind::Field { value, field } = e.kind()
            && matches!(value.kind(), ExprKind::Name(n) if n == base_name)
            && matches!(field.as_str(), "index" | "pos")
        {
            idx_ref = true;
        }
    });
    if dropped && idx_ref {
        out.push(Diagnostic::warning(
            "W-drop-then-return",
            format!(
                "{u_name} 已 consume(…, \"drop\")，但同一个 {base_name} 的 index/pos 还在这个 unsure 臂里被提到：它所在的元素被投影出去了，责任却丢了——这个投影不带来源，运行期看不出来（`21` 步 24 补运行期的盲点）。修法：转交——投影里保留 exit 字段，或返回 undecided(o) 与 unobserved(o)；这一项确实不进入输出、不参与路由，才 drop 且不返回它"
            ),
            unsure_arm.span,
        ));
    }
    out
}

fn is_outcome_call(e: &Expr) -> bool {
    matches!(
        call_name(e),
        Some("sieve") | Some("pair") | Some("tally") | Some("first_k") | Some("outcome")
    )
}

/// 契约值在语句位置被丢掉
fn dropped_outcome(_cx: &Cx, e: &Expr) -> Vec<Diagnostic> {
    let mut out = vec![];
    // J-05 默认链（B0492 S2c）：语句位置丢掉的 sieve，运行时在该站点当场走默认链，检查器只提示
    if call_name(e) == Some("sieve")
        && crate::unsure_sites::unsure_default_sites(_cx.p).contains(&e.span.start)
    {
        out.push(Diagnostic::warning(
            "N-unsure-default",
            "sieve 的结果（契约值）在语句位置被丢掉：它拿不准的项没有作者给的去向，运行时在这个站点当场走语言的默认链（问缺哪类信息、取来再判，仍拿不准记账放弃）。要自己定去向，绑定它并返回 pending；开 --guard 时这里在运行期是 J-05 错",
            e.span,
        ));
        return out;
    }
    if is_outcome_call(e) {
        out.push(Diagnostic::error(
            "J-05",
            format!("{} 的结果（契约值）在语句位置被丢掉：它的未决清单随包转移，丢掉就是静默丢弃未决（13 §3）。修法：绑定并返回它（或返回 undecided(…) 与 unobserved(…)）、交给下一个构造；契约值不能整份 drop（B95），确实不进入输出的项才逐项丢", call_name(e).unwrap_or("")),
            e.span,
        ));
    }
    out
}

/// 出口绑定之后在本块里再没被提到 = 静默丢弃
fn exit_binding(_cx: &Cx, value: &Expr, name: &str, span: Span, block: &Block) -> Vec<Diagnostic> {
    let mut out = handle_flow(value, name, block);
    // 伴随题（B0492 S5，主控复核 2026-09-30）：程序自己定义的 `unsure_companions` 不是题式列表时，运行时用不上它、
    // 回落到标准库的伴随题；提示作者
    if name == "unsure_companions" {
        let 像题式列表 = match value.kind() {
            ExprKind::List(items) => items
                .iter()
                .all(|x| matches!(x.kind(), ExprKind::Name(_)) || call_name(x) == Some("form")),
            _ => false,
        };
        if !像题式列表 {
            out.push(Diagnostic::warning(
                "N-unsure-companions",
                "unsure_companions 不像题式列表（要写成 [form(…), …] 或题式名的列表）：运行时用不上它，伴随题回落到标准库 lib/unsure.jpp 的题式。修法：改成题式列表，或换个名字",
                span,
            ));
        }
    }
    let is_outcome = is_outcome_call(value);
    if !matches!(call_name(value), Some("cut") | Some("unsure") | Some("ask")) && !is_outcome {
        return out;
    }
    let mut used = false;
    let mut note = |e: &Expr| {
        if let ExprKind::Name(n) = e.kind() {
            if n == name {
                used = true;
            }
        }
    };
    for s in &block.statements {
        match s {
            Statement::Let {
                value: v, name: n, ..
            } if n == name && std::ptr::eq(v, value) => {}
            Statement::Let { value: v, .. } => walk_expr(v, &mut note),
            Statement::Function { function, .. } => walk_block(&function.body, &mut note),
            Statement::Expr(e) => walk_expr(e, &mut note),
        }
    }
    if let Some(r) = &block.result {
        walk_expr(r, &mut note);
    }
    // J-05 默认链（B0492 S2c）：cut 与 sieve 绑定后再没被提到，运行时在该站点当场走默认链（问缺哪类信息、取来再判，
    // 仍拿不准记账放弃），检查器只提示；`--guard` 下运行期照旧是 J-05 错
    // 只有标了「无作者去向」的站点，运行时才当场走默认链；没标的（同一读数另有持有者等）运行期仍是 J-05，
    // 检查器同样报错，与运行期一致（复查 2026-09-30）
    let 走链 = matches!(call_name(value), Some("cut") | Some("sieve"))
        && crate::unsure_sites::unsure_default_sites(_cx.p).contains(&value.span.start);
    // 同一判断直接绑定的一组 cut 里另一处已有作者去向（B162：任一持有者交出，其余视图解除；主控 2026-09-30 路 2）
    let 同组已交 = call_name(value) == Some("cut")
        && crate::unsure_sites::b162_shared_sites(_cx.p).contains(&value.span.start);
    if !used && 同组已交 {
        out.push(Diagnostic::warning(
            "N-duty-shared",
            format!("{name} 绑定之后再没被提到，但它与同一块里另一个 cut 切的是同一个判断，那个视图已有作者给的去向：同一判断的另一视图已有去向，这份不计责任（B162）。运行时这里不走默认链"),
            span,
        ));
    } else if !used && 走链 {
        out.push(Diagnostic::warning(
            "N-unsure-default",
            format!("{name} 绑定之后再没被提到：这里拿不准的项没有作者给的去向，运行时在这个站点当场走语言的默认链——问缺哪类信息、从 unsure_source 声明的来源取来再判，仍拿不准就记账放弃，逐条进报告 unsure_default 段。要自己定去向，handle 它或把它（契约值的 pending）放进返回值；开 --guard 时这里在运行期是 J-05 错"),
            span,
        ));
    } else if !used && is_outcome {
        out.push(Diagnostic::error(
            "J-05",
            format!("契约值 {name} 绑定之后再没被提到：它的未决清单（pending）随包转移给了你，丢掉它就是静默丢弃未决（13 §3）。修法：转交——返回它，或返回 undecided({name}) 与 unobserved({name})（或 {name}.pending），或交给下一个构造；契约值不能整份 drop（B95），确实不进入输出的项才逐项丢"),
            span,
        ));
    } else if !used && call_name(value) == Some("cut") {
        // 裁定五十九第三节（G1 步 33，过程记录 5.25）：读数另有持有者的 cut（别名、先取下标、闭包捕获、同组另一视图只是
        // 被提到过等）静态看不清责任由谁接，不整份拒，报 W-unsure-untracked 交运行期按 B162 记账
        let 读数 = call_args(value)
            .first()
            .map(|x| match x.kind() {
                ExprKind::Name(n) => n.to_string(),
                _ => "（表达式）".to_string(),
            })
            .unwrap_or_default();
        out.push(Diagnostic::warning(
            "W-unsure-untracked",
            format!("出口 {name}（绑定在这里，切的读数是 {读数}）绑定之后再没被提到，而这个读数另有持有者（别名、下标、闭包捕获或同一判断的另一视图）：静态看不清这份未决责任由谁接，交运行期按 B162 记账——同一判断任一视图随值交出即解除，都没人接时运行期照报 J-05（裁定五十九）。要自己定去向，handle({name}, {{…, unsure: …}}) 或把它放进返回值"),
            span,
        ));
    } else if !used {
        out.push(Diagnostic::error(
            "J-05",
            format!("出口 {name} 绑定之后再没被提到：未消费的 unsure 就是静默丢弃。修法：handle({name}, {{…, unsure: …}}) 并在 unsure 臂转交或 escalate，或把 {name} 放进返回值；consume({name}, \"drop\") 排最后，只用于不进入任何输出、不参与路由的题"),
            span,
        ));
    } else if is_outcome {
        // W-pending-unreturned（B81(c) 补句、B95，步 24a）：上面两条错误都没触发（`used` 为真），
        // 但「用过」不等于「转交了未决」——细分引用信号再判一层，见 refs_after_binding。
        let sig = refs_after_binding(value, name, block);
        if sig.窄引用 && !sig.未决感知 && !sig.整体转交 && !sig.显式丢 {
            out.push(Diagnostic::warning(
                "W-pending-unreturned",
                format!("契约值 {name} 的 value/accepted()/ignored() 被用了，但 pending/undecided()/unobserved() 都没被提到、没有整体转交、也没有 consume：未决清单可能被悄悄漏掉了。修法：返回 undecided({name}) 与 unobserved({name})（或 {name}.pending），或整体返回 {name}，或交给下一个构造；确实不进入任何输出、不参与路由才 consume({name}, \"drop\")"),
                span,
            ));
        }
    }
    out
}

/// J-05 默认链的数据流提示（B0492 S4，草案第三稿 (5)）：`let v = handle(x, {…没有 unsure…})` 之后，v 流进 `if` 条件、
/// 效应实参、块结果时各提示一条——拿不准时 v 是未决值（S3 的传播规则）。只看同一块里 v 的直接出现，不追绑定链。
fn handle_flow(value: &Expr, name: &str, block: &Block) -> Vec<Diagnostic> {
    let mut out = vec![];
    if call_name(value) != Some("handle") {
        return out;
    }
    let 缺臂 = call_args(value).get(1).is_some_and(|a| match a.kind() {
        ExprKind::Record(fields) => !fields.iter().any(|(n, _)| n == "unsure"),
        _ => false,
    });
    if !缺臂 {
        return out;
    }
    let mut 看 = |e: &Expr| match e.kind() {
        ExprKind::If { condition, .. } if mentions(condition, name) => {
            out.push(Diagnostic::warning(
                "N-unsure-default",
                format!("{name} 来自没写 unsure 臂的 handle：拿不准时它是未决值，这个 if 两支都不走，整个表达式的值是它。要自己定去向，给那个 handle 写 unsure 臂"),
                condition.span,
            ));
        }
        ExprKind::Call {
            function,
            arguments,
        } if function
            .name()
            .is_some_and(|f| jpp_effects::by_name(f).is_some())
            && arguments.iter().any(|a| mentions(a, name)) =>
        {
            out.push(Diagnostic::warning(
                "N-unsure-default",
                format!("{name} 来自没写 unsure 臂的 handle，这里作 {} 的实参：拿不准时这个效应不发出，账本记 Skip，结果是那个未决值。要自己定去向，给那个 handle 写 unsure 臂", function.name().unwrap_or("")),
                e.span,
            ));
        }
        _ => {}
    };
    let mut 起 = false;
    for s in &block.statements {
        match s {
            Statement::Let { value: v, .. } if std::ptr::eq(v, value) => 起 = true,
            _ if !起 => {}
            Statement::Let { value: v, .. } => walk_expr(v, &mut 看),
            Statement::Function { function, .. } => walk_block(&function.body, &mut 看),
            Statement::Expr(e) => walk_expr(e, &mut 看),
        }
    }
    if let Some(r) = &block.result {
        walk_expr(r, &mut 看);
        if mentions(r, name) {
            out.push(Diagnostic::warning(
                "N-unsure-default",
                format!("{name} 来自没写 unsure 臂的 handle，随块结果交出：拿不准时交出的是未决值（默认链已记账）。要自己定去向，给那个 handle 写 unsure 臂"),
                r.span,
            ));
        }
    }
    out
}

/// 窄引用 / 未决感知 / 整体转交 / 显式丢——四类互斥的引用信号（`W-pending-unreturned` 用，
/// 步 24a）。每次出现按其直接容器归类，不重复计数同一次出现。
#[derive(Default)]
struct 未决信号 {
    窄引用: bool,
    未决感知: bool,
    整体转交: bool,
    显式丢: bool,
}

/// 与 `exit_binding` 的 `used` 判定同一个遍历范围（跳过绑定它自己的那条 `let`），改成细分分类。
fn refs_after_binding(value: &Expr, name: &str, block: &Block) -> 未决信号 {
    let mut sig = 未决信号::default();
    for s in &block.statements {
        match s {
            Statement::Let {
                value: v, name: n, ..
            } if n == name && std::ptr::eq(v, value) => {}
            Statement::Let { value: v, .. } => classify(v, name, &mut sig),
            Statement::Function { function, .. } => classify_block(&function.body, name, &mut sig),
            Statement::Expr(e) => classify(e, name, &mut sig),
        }
    }
    if let Some(r) = &block.result {
        classify(r, name, &mut sig);
    }
    sig
}

fn classify(e: &Expr, name: &str, sig: &mut 未决信号) {
    match e.kind() {
        ExprKind::Name(n) => {
            if n == name {
                sig.整体转交 = true;
            }
        }
        ExprKind::Field { value, field } => {
            if matches!(value.kind(), ExprKind::Name(n) if n == name) {
                match field.as_str() {
                    "value" => sig.窄引用 = true,
                    "pending" => sig.未决感知 = true,
                    _ => sig.整体转交 = true,
                }
            } else {
                classify(value, name, sig);
            }
        }
        ExprKind::Call {
            function,
            arguments,
        } => {
            let mut skip_first = false;
            if let Some(fname) = function.name()
                && matches!(arguments.first().map(|a| a.kind()), Some(ExprKind::Name(n)) if n == name)
            {
                match fname {
                    "accepted" | "ignored" => {
                        sig.窄引用 = true;
                        skip_first = true;
                    }
                    "undecided" | "unobserved" => {
                        sig.未决感知 = true;
                        skip_first = true;
                    }
                    "consume" => {
                        sig.显式丢 = true;
                        skip_first = true;
                    }
                    _ => {}
                }
            }
            if let Some(callee) = function.expr() {
                classify(callee, name, sig);
            }
            for (i, a) in arguments.iter().enumerate() {
                if i == 0 && skip_first {
                    continue;
                }
                classify(a, name, sig);
            }
        }
        ExprKind::List(items) => items.iter().for_each(|x| classify(x, name, sig)),
        ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| classify(x, name, sig)),
        ExprKind::Function(f) => classify_block(&f.body, name, sig),
        ExprKind::Index { value, index } => {
            classify(value, name, sig);
            classify(index, name, sig);
        }
        ExprKind::Unary { value, .. } => classify(value, name, sig),
        ExprKind::Binary { left, right, .. } => {
            classify(left, name, sig);
            classify(right, name, sig);
        }
        ExprKind::If { condition, yes, no } => {
            classify(condition, name, sig);
            classify_block(yes, name, sig);
            classify_block(no, name, sig);
        }
        ExprKind::Block(b) => classify_block(b, name, sig),
        _ => {}
    }
}

fn classify_block(b: &Block, name: &str, sig: &mut 未决信号) {
    for s in &b.statements {
        match s {
            Statement::Let { value, .. } => classify(value, name, sig),
            Statement::Function { function, .. } => classify_block(&function.body, name, sig),
            Statement::Expr(e) => classify(e, name, sig),
        }
    }
    if let Some(r) = &b.result {
        classify(r, name, sig);
    }
}

/// 函数的结果就是出口名字本身，而返回类型没提 Exit
fn exit_returned(_cx: &Cx, f: &Function) -> Vec<Diagnostic> {
    let mut out = vec![];
    let Some(result) = &f.body.result else {
        return out;
    };
    let ExprKind::Name(n) = result.kind() else {
        return out;
    };
    let bound = f.body.statements.iter().any(|s| match s {
        Statement::Let { name, value, .. } => {
            name == n && matches!(call_name(value), Some("cut") | Some("unsure") | Some("ask"))
        }
        _ => false,
    });
    if !bound {
        return out;
    }
    if !f
        .result_type
        .as_ref()
        .map(|t| t.mentions("Exit"))
        .unwrap_or(false)
    {
        out.push(Diagnostic::error(
            "J-05",
            format!("函数把出口 {n} 直接带出，返回类型却没提 Exit：调用者不知道自己要消费它。修法：标注 -> Exit（或含 Exit 的类型），或在函数里 handle / consume 掉"),
            result.span,
        ));
    }
    out
}
