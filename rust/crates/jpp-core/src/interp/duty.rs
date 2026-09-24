//! 责任：`handle` 与未决分支的处置（20 §2.3 `duty.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    pub(in crate::interp) fn handle(&mut self, e: &Rc<Exit>, arms: &Value, sp: Span) -> R<Value> {
        let Value::Record(_) = arms else {
            return err(
                Some("J-05"),
                "handle 的第二个参数是记录 {act, ignore, pick, at, unsure, otherwise}",
                sp,
            );
        };
        let required: &[&str] = match e.op {
            Op::Test => &["act", "ignore", "unsure"],
            Op::Select => &["pick", "unsure"],
            Op::Measure => &["at", "unsure"],
        };
        let has_other = arms.get("otherwise").is_some();
        // 语法覆盖约束：Unsure 必须有显式的 unsure 臂，通配分支不能代替它。
        // 其余去向可以由 otherwise 兜底。
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|k| arms.get(k).is_none() && (*k == "unsure" || !has_other))
            .collect();
        if !missing.is_empty() {
            return err(
                Some("J-05"),
                format!(
                    "handle 不穷尽：{} 题的出口缺分支 {}。修法：补上；注意 unsure 必须自己写一臂，otherwise 兜不住未决责任",
                    e.op.phys(),
                    missing.join(", ")
                ),
                sp,
            );
        }
        if e.is_unsure() {
            let arm = arms.get("unsure").unwrap();
            // **unsure 臂也要压守卫。** 那里拿到的是未决责任，**本来就不是一个放行判定**，
            // 所以它不提供 `trusted` 合取项。但**不压就是空栈 → 整条 J-08 跳过 →
            // 臂里的不可逆 `do` 自由执行**，与这次修的是同一个洞。
            self.guards.push(守卫(e));
            let r = self.handle_unsure(e, &arm, sp);
            self.guards.pop();
            return r;
        }
        let (name, arg): (&str, Value) = match &e.kind {
            ExitKind::Act => ("act", Value::Unit),
            ExitKind::Ignore => ("ignore", Value::Unit),
            // B84：k 继承出口 taint（`20` §3.10「Exit → Bool 继承出口」同一规则，修现状恒 Trusted 的漏），
            // 并带 sources = {出口键}；`over[k]` 经下标规则把它带给取出的候选或题
            ExitKind::Pick(k) => ("pick", Value::Int(*k as i64, 出口标签(e))),
            ExitKind::At(l) => ("at", Value::Int(*l as i64, 出口标签(e))),
            ExitKind::Unsure(_) => unreachable!("上面已经分流"),
        };
        let arm = arms.get(name).or_else(|| arms.get("otherwise")).unwrap();
        e.consumed.set(true);
        *e.consumed_by.borrow_mut() = format!("handle:{name}");
        match arm {
            Value::Fn(c) => {
                let n = c.function.parameters.len();
                let args = if n == 0 { vec![] } else { vec![arg] };
                // **这一臂之所以执行，正是因为那次 `cut` 切出了这个出口——那个出口就是守卫。**
                // 以前 `guards` 只在 `if` 处 push，于是写在 handler 臂里的不可逆 `do`
                // 在 J-08 眼里是「无条件执行」，完全不受管——**而那正是 §5 与 J-05 推荐的写法**。
                // `act`/`ignore`/`pick`/`at` 全走这里，**漏掉任何一个就是把同一个洞挪过去一个分支**。
                self.guards.push(守卫(e));
                let r = self.call_closure(&c, args, sp);
                // **出错路径也要弹**：早返回会把脏栈留给这次运行的其余部分。
                self.guards.pop();
                r
            }
            other => Ok(other),
        }
    }

    /// unsure 臂收到的是**未决责任本身**（`Value::Duty`），不是原因文本。臂体跑完再核它是不是真的
    /// 交出去了：escalate 给人、字面化重问、包装进返回值、或显式 drop。什么都不做就是静默丢弃（J-05）。
    pub(in crate::interp) fn handle_unsure(
        &mut self,
        e: &Rc<Exit>,
        arm: &Value,
        sp: Span,
    ) -> R<Value> {
        let Value::Fn(c) = arm else {
            return err(
                Some("J-05"),
                format!(
                    "unsure 的臂是个 {}，收不下未决责任。修法：写成 unsure: fn(u) {{ … }}，在体内 escalate(u, …) / literalize(u, …) / consume(u, \"drop\")，或把 u 包进返回值交给调用者",
                    arm.type_name()
                ),
                sp,
            );
        };
        if c.function.parameters.is_empty() {
            return err(
                Some("J-05"),
                "unsure 的臂没有参数，接不到未决责任。修法：写成 fn(u) { … }",
                c.span,
            );
        }
        let site = c.span;
        let result = self.call_closure(c, vec![Value::Duty(e.clone())], sp)?;
        // escalate / literalize / consume 已经在臂体里销过账
        if e.consumed.get() {
            return Ok(result);
        }
        let mut carried = HashSet::new();
        collect_exit_ids(&result, &mut carried);
        if carried.contains(&e.id) {
            // 13 §3：包进返回值是**转交**，不是了结。这里不销账——责任继续挂着，
            // 交给函数返回检查与程序结束前检查去核。那两处把两件事分开：
            // 责任**真被丢了**是错；责任**如实交了出去、只是签名没说**是警告（见 call_closure）。
            *e.consumed_by.borrow_mut() = "handle:unsure(转交调用者)".into();
            return Ok(result);
        }
        err(
            Some("J-05"),
            format!(
                "unsure 的臂把未决责任丢了：{} 既没 escalate、没重问、没 drop，也没进返回值。进臂不等于销账。修法：escalate(u, state, 题) 交给人，literalize(u, state, 更字面的题) 重问，consume(u, \"drop\") 显式丢并记账，或把 u 放进返回值",
                e.label()
            ),
            site,
        )
    }
}
