//! 列表、数与文本的通用内置。步 36 G3 从 `host_builtins.rs` 原样搬出（只搬不改）。

use super::*;

impl<'a> Interp<'a> {
    #[allow(unused_variables)]
    pub(crate) fn b_len(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
            Value::List(l) => Ok(Value::Int(l.len() as i64, Taint::Trusted.into())),
            Value::Text(t, _) => Ok(Value::Int(t.chars().count() as i64, Taint::Trusted.into())),
            Value::Record(r) => Ok(Value::Int(r.len() as i64, Taint::Trusted.into())),
            other => err(
                Some("E-rt-type"),
                format!("len 不适用于 {}", other.type_name()),
                sp,
            ),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_map(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::List(l), f) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), format!("{name}(list, fn)"), sp);
        };
        self.vectorize_ahead(f, l, sp);
        let mut out = vec![];
        let mut 未决谓词: Vec<Rc<Exit>> = vec![];
        for it in l.iter() {
            let r = self.apply(f.clone(), vec![it.clone()], sp)?;
            if name == "map" {
                out.push(r);
                continue;
            }
            // filter 的谓词必须返回 Bool。返回别的东西以前被**静默当假**：
            // 出口、未决责任传进来会无声消失，正是 13 §3 要堵的那类。
            match r {
                Value::Bool(true, _, _) => out.push(it.clone()),
                Value::Bool(false, _, _) => {}
                // 未决值传播（B0492 S3）：谓词未决，filter 的结果依赖它，整个结果是它（多个合并）
                Value::Duty(d) => 未决谓词.push(d),
                other => {
                    return err(
                        Some("E-rt-type"),
                        format!(
                            "filter 的谓词要返回 Bool（真假），收到 {}。返回别的东西以前被当成假、元素被静默丢掉。修法：让谓词自己算出真假再返回",
                            other.type_name()
                        ),
                        sp,
                    );
                }
            }
        }
        if !未决谓词.is_empty() {
            return Ok(self.合并未决(未决谓词, sp));
        }
        Ok(Value::list(out))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_fold(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::List(l), init, f) = (&args[0], &args[1], &args[2]) else {
            return err(Some("E-rt-arg"), "fold(list, init, fn(acc, x))", sp);
        };
        let mut acc = init.clone();
        for it in l.iter() {
            acc = self.apply(f.clone(), vec![acc, it.clone()], sp)?;
        }
        Ok(acc)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_range(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "range(a, b)", sp);
        };
        Ok(Value::list((*a..*b).map(Value::int).collect()))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_append(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "append(list, v)", sp);
        };
        let mut v: Vec<Value> = l.iter().cloned().collect();
        v.push(args[1].clone());
        Ok(Value::list(v))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_concat(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::List(a), Value::List(b)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "concat(a, b)", sp);
        };
        Ok(Value::list(a.iter().chain(b.iter()).cloned().collect()))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_slice(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::List(l), Value::Int(a, _), Value::Int(b, _)) = (&args[0], &args[1], &args[2])
        else {
            return err(Some("E-rt-arg"), "slice(list, a, b)", sp);
        };
        let a = (*a).clamp(0, l.len() as i64) as usize;
        let b = (*b).clamp(a as i64, l.len() as i64) as usize;
        Ok(Value::list(l[a..b].to_vec()))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_contains(
        &mut self,
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
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "contains(list, v)", sp);
        };
        for it in l.iter() {
            match it.equals(&args[1]) {
                Some(true) => return Ok(Value::Bool(true, Taint::Trusted.into(), GuardEv::EMPTY)),
                None => return err(Some("J-01"), "读数不可比", sp),
                _ => {}
            }
        }
        Ok(Value::Bool(false, Taint::Trusted.into(), GuardEv::EMPTY))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_sum(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "sum(list)", sp);
        };
        let mut acc = Value::Int(0, Taint::Trusted.into());
        for it in l.iter() {
            acc = self.binop("+", acc, it.clone(), sp)?;
        }
        Ok(acc)
    }

    #[allow(unused_variables)]
    pub(crate) fn b_min(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        // 步 20j-4：声明式拟合的闭包要在概率与期望档位上取大取小（B153 (2) 的 `max(a.p, b.p)`），
        // 所以收 Float；Int 与 Float 混用提升为 Float，与算术同一条规则（B70）。两个 Int 照旧是 Int
        let 取 = |v: &Value| match v {
            Value::Int(i, _) => Some(*i as f64),
            Value::Float(f, _) => Some(*f),
            _ => None,
        };
        match (&args[0], &args[1]) {
            (Value::Int(a, _), Value::Int(b, _)) => Ok(Value::Int(
                if name == "min" { *a.min(b) } else { *a.max(b) },
                Taint::Trusted.into(),
            )),
            (x, y) => match (取(x), 取(y)) {
                (Some(a), Some(b)) => Ok(Value::Float(
                    if name == "min" { a.min(b) } else { a.max(b) },
                    Taint::Trusted.into(),
                )),
                _ => err(Some("E-rt-arg"), format!("{name}(数, 数)"), sp),
            },
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_abs(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
            // 13 §6：i64::MIN 没有对应的正数，取绝对值同样越界
            Value::Int(a, _) => Ok(Value::Int(
                a.checked_abs()
                    .ok_or_else(|| overflow("取绝对值", *a, 0, sp))?,
                Taint::Trusted.into(),
            )),
            Value::Float(a, _) => Ok(Value::Float(a.abs(), Taint::Trusted.into())),
            _ => err(Some("E-rt-arg"), "abs(number)", sp),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_floor(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
            Value::Float(a, _) => Ok(Value::Int(a.floor() as i64, Taint::Trusted.into())),
            Value::Int(a, _) => Ok(Value::Int(*a, Taint::Trusted.into())),
            _ => err(Some("E-rt-arg"), "floor(number)", sp),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_reverse(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let Value::List(l) = &args[0] else {
            return err(Some("E-rt-arg"), "reverse(list)", sp);
        };
        Ok(Value::list(l.iter().rev().cloned().collect()))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_keys(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let Value::Record(r) = &args[0] else {
            return err(Some("E-rt-arg"), "keys(record)", sp);
        };
        Ok(Value::list(r.iter().map(|(k, _)| Value::text(k)).collect()))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_has(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::Record(_), Value::Text(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "has(record, key)", sp);
        };
        Ok(Value::Bool(
            args[0].get(k).is_some(),
            Taint::Trusted.into(),
            GuardEv::EMPTY,
        ))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_with(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::Record(r), Value::Text(k, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "with(record, key, value)", sp);
        };
        let mut v: Vec<(String, Value)> = r
            .iter()
            .filter(|(kk, _)| kk.as_str() != k.as_ref())
            .cloned()
            .collect();
        v.push((k.to_string(), args[2].clone()));
        Ok(Value::record(v))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_text(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
            Value::Text(t, _) => Ok(Value::text(t)),
            Value::Reading(_) => err(Some("J-01"), "读数不能转文字", sp),
            // 读出规则：材料的文字带材料的位；其余由分派处的 ∨ 输入给出
            // B84：同时带材料的来源读数
            Value::Mat(m) => Ok(Value::Text(Rc::from(m.text().as_str()), m.prov())),
            other => Ok(Value::text(other.to_json().to_string().trim_matches('"'))),
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn b_join(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let (Value::List(l), Value::Text(sep, _)) = (&args[0], &args[1]) else {
            return err(Some("E-rt-arg"), "join([Text], sep)", sp);
        };
        let parts: Vec<String> = l
            .iter()
            .map(|v| match v {
                Value::Text(t, _) => t.to_string(),
                o => o.to_json().to_string(),
            })
            .collect();
        Ok(Value::text(&parts.join(sep)))
    }

    #[allow(unused_variables)]
    pub(crate) fn b_print(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        let s = args[0].to_json().to_string();
        self.trace.push("print", "", false, 0.0, sp, s);
        Ok(Value::Unit)
    }
}
