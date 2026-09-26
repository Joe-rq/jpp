//! `E-action-no-sandbox`（B164）：`exec_py`/`check_tests`/`exec_sql` 这类执行器动作只在
//! 宿主启动时探测到操作系统级沙箱（`sandbox-exec`/`bwrap`）才能跑；探测不到时，宿主的
//! `ActionFacts.no_sandbox` 为 `true`（同时 `reversible` 也会被置 `false`，J-08 因此也会要求
//! 守卫），但「没有沙箱」是环境问题，不是放行策略能解决的——不管调用点有没有守卫、
//! 有没有认证过的线，都不该被 `declare`/试用线之类的机制放行。所以这条无条件报，
//! 同 J-11「未登记动作」同一口径：不看守卫，纯粹看动作名字面量与动作表。
//!
//! 依据：主会话 2026-09-26 传达的 Fable 裁定 B164（写入 `12` B150 条、`21` 24e-1 补注，
//! 尚未合入时以本文件与过程记录 `地基/过程记录/工程-执行器动作安全修补.md` 为准）。

#![allow(unused_imports)]
use super::{CallSite, Cx, Hooks, Rule};
use crate::*;
use jpp_effects::spec::{ProfileSchema, SlotKind};

/// 依据：B164（宿主启动时探测沙箱；探测不到则该动作无条件报错，不看守卫）。
pub(crate) const RULE: Rule = Rule {
    code: "E-action-no-sandbox",
    requires: &[],
    hooks: Hooks {
        call: Some(call),
        ..Hooks::NONE
    },
};

fn call(cx: &Cx, s: &CallSite) -> Vec<Diagnostic> {
    let mut out = vec![];
    let Some(sp) = jpp_effects::by_name(s.name) else {
        return out;
    };
    if sp.profile_schema != ProfileSchema::Action {
        return out;
    }
    let Some(pos) = sp
        .input_schema
        .iter()
        .position(|d| d.kind == SlotKind::Name)
    else {
        return out;
    };
    let Some(name_arg) = s.args.get(pos) else {
        return out;
    };
    match name_arg.kind() {
        ExprKind::Text(name) => report(cx, &mut out, name, name_arg.span, None),
        // 动作名不是字面量：查是不是「形参经全程序调用点都同一字面量」（B179 (b)，步 24e-4）；
        // 追不到就静态判不了，交运行期（同 J-11 的口径）
        _ => {
            if let Some(v) = cx.via_param.get(&(*name_arg as *const Expr)) {
                let via = format!("经 `{}` 的形参 `{}` 传入", v.fn_name, v.param);
                for span in &v.call_sites {
                    report(cx, &mut out, &v.action, *span, Some(&via));
                }
            }
        }
    }
    out
}

fn report(cx: &Cx, out: &mut Vec<Diagnostic>, name: &str, span: Span, via: Option<&str>) {
    // 没有动作表：检查期不知道注册了什么，不报（同 J-08/J-11 静态子面的口径）
    let Some(actions) = cx.actions else {
        return;
    };
    let Some(facts) = actions.actions.get(name) else {
        return; // 未登记的名字交给 J-11
    };
    if facts.no_sandbox {
        let 来源 = via.map(|v| format!("（{v}）")).unwrap_or_default();
        out.push(Diagnostic::error(
            "E-action-no-sandbox",
            format!(
                "动作 {name} 需要操作系统级沙箱（macOS sandbox-exec 或 Linux bwrap）才能执行，\
                 本机启动时没有探测到——不是放行策略能解决的问题，不管这里有没有守卫都会在\
                 运行期返回失败值（NoSandbox）{来源}。装上 sandbox-exec（macOS 自带）或 bubblewrap \
                 的 bwrap（Linux，`apt install bubblewrap`/`dnf install bubblewrap`）后重跑。"
            ),
            span,
        ));
    }
}
