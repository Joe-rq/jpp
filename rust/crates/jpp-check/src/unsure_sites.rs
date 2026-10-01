//! 「无作者去向」站点（主控板 B0492 S4，供 S2c 用；主控 2026-09-29 定 S2c 读法 B）。
//!
//! 一个 `cut` 或出契约值的构造，在它的作用域里**一定**没有作者给的未决去向时，标出它的调用起点。运行时在这些站点
//! 切出未决时当场走 J-05 默认链（问缺哪类信息 → 取来 → 再判），下游拿到的是再判后的出口。
//! 标的是保守的子集：只看同一块里 `let` 绑定之后的出口（与 `rules/j05.rs::exit_binding` 同一作用域），
//! 任何可能接走责任的出现（交给 handle / consume / escalate / literalize / 构造 / 函数、放进容器、作结果）都不标。
//! 语句位置直接丢掉的 `cut` 与契约值也标。依据：J-05 草案第三稿 (1)「作者没给去向」、(5)。

#![allow(unused_imports)]
use crate::*;

/// 读法内置：只读出口的一个方面，不接走责任
const 读法: &[&str] = &[
    "exit_kind",
    "unsure_cause",
    "taint",
    "untested",
    "near_boundary",
    "line_source",
    "cert",
    "key_of",
];

/// 程序里「无作者去向」的 `cut` 与出契约值构造的调用起点（源码偏移，与运行时内置调用的站点同一口径）
pub fn unsure_default_sites(program: &Program) -> BTreeSet<usize> {
    站点表(program).标
}

/// 同一判断直接绑定的一组 `cut` 里，另一处已有作者去向、自己没被提到的那几处的调用起点（B162：任一持有者交出，
/// 其余视图解除；主控 2026-09-30 路 2，过程记录 5.18）。它们不标站点，检查器也不报 J-05，改报 `N-duty-shared`
pub fn b162_shared_sites(program: &Program) -> BTreeSet<usize> {
    站点表(program).同组
}

#[derive(Default)]
struct 站点 {
    标: BTreeSet<usize>,
    同组: BTreeSet<usize>,
}

fn 站点表(program: &Program) -> 站点 {
    let mut out = 站点::default();
    块(&program.body, &program.body, &mut out);
    out
}

/// `cut` 的读数实参从哪来（B162 按账本键计持有者）
enum 持有 {
    /// 没有别的持有者，或同组都没有作者去向：可以标
    可标,
    /// 同组另一处已有作者去向：这份不计责任
    同组已交,
    /// 看不清来处，按另有持有者计：不标
    另有,
}

fn 是契约值(e: &Expr) -> bool {
    matches!(
        call_name(e),
        Some("sieve") | Some("pair") | Some("tally") | Some("first_k") | Some("outcome")
    )
}

/// 同一责任可能另有持有者（B162 按账本键计）。认定没有别的持有者的写法（复查 2026-09-30，p5–p7：只数名字
/// 出现几次不够，还要追名字从哪来）：
/// - `cut(judge(…))` 就地切新登记的读数；
/// - `cut(r)`，`r` 在整个程序里只有一处绑定、就是 `let r = judge(…)`，不是任何函数的参数，且 `r` 全程序只出现这一次。
///
/// 同组（二次复查 p1，过程记录 5.18）：`r` 满足上面的绑定条件、出现不止一次，而每次出现都是绑定所在同一块里某条
/// `let x = cut(r)` 的值或这一块的结果 `cut(r)`——组里有一处有作者去向，其余是 [`持有::同组已交`]；都没有，都可标。
/// 其余一律按有别的持有者计（函数参数传进来的、`let r2 = r` 改名的、`let r = rs[0]` 取出的、闭包捕获的都算）
fn 分类(value: &Expr, b: &Block, root: &Block) -> 持有 {
    let Some(first) = call_args(value).first().copied() else {
        return 持有::可标;
    };
    let ExprKind::Name(r) = first.kind() else {
        return if call_name(first) == Some("judge") {
            持有::可标
        } else {
            持有::另有
        };
    };
    let mut 出现 = 0;
    walk_block(root, &mut |x| {
        if matches!(x.kind(), ExprKind::Name(m) if m == r) {
            出现 += 1;
        }
    });
    let (mut 绑定, mut 绑定是判断, mut 是参数) = (0, true, false);
    绑定来历(root, r, &mut 绑定, &mut 绑定是判断, &mut 是参数);
    if !(绑定 == 1 && 绑定是判断 && !是参数) {
        return 持有::另有;
    }
    if 出现 == 1 {
        return 持有::可标;
    }
    // 同组：绑定在本块，每次出现都是本块某条 `let x = cut(r)` 的值或块结果 `cut(r)`
    let 是本组 = |e: &Expr| {
        call_name(e) == Some("cut")
            && call_args(e).len() == 1
            && matches!(call_args(e)[0].kind(), ExprKind::Name(m) if m == r)
    };
    if !b
        .statements
        .iter()
        .any(|s| matches!(s, Statement::Let { name, .. } if name == r))
    {
        return 持有::另有;
    }
    let mut 成员 = 0;
    let (mut 有去向, mut 确定) = (false, false);
    for (i, s) in b.statements.iter().enumerate() {
        if let Statement::Let { name, value: v, .. } = s
            && 是本组(v)
        {
            成员 += 1;
            有去向 |= !没有去向(name, true, i, b);
            确定 |= 确定去向(name, i, b);
        }
    }
    if let Some(e) = &b.result
        && 是本组(e)
    {
        成员 += 1;
        有去向 = true;
        确定 = true;
    }
    // 同组豁免只认确定的去向（G1 步 33，Z0448 q1–q4，过程记录 5.25）：另一视图只是被提到过（放进丢掉的列表、print、
    // 不执行的分支、交给函数），静态看不清它有没有交出，这一组按另有持有者处理
    match (成员 == 出现, 确定, 有去向) {
        (false, _, _) => 持有::另有,
        (true, true, _) => 持有::同组已交,
        (true, false, true) => 持有::另有,
        (true, false, false) => 持有::可标,
    }
}

