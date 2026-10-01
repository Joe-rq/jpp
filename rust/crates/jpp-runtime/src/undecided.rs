//! 未决值传播（主控板 B0492 S3；J-05 草案第三稿 (5a)；Z0207 第 3 条：复用 `Value::Duty`）。
//!
//! 一条规则：结果依赖未决值内容的运算，结果就是那个未决值。放进容器照放；交给责任形式照常；比较、算术、拼接、
//! 取字段、下标、`if` 条件、按值计算的内置得到未决值；效应参数里有未决值则不发出、账本记 `Skip`。
//! 多个不同的未决值相遇，合并成一个合成出口（`parts` 是各分量，同 `compose` 的吸收口径）。
//! 未决值不当假、不当空、不报运行期类型错。依据：B131（强 Kleene）推到一般的值上，取保守的一半。

use super::*;

/// 按值计算的内置：实参里有未决值，结果就是它（只看实参本身）
const 按值: &[&str] = &[
    "text",
    "split",
    "lower",
    "upper",
    "trim",
    "replace",
    "starts_with",
    "ends_with",
    "index_of",
    "chars",
    "regex_match",
    "regex_find",
    "parse_json",
    "hash",
    "abs",
    "floor",
    "range",
    "slice",
    "has",
    "keys",
    "date_parse",
    "date_format",
    "date_add",
    "mat",
    "content",
    "state",
    "cut",
    // 主控复核 2026-09-30：to_json 会把未决值渲染成普通文本，下游再认不出来
    "to_json",
];
/// 其中结果依赖列表、记录内容的：看进去
const 看进去: &[&str] = &[
    "sum", "min", "max", "join", "sort", "sort_by", "contains", "text", "mat", "state", "to_json",
];

/// 值本身是未决值
pub(crate) fn 未决(v: &Value) -> Option<Rc<Exit>> {
    match v {
        Value::Duty(e) => Some(e.clone()),
        _ => None,
    }
}

/// 值里（含列表、记录、`stop` 里）的未决值，按出口号去重、按出现序
pub(crate) fn 收未决(v: &Value, out: &mut Vec<Rc<Exit>>) {
    match v {
        Value::Duty(e) => {
            if !out.iter().any(|x| x.id == e.id) {
                out.push(e.clone());
            }
        }
        Value::List(l) => l.iter().for_each(|x| 收未决(x, out)),
        Value::Record(r) => r.iter().for_each(|(_, x)| 收未决(x, out)),
        Value::Stop(x) => 收未决(x, out),
        _ => {}
    }
}

impl<'a> Interp<'a> {
    /// 多个未决值相遇：一个就是它；多个合成一个出口（原因相同取之，不同记 `merged`）。分量都已了结（默认链记过账），
    /// 合成出口也已了结；否则没了结的分量被吸收（`consumed_by = "compose"`，B162 按账本键仍认它），责任随合成出口走。
    pub(crate) fn 合并未决(&mut self, ds: Vec<Rc<Exit>>, sp: Span) -> Value {
        let mut 分量: Vec<Rc<Exit>> = vec![];
        for d in ds {
            if !分量.iter().any(|x| x.id == d.id) {
                分量.push(d);
            }
        }
        if 分量.len() == 1 {
            return Value::Duty(分量.remove(0));
        }
        // 步 36 G3（待定项 3，主控定）：各分量原因相同取它；不同取第一个缺席类原因，没有就取第一个（与 `compose` 的
        // 「未决原因」同规则；改前记非成员 `merged`）
        let 各因: Vec<Why> = 分量.iter().filter_map(|d| d.why().cloned()).collect();
        let cause = if 各因.iter().all(|w| Some(w) == 各因.first()) {
            各因.first().cloned()
        } else {
            各因
                .iter()
                .find(|w| w.cause.is_absent_class())
                .or(各因.first())
                .cloned()
        }
        .unwrap_or_else(|| Why::of(UnsureCause::Tie));
        let taint = if 分量.iter().any(|d| d.taint == Taint::Untrusted) {
            Taint::Untrusted
        } else {
            Taint::Trusted
        };
        let Value::Exit(m) =
            self.new_exit(ExitKind::Unsure(cause), None, Op::Test, "", "", taint, sp)
        else {
            unreachable!("new_exit 给出口")
        };
        *m.parts.borrow_mut() = 分量.clone();
        if 分量
            .iter()
            .all(|d| d.consumed.get() && !crate::duty::已被吸收(d))
        {
            m.consumed.set(true);
            *m.consumed_by.borrow_mut() = "merge:已了结".into();
        } else {
            for d in &分量 {
                if !d.consumed.get() {
                    d.consumed.set(true);
                    *d.consumed_by.borrow_mut() = "compose".into();
                }
            }
        }
        Value::Duty(m)
    }

    /// 内置入口：按值计算的内置、效应，实参里有未决值时的结果（`None` = 照常求值）。
    /// 效应不发出，账本记 `Skip{of, kind, site}`（草案 (5a)「该效应不发出，账本记一条跳过」）。
    pub(crate) fn 未决实参(
        &mut self,
        name: &'static str,
        args: &[Value],
        sp: Span,
    ) -> Option<Value> {
        let 效应 = jpp_effects::by_name(name).is_some();
        let mut ds = vec![];
        if 效应 || 看进去.contains(&name) {
            args.iter().for_each(|a| 收未决(a, &mut ds));
        } else if 按值.contains(&name) {
            ds.extend(args.iter().filter_map(未决));
        }
        if ds.is_empty() {
            return None;
        }
        if 效应 {
            let mut of: Vec<String> = vec![];
            for d in &ds {
                for k in crate::duty::责任键(d) {
                    if !of.contains(&k) {
                        of.push(k);
                    }
                }
            }
            self.写去向(Entry::Skip {
                of,
                kind: name.into(),
                site: sp.start,
                attempt: None,
            });
        }
        Some(self.合并未决(ds, sp))
    }
}
