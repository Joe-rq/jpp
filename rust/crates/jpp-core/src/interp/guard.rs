//! 守卫：条件的可信合取项与来源追踪（J-08；20 §2.3 `guard.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    /// 这个条件表达式里，有没有**来自 trusted 状态的合取项**、有没有经 `ask`（J-08）。
    ///
    /// 「合取项」按 `&&` 拆——`12`:265 说的是**合取**，所以 `||` 的两侧不算独立合取项
    /// （`a || b` 成立时不知道是哪一侧成立，不能声称 trusted 那一侧放的行）。
    ///
    /// 追来源：一个合取项最终来自哪个状态，靠的是**出口身上带的 taint**（`cut` 继承状态 taint，
    /// 前面几包刚做的）。所以这里顺着名字回到环境里的值，看它是不是由某个出口决定的。
    /// **追不到就当不可信**——兜底往拒绝那边倒。
    pub(in crate::interp) fn guard_of(&self, cond: &Expr, env: &Env) -> GuardInfo {
        let mut info = GuardInfo::default();
        self.walk_conjuncts(cond, env, &mut info);
        info
    }

    pub(in crate::interp) fn walk_conjuncts(&self, e: &Expr, env: &Env, info: &mut GuardInfo) {
        match kind(e) {
            // 合取：两侧都是独立的合取项
            K::Binary {
                op: "&&",
                left,
                right,
            } => {
                self.walk_conjuncts(left, env, info);
                self.walk_conjuncts(right, env, info);
            }
            // 取反、析取：里面的东西不再是「这个条件成立所保证的」，不追
            K::Unary { .. } | K::Binary { .. } => {}
            _ => {
                if let Some(src) = self.taint_source(e, env) {
                    info.trusted |= src.0;
                    info.asked |= src.1;
                }
            }
        }
    }

    pub(in crate::interp) fn lookup_field_prov(
        &self,
        env: &Env,
        name: &str,
        field: &str,
    ) -> Option<(bool, bool)> {
        match env_lookup(env, &field_prov_key(name, field))? {
            Value::List(l) if l.len() == 2 => match (&l[0], &l[1]) {
                (Value::Bool(t, _), Value::Bool(a, _)) => Some((*t, *a)),
                _ => None,
            },
            _ => None,
        }
    }

    /// 从 `before` 之后本帧新增的出口里折出来源（同一个字段内部**可以**取并——
    /// 那是「这个字段由哪些判断共同决定」，不是把两个字段混在一起）。
    pub(in crate::interp) fn provenance_since(&self, before: usize) -> Option<(bool, bool)> {
        let exits = &self.frames.last()?.exits;
        let made = &exits[before.min(exits.len())..];
        if made.is_empty() {
            return None;
        }
        Some(made.iter().fold((false, false), |acc, x| {
            (acc.0 || x.guard_trusted(), acc.1 || x.from_ask.get())
        }))
    }

    /// 字段的值直接引用了某个已有绑定时，取那个绑定的来源
    pub(in crate::interp) fn field_provenance_of(
        &self,
        e: &Expr,
        env: &Env,
    ) -> Option<(bool, bool)> {
        match kind(e) {
            K::Name(_) | K::Field { .. } => self.taint_source(e, env),
            _ => None,
        }
    }

    /// 一个合取项的来源：`(来自 trusted 状态, 经过 ask)`。追不到返回 `None`。
    ///
    /// 布尔名字的来源**从环境里查**（`x\u{1f}prov`）——那是在 `let` 绑定时连同值一起绑进去的：
    /// **求值这个绑定的过程中产生了哪些出口**。出口身上带着状态的 taint（`cut` 继承，
    /// 前面几包做的），所以「这个布尔是不是由可信状态上的判断决定的」答得出来。
    pub(in crate::interp) fn taint_source(&self, e: &Expr, env: &Env) -> Option<(bool, bool)> {
        match kind(e) {
            K::Name(n) => {
                let v = env_lookup(env, n)?;
                match &v {
                    Value::Exit(x) | Value::Duty(x) => Some((x.guard_trusted(), x.from_ask.get())),
                    Value::Bool(_, _) | Value::Record(_) => match env_lookup(env, &prov_key(n)) {
                        Some(Value::List(l)) if l.len() == 2 => match (&l[0], &l[1]) {
                            (Value::Bool(t, _), Value::Bool(a, _)) => Some((*t, *a)),
                            _ => None,
                        },
                        _ => None,
                    },
                    _ => None,
                }
            }
            // **取字段先查这个字段自己的来源**；查不到才退回整条记录的。
            // 此前直接继承整条记录——于是「由可信判断决定的字段」和「由不可信判断决定的字段」
            // 是同一个来源，包一层记录再取字段就绕过了守卫那一层对析取的拒绝。
            K::Field { value, field } => {
                if let K::Name(n) = kind(value) {
                    if let Some(p) = self.lookup_field_prov(env, n, field) {
                        return Some(p);
                    }
                    // 记录里有这个字段、但它没有记到来源 = 这个字段不是判断决定的（字面量之类）
                    if let Some(Value::Record(fs)) = env_lookup(env, n) {
                        if fs.iter().any(|(k, _)| k == field)
                            && env_lookup(env, &field_prov_key(n, field)).is_some()
                        {
                            return None;
                        }
                    }
                }
                self.taint_source(value, env)
            }
            _ => None,
        }
    }
}