/// 名字 `r` 在整个块（含嵌套块、函数体）里的绑定：`let` 几处、是否都是 `judge(…)`、是否是某个函数的参数
fn 绑定来历(
    b: &Block, r: &str, 绑定: &mut usize, 绑定是判断: &mut bool, 是参数: &mut bool
) {
    fn 函数(
        f: &Function, r: &str, 绑定: &mut usize, 绑定是判断: &mut bool, 是参数: &mut bool
    ) {
        if f.parameters.iter().any(|p| p.name == r) {
            *是参数 = true;
        }
        绑定来历(&f.body, r, 绑定, 绑定是判断, 是参数);
    }
    fn 表达式(
        e: &Expr, r: &str, 绑定: &mut usize, 绑定是判断: &mut bool, 是参数: &mut bool
    ) {
        walk_expr(e, &mut |x| {
            if let ExprKind::Function(f) = x.kind()
                && f.parameters.iter().any(|p| p.name == r)
            {
                *是参数 = true;
            }
        });
        let _ = (绑定, 绑定是判断);
    }
    for s in &b.statements {
        match s {
            Statement::Let { name, value, .. } => {
                if name == r {
                    *绑定 += 1;
                    if call_name(value) != Some("judge") {
                        *绑定是判断 = false;
                    }
                }
                表达式(value, r, 绑定, 绑定是判断, 是参数);
                嵌套(value, r, 绑定, 绑定是判断, 是参数);
            }
            Statement::Function { function, .. } => 函数(function, r, 绑定, 绑定是判断, 是参数),
            Statement::Expr(e) => {
                表达式(e, r, 绑定, 绑定是判断, 是参数);
                嵌套(e, r, 绑定, 绑定是判断, 是参数);
            }
        }
    }
    if let Some(e) = &b.result {
        表达式(e, r, 绑定, 绑定是判断, 是参数);
        嵌套(e, r, 绑定, 绑定是判断, 是参数);
    }
}

/// 表达式里嵌着的块（函数体、if 两支、块表达式）里的 `let` 绑定也要数
fn 嵌套(e: &Expr, r: &str, 绑定: &mut usize, 绑定是判断: &mut bool, 是参数: &mut bool) {
    let mut 块们: Vec<&Block> = vec![];
    fn 收<'e>(e: &'e Expr, out: &mut Vec<&'e Block>) {
        match e.kind() {
            ExprKind::Function(f) => out.push(&f.body),
            ExprKind::Block(b) => out.push(b),
            ExprKind::If { condition, yes, no } => {
                收(condition, out);
                out.push(yes);
                out.push(no);
            }
            ExprKind::List(items) => items.iter().for_each(|x| 收(x, out)),
            ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| 收(x, out)),
            ExprKind::Call {
                function,
                arguments,
            } => {
                if let Some(c) = function.expr() {
                    收(c, out);
                }
                arguments.iter().for_each(|x| 收(x, out));
            }
            ExprKind::Field { value, .. } | ExprKind::Unary { value, .. } => 收(value, out),
            ExprKind::Index { value, index } => {
                收(value, out);
                收(index, out);
            }
            ExprKind::Binary { left, right, .. } => {
                收(left, out);
                收(right, out);
            }
            _ => {}
        }
    }
    收(e, &mut 块们);
    for b in 块们 {
        绑定来历(b, r, 绑定, 绑定是判断, 是参数);
    }
}

