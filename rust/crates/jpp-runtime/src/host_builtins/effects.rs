//! 效应与材料：`gen`、`do`、`ask`、`transform`、`mat`、`content`。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改）。

use super::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_gen(
        &mut self,
        s: &'static EffectSpec,
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
        arity(4)?;
        let (Value::Text(p, _), ctx, Value::Int(k, _), Value::Int(r, _)) =
            (&args[0], &args[1], &args[2], &args[3])
        else {
            return err(Some("E-rt-arg"), "gen(prompt, [ctx], n, retry_seq)", sp);
        };
        let (ctx, _) = self.as_mats(ctx, "ctx", sp)?;
        let p = p.to_string();
        self.generate(s, &p, &ctx, *k as usize, *r, sp)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_do(
        &mut self,
        s: &'static EffectSpec,
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
        arity(3)?;
        let (Value::Text(a, _), Value::List(l), Value::Int(i, _)) = (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "do(action, [args], iter_seq)", sp);
        };
        let a = a.to_string();
        let l: Vec<Value> = l.iter().cloned().collect();
        self.do_(s, &a, &l, *i, sp)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_ask(
        &mut self,
        spec: &'static EffectSpec,
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
        arity(2)?;
        let (Value::State(s), Value::Question(q)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "ask(state, question)", sp);
        };
        let (s, q) = (s.clone(), q.clone());
        self.ask(spec, &s, &q, sp)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_transform(
        &mut self,
        s: &'static EffectSpec,
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
        if n < 1 {
            return err(Some("E-rt-arg"), "transform(f, mats…)", sp);
        }
        match &args[0] {
            Value::Fn(f) => {
                let f = f.clone();
                self.transform(s, crate::effects_exec::变换方法::闭包(&f), &args[1..], sp)
            }
            // 宿主变换（步 26，`12` §2.8 `f: HostFn`）：第一参是宿主变换表里登记的名字，与 `do("名", …)` 同一写法
            Value::Text(n, _) => {
                let Some(table) = self.transforms else {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "没有宿主变换表，transform(\"{n}\", …) 无从执行。修法【宿主】：执行前用 jpp_lib::s_library(&program) 构造并经 Interp::set_transforms 注入（jpp::Session 已这样做）"
                        ),
                        sp,
                    );
                };
                let Some(h) = table.get(n) else {
                    return err(
                        Some("E-rt-arg"),
                        format!(
                            "宿主变换 {n} 没有登记。已登记：{}",
                            table.names().join("、")
                        ),
                        sp,
                    );
                };
                self.transform(s, crate::effects_exec::变换方法::宿主(h), &args[1..], sp)
            }
            _ => err(
                Some("E-rt-arg"),
                "transform 的第一个参数要是函数，或登记过的宿主变换名（文字）",
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_mat(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        arity(1)?;
        Ok(Value::Mat(Rc::new(self.as_mat(&args[0], "mat", sp)?)))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_content(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        arity(1)?;
        match &args[0] {
            // 读出规则（B33 第 2 点）：从 untrusted 材料读出的值，所有叶子标 untrusted。
            // 值自己带着来源，拼接、join、text、取字段之后仍带着（内置输出 ∨ 输入），
            // 取代此前按值匹配的旁路表（982d7ca）：那张表没有作用域，同时漏与串。
            // B84：读出的叶子同时带材料的来源读数
            Value::Mat(m) => Ok(json_to_value(&m.content).with_prov(&m.prov())),
            Value::Reading(_) => err(Some("J-01"), "读数没有内容可读；只能经 cut 离开", sp),
            // 依据：B153 (2)（Score 不能读出为数）
            Value::Score(_) => err(
                Some("J-01"),
                "声明式拟合的结果（Score）没有内容可读：它不是数，只能进 cut 的声明线或同一拟合的 order（B153）",
                sp,
            ),
            other => err(
                Some("E-rt-arg"),
                format!("content 只收材料，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
}
