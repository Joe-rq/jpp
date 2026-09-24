//! J-12 的静态面（原 `check.rs`，只搬不改）。

#![allow(unused_imports)]
use super::{Cx, Hooks, Rule};
use crate::*;

/// 依据：12 §5 J-12（程序边界不含 ⊎ Fail 时必须 on_fail 处理）。
pub(crate) const RULE: Rule = Rule {
    code: "J-12",
    requires: &[],
    hooks: Hooks {
        after: Some(run),
        ..Hooks::NONE
    },
};

fn run(cx: &Cx) -> Vec<Diagnostic> {
    fail_at_boundary(cx.p)
}

// ---------------------------------------------------------------- J-12 的静态面

/// `12`:263 J-12：「程序边界不含 `⊎ Fail` 时**必须 `on_fail` 处理**」（栏位：静态）。
///
/// 拦的是：一个 `Fail` **无声地成为程序的结果**。调用者拿到 `{"fail": "…"}` 这样一个记录，
/// 而没有任何地方说过这是失败——**它长得像数据**。与 J-05「未决必须被消费」是同一条纪律的
/// 两面：未决会被拦，失败不会。
///
/// 判得住的只有确定的那一档（宁可漏报不误报）：绑定直接来自 `do` / `gen` / `fail`，
/// 且这个名字**从没被 `is_fail` 查过**，却出现在程序的返回值里。
fn fail_at_boundary(p: &Program) -> Vec<Diagnostic> {
    let mut out = vec![];
    // 哪些绑定直接来自会产生 Fail 的效应
    let mut from_fail: HashMap<String, Span> = HashMap::new();
    for st in &p.body.statements {
        let Statement::Let {
            name, value, span, ..
        } = st
        else {
            continue;
        };
        if matches!(call_name(value), Some("do") | Some("gen") | Some("fail")) {
            from_fail.insert(name.clone(), *span);
        }
    }
    if from_fail.is_empty() {
        return out;
    }
    // 被 is_fail 查过的名字：作者处理过了
    let mut checked: HashSet<String> = HashSet::new();
    walk_block(&p.body, &mut |e| {
        let ExprKind::Call {
            function,
            arguments,
        } = e.kind()
        else {
            return;
        };
        if call_name(e) != Some("is_fail") {
            let _ = function;
            return;
        }
        if let Some(ExprKind::Name(n)) = arguments.first().map(|a| a.kind()) {
            checked.insert(n.to_string());
        }
    });
    // 出现在程序返回值里、**且是裸着出现**的名字。
    //
    // 「裸着」是关键：`content(input)` 不算——`content` 收到 Fail 会当场报错，
    // 失败在那里就暴露了，不会长得像数据。这条检查只拦**把 Fail 原样交出去**：
    // 那时调用者拿到的是 `{fail: …}` 一个记录，没有任何地方说过它是失败。
    //
    // 实测这一条拦下过 `examples/lifecycle.jpp` 的 `content(input)`——那是误报，
    // 所以才有这个区分。误伤会处理失败的程序，和放过无声失败一样是错。
    let Some(result) = &p.body.result else {
        return out;
    };
    let mut in_result: HashSet<String> = HashSet::new();
    collect_bare_names(result, &mut in_result);

    let mut bad: Vec<(String, Span)> = from_fail
        .into_iter()
        .filter(|(n, _)| in_result.contains(n) && !checked.contains(n))
        .collect();
    bad.sort_by_key(|(_, sp)| sp.start);
    for (name, span) in bad {
        out.push(Diagnostic::error(
                "J-12",
                format!(
                    "{name} 可能是 Fail，却直接成了程序的结果：失败会长得像普通记录（`{{fail: …}}`），调用者看不出这是失败。修法：用 `is_fail({name})` 查一下再决定怎么办，或在程序里处理掉它"
                ),
                span,
            ));
    }
    out
}