fn 块(b: &Block, root: &Block, out: &mut 站点) {
    for (i, s) in b.statements.iter().enumerate() {
        match s {
            Statement::Let { name, value, .. } => {
                let 出口 = call_name(value) == Some("cut");
                if (出口 || 是契约值(value)) && 没有去向(name, 出口, i, b) {
                    match if 出口 {
                        分类(value, b, root)
                    } else {
                        持有::可标
                    } {
                        持有::可标 => {
                            out.标.insert(value.span.start);
                        }
                        持有::同组已交 => {
                            out.同组.insert(value.span.start);
                        }
                        持有::另有 => {}
                    }
                }
                表达式(value, root, out);
            }
            Statement::Function { function, .. } => 块(&function.body, root, out),
            Statement::Expr(e) => {
                // 语句位置的裸 `cut(r)`：只有 `r` 独有（全程序只出现这一次）才标；它不算同组成员
                let 裸独有 = call_name(e) == Some("cut")
                    && match call_args(e).first().map(|x| x.kind()) {
                        Some(ExprKind::Name(r)) => {
                            let mut n = 0;
                            walk_block(root, &mut |x| {
                                if matches!(x.kind(), ExprKind::Name(m) if m == r) {
                                    n += 1;
                                }
                            });
                            n == 1 && matches!(分类(e, b, root), 持有::可标)
                        }
                        _ => matches!(分类(e, b, root), 持有::可标),
                    };
                if 裸独有 || 是契约值(e) {
                    out.标.insert(e.span.start);
                }
                表达式(e, root, out);
            }
        }
    }
    if let Some(r) = &b.result {
        表达式(r, root, out);
    }
}

/// 找表达式里嵌着的块（函数体、if 两支、块表达式）
fn 表达式(e: &Expr, root: &Block, out: &mut 站点) {
    match e.kind() {
        ExprKind::Function(f) => 块(&f.body, root, out),
        ExprKind::Block(b) => 块(b, root, out),
        ExprKind::If { condition, yes, no } => {
            表达式(condition, root, out);
            块(yes, root, out);
            块(no, root, out);
        }
        ExprKind::List(items) => items.iter().for_each(|x| 表达式(x, root, out)),
        ExprKind::Record(fields) => fields.iter().for_each(|(_, x)| 表达式(x, root, out)),
        ExprKind::Call {
            function,
            arguments,
        } => {
            if let Some(c) = function.expr() {
                表达式(c, root, out);
            }
            arguments.iter().for_each(|x| 表达式(x, root, out));
        }
        ExprKind::Field { value, .. } | ExprKind::Unary { value, .. } => {
            表达式(value, root, out)
        }
        ExprKind::Index { value, index } => {
            表达式(value, root, out);
            表达式(index, root, out);
        }
        ExprKind::Binary { left, right, .. } => {
            表达式(left, root, out);
            表达式(right, root, out);
        }
        _ => {}
    }
}

/// 第 `at` 条语句绑定的 `name`，在本块其后的每次出现都只在不接走责任的位置
fn 没有去向(name: &str, 出口: bool, at: usize, b: &Block) -> bool {
    let mut ok = true;
    for s in b.statements.iter().skip(at + 1) {
        match s {
            Statement::Let { name: n, value, .. } => {
                ok &= 位置(value, name, 出口);
                if n == name {
                    // 名字被重新绑定：之后的出现不是它
                    return ok;
                }
            }
            Statement::Function { function, .. } => ok &= !块里提到(&function.body, name),
            Statement::Expr(e) => ok &= 位置(e, name, 出口),
        }
    }
    if let Some(r) = &b.result {
        // 作块结果：名字本身出现即转交；别的位置按同一规则
        ok &= 位置(r, name, 出口) && !matches!(r.kind(), ExprKind::Name(n) if n == name);
    }
    ok
}

/// 第 `at` 条语句绑定的出口 `name` 在本块其后有**确定的**作者去向（过程记录 5.25）：直接作块结果或在块结果的记录、
/// 列表字面量里（逐层），或是 `handle`、`consume`、`escalate`、`literalize`、`refine`、`unsure_default`（Z0514）的第一个实参（语句本身或 `let`
/// 的值就是这个调用，不嵌在分支、闭包或别的调用里）。名字被重新绑定后的不算
fn 确定去向(name: &str, at: usize, b: &Block) -> bool {
    let 接走 = |e: &Expr| {
        matches!(
            call_name(e),
            Some("handle" | "consume" | "escalate" | "literalize" | "refine" | "unsure_default")
        ) && matches!(call_args(e).first().map(|x| x.kind()), Some(ExprKind::Name(n)) if n == name)
    };
    for s in b.statements.iter().skip(at + 1) {
        match s {
            Statement::Let { name: n, value, .. } => {
                if 接走(value) {
                    return true;
                }
                if n == name {
                    return false;
                }
            }
            Statement::Expr(e) if 接走(e) => return true,
            _ => {}
        }
    }
    fn 在结果里(e: &Expr, name: &str) -> bool {
        match e.kind() {
            ExprKind::Name(n) => n == name,
            ExprKind::List(items) => items.iter().any(|x| 在结果里(x, name)),
            ExprKind::Record(fields) => fields.iter().any(|(_, x)| 在结果里(x, name)),
            _ => false,
        }
    }
    b.result
        .as_ref()
        .is_some_and(|r| 在结果里(r, name) || 接走(r))
}

fn 块里提到(b: &Block, name: &str) -> bool {
    let mut found = false;
    walk_block(b, &mut |x| {
        if matches!(x.kind(), ExprKind::Name(n) if n == name) {
            found = true;
        }
    });
    found
}

/// `e` 里 `name` 的每次出现都在不接走责任的位置
fn 位置(e: &Expr, name: &str, 出口: bool) -> bool {
    match e.kind() {
        // 裸名出现在这里：它的直接容器不是下面认得的读法位置，可能接走
        ExprKind::Name(n) => n != name,
        ExprKind::Call {
            function,
            arguments,
        } => {
            let fname = function.name();
            let 读法位 = 出口 && fname.is_some_and(|f| 读法.contains(&f));
            let 窄引用 = !出口 && matches!(fname, Some("accepted") | Some("ignored"));
            if let Some(c) = function.expr()
                && !位置(c, name, 出口)
            {
                return false;
            }
            arguments.iter().enumerate().all(|(i, a)| {
                if i == 0
                    && (读法位 || 窄引用)
                    && matches!(a.kind(), ExprKind::Name(n) if n == name)
                {
                    return true;
                }
                位置(a, name, 出口)
            })
        }
        ExprKind::Field { value, field } => {
            if matches!(value.kind(), ExprKind::Name(n) if n == name) {
                return if 出口 {
                    field.as_str() == "kind"
                } else {
                    field.as_str() == "value"
                };
            }
            位置(value, name, 出口)
        }
        ExprKind::Binary { left, right, .. } if 出口 => {
            let 裸 = |x: &Expr| matches!(x.kind(), ExprKind::Name(n) if n == name);
            (裸(left) || 位置(left, name, 出口)) && (裸(right) || 位置(right, name, 出口))
        }
        ExprKind::Unary { value, .. } if 出口 => {
            matches!(value.kind(), ExprKind::Name(n) if n == name) || 位置(value, name, 出口)
        }
        ExprKind::If { condition, yes, no } => {
            let 条件 = (出口 && matches!(condition.kind(), ExprKind::Name(n) if n == name))
                || 位置(condition, name, 出口);
            条件 && !块里提到(yes, name) && !块里提到(no, name)
        }
        ExprKind::Binary { left, right, .. } => 位置(left, name, 出口) && 位置(right, name, 出口),
        ExprKind::Unary { value, .. } => 位置(value, name, 出口),
        ExprKind::Index { value, index } => 位置(value, name, 出口) && 位置(index, name, 出口),
        ExprKind::List(items) => items.iter().all(|x| 位置(x, name, 出口)),
        ExprKind::Record(fields) => fields.iter().all(|(_, x)| 位置(x, name, 出口)),
        ExprKind::Function(f) => !块里提到(&f.body, name),
        ExprKind::Block(b) => !块里提到(b, name),
        _ => true,
    }
}
