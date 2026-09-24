//! 求值器：块、表达式、闭包、二元运算与整数边界、`loop`、材料与状态的构造（20 §2.3 `eval.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    pub(in crate::interp) fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    // ---------- 求值 ----------

    pub(in crate::interp) fn eval_block(&mut self, b: &Block, env: &Env) -> R<Value> {
        let env = env_child(env);
        // lift 提前登记过的语句下标：它们的绑定已经做好，轮到时跳过
        let mut lifted: HashSet<usize> = HashSet::new();
        for (i, s) in b.statements.iter().enumerate() {
            if lifted.contains(&i) {
                continue;
            }
            match s {
                Stmt::Let { name, value, .. } => {
                    // 推测执行（`12`:610「judge 推测提升」）：**在这条语句求值之前**，
                    // 把它后面 `if` 两侧分支体里此刻已能求值的 judge 站点一起登记。
                    //
                    // 为什么在这里而不是在 `if` 那里：条件自己的 `cut` 会刷新一次，
                    // 走到 `if` 时那一层**已经发出去了**，再登记就赶不上同一层了。
                    // 推测要在**触发刷新的那条语句之前**完成——与 Python `spec.py` 的
                    // `_walk_block(start=当前语句, in_progress=True)` 同一个位置。
                    self.speculate_ahead(b, value.id, &env);
                    // J-08：记下求值这个绑定时产生了哪些出口——出口带着状态 taint，
                    // 于是「这个布尔由可信状态上的判断决定吗」答得出来。
                    let before = self.frames.last().map(|f| f.exits.len()).unwrap_or(0);
                    // **求值前清空传送带**：它只承载这一条 `let` 自己求值期间的来源，
                    // 不继承上一条语句留下的。没有这一句，任意一条不相关的 `let` 都会串味。
                    self.last_eval_provenance = None;
                    self.pending_field_prov = None;
                    let v = self.eval(value, &env)?;
                    // 记录（而不只是裸 bool）也要记来源：`request_test` 那类 helper 返回的是
                    // `{resolved, value}`，守卫写成 `包.resolved && 包.value`——来源在那个记录上。
                    if matches!(v, Value::Bool(_, _) | Value::Record(_)) {
                        let prov = {
                            // 出口按帧记，而 helper 函数自成一帧——`问人(m)` 里的 ask 出口落在
                            // 它自己那帧上，求值结束帧就弹掉了。所以这里看的是**这次求值总共新增了
                            // 多少出口**：内层帧退出时未消费的出口会并进外层（`call_closure`），
                            // 已消费的（`handle` 吃掉的）则由 `last_eval_provenance` 带回来。
                            let exits = &self.frames.last().expect("有帧").exits;
                            let made = &exits[before.min(exits.len())..];
                            let mut acc = self.last_eval_provenance.take();
                            for x in made {
                                let cur = acc.unwrap_or((false, false));
                                acc = Some((cur.0 || x.guard_trusted(), cur.1 || x.from_ask.get()));
                            }
                            acc
                        };
                        // 逐字段来源也绑进环境，键是 `名字\u{1f}字段\u{1f}prov`
                        if let Some(各字段) = self.pending_field_prov.take() {
                            for (f, (t, a)) in 各字段 {
                                env_define(
                                    &env,
                                    &field_prov_key(name, &f),
                                    Value::List(Rc::new(vec![
                                        Value::Bool(t, Taint::Trusted.into()),
                                        Value::Bool(a, Taint::Trusted.into()),
                                    ])),
                                );
                            }
                        }
                        // 来源绑进**环境**，作用域与这个绑定完全一致
                        env_define(
                            &env,
                            &prov_key(name),
                            match prov {
                                Some((t, a)) => Value::List(Rc::new(vec![
                                    Value::Bool(t, Taint::Trusted.into()),
                                    Value::Bool(a, Taint::Trusted.into()),
                                ])),
                                // 显式记「这个绑定没有来源」，盖住外层同名绑定的来源
                                None => Value::Unit,
                            },
                        );
                    }
                    env_define(&env, name, v);
                    // 提升 pass（12 §4 序 1 + :610 修订记录 1 的推测提升）：
                    // 刚登记了一个 judge，就把后面**同状态**、中间无副作用的 judge 一起登记上来，
                    // 免得它们各自等到下一个刷新点、各成一层。只推测 judge——登记零成本零副作用，
                    // 所以不需要回滚。**不跨分支**（`12`:13「§4 删『跨分支提升』，提升只在直线段内」）：
                    // 下面的前瞻只走同一个块的后续语句，遇到分支或副作用就停。
                    self.lift_followers(b, value.id, &env, &mut lifted)?;
                }
                Stmt::Function {
                    name,
                    function,
                    span,
                } => {
                    let c = self.closure(function, &env, Some(name.clone()), *span);
                    env_define(&env, name, c);
                }
                Stmt::Expr(e) => {
                    self.eval(e, &env)?;
                }
            }
        }
        match &b.result {
            Some(e) => self.eval(e, &env),
            None => Ok(Value::Unit),
        }
    }

    pub(in crate::interp) fn closure(
        &self,
        f: &Function,
        env: &Env,
        name: Option<String>,
        span: Span,
    ) -> Value {
        // 方法身份：降级时按源码函数算好（口径与步 12c 前相同，账本键不变）
        let hash = f.source_hash.clone();
        Value::Fn(Rc::new(Closure {
            function: f.clone(),
            env: env.clone(),
            name,
            span,
            hash,
        }))
    }

    pub(in crate::interp) fn eval(&mut self, e: &Expr, env: &Env) -> R<Value> {
        let sp = e.span;
        match kind(e) {
            K::Integer(i) => Ok(Value::Int(i, Taint::Trusted.into())),
            K::Decimal(d) => Ok(Value::Float(d, Taint::Trusted.into())),
            K::Bool(b) => Ok(Value::Bool(b, Taint::Trusted.into())),
            K::Text(t) => Ok(Value::text(t)),
            K::Unit => Ok(Value::Unit),
            K::Name(n) => env_lookup(env, n).ok_or_else(|| {
                Fault::Error(RtError::new(
                    Some("E-rt-name"),
                    format!("未定义的名字 {n}"),
                    sp,
                ))
            }),
            K::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    v.push(self.eval(it, env)?);
                }
                Ok(Value::list(v))
            }
            K::Record(fields) => {
                let mut v = Vec::with_capacity(fields.len());
                // **逐字段记来源**：`{脏字段: 脏判, 净字段: 净判}` 这两个字段的来源不同，
                // 折成一个就等于做了析取——而 `walk_conjuncts` 在守卫那一层**专门拒绝追析取**
                // （「里面的东西不再是这个条件成立所保证的」）。同一份谨慎不能隔一层被自己拆掉。
                //
                // `12`:265 要的是「至少一个**合取项**来自 trusted 状态」，**合取项是值级的概念**。
                let mut 各字段来源: Vec<(String, (bool, bool))> = vec![];
                for (k, it) in fields {
                    let before = self.frames.last().map(|f| f.exits.len()).unwrap_or(0);
                    let saved = self.last_eval_provenance.take();
                    let val = self.eval(it, env)?;
                    if let Some(p) = self.provenance_since(before) {
                        各字段来源.push((k.clone(), p));
                    } else if let Some(p) = self.field_provenance_of(it, env) {
                        // 字段直接引用一个已有绑定（`{脏字段: 脏判}`）：继承那个绑定的来源
                        各字段来源.push((k.clone(), p));
                    }
                    // 本字段的来源已归到本字段名下，不让它漏给整条记录
                    self.last_eval_provenance = saved;
                    v.push((k.clone(), val));
                }
                if !各字段来源.is_empty() {
                    self.pending_field_prov = Some(各字段来源);
                }
                Ok(Value::record(v))
            }
            K::Function(f) => Ok(self.closure(f, env, None, sp)),
            K::Block(b) => self.eval_block(b, env),
            K::If { condition, yes, no } => {
                let c = self.eval(condition, env)?;
                // 刷新点：分支要在已知信息上走，不能让未发出的判断跨过分支边界（12 §2.2:129）
                self.flush("if")?;
                // J-08：条件求值成 Bool 之后 taint 就没了，所以**在这一刻**记下这层守卫的来源。
                let guard = self.guard_of(condition, env);
                match c {
                    Value::Bool(b, _) => {
                        self.guards.push(guard);
                        let r = if b {
                            self.eval_block(yes, env)
                        } else {
                            self.eval_block(no, env)
                        };
                        self.guards.pop();
                        r
                    }
                    Value::Reading(_) => err(
                        Some("J-01"),
                        "读数不能当条件；先 cut 成出口再 handle",
                        condition.span,
                    ),
                    other => err(
                        Some("E-rt-type"),
                        format!("if 的条件要是 Bool，收到 {}", other.type_name()),
                        condition.span,
                    ),
                }
            }
            K::Field { value, field } => {
                let v = self.eval(value, env)?;
                match &v {
                    Value::Record(_) => v.get(field).ok_or_else(|| {
                        Fault::Error(RtError::new(
                            Some("E-rt-field"),
                            format!("记录没有字段 {field}"),
                            sp,
                        ))
                    }),
                    Value::Mat(m) => match field {
                        "content" => {
                            // 刷新点：宿主读内容
                            self.flush("content")?;
                            // 读出规则（B33 第 2 点）：从 untrusted 材料读出，所有叶子标 untrusted；
                            // B84：叶子同时带材料的来源读数
                            Ok(json_to_value(&m.content).with_prov(&m.prov()))
                        }
                        "taint" => Ok(Value::text(if m.taint == Taint::Trusted {
                            "trusted"
                        } else {
                            "untrusted"
                        })),
                        "hash" => Ok(Value::text(&m.hash)),
                        _ => err(Some("E-rt-field"), format!("Mat 没有字段 {field}"), sp),
                    },
                    Value::Exit(x) => match field {
                        "kind" => Ok(Value::text(&x.label())),
                        _ => err(
                            Some("E-rt-field"),
                            format!("Exit 没有字段 {field}（用 handle 消费）"),
                            sp,
                        ),
                    },
                    // B84：题的字段带题的来源（taint 不变：题今天按 trusted）
                    Value::Question(q) => question_field(q, field)
                        .map(|x| x.with_prov(&v.prov()))
                        .ok_or_else(|| {
                            Fault::Error(RtError::new(
                                Some("E-rt-field"),
                                format!(
                                    "Question 没有字段 {field}；可读字段：{}",
                                    QUESTION_FIELDS.join("、")
                                ),
                                sp,
                            ))
                        }),
                    Value::Form(f) => form_field(f, field).ok_or_else(|| {
                        Fault::Error(RtError::new(
                            Some("E-rt-field"),
                            format!(
                                "Form 没有字段 {field}；可读字段：{}",
                                FORM_FIELDS.join("、")
                            ),
                            sp,
                        ))
                    }),
                    Value::Reading(_) => err(Some("J-01"), "读数没有可读字段；只能经 cut 离开", sp),
                    other => err(
                        Some("E-rt-field"),
                        format!("{} 没有字段 {field}", other.type_name()),
                        sp,
                    ),
                }
            }
            K::Index { value, index } => {
                let v = self.eval(value, env)?;
                let i = self.eval(index, env)?;
                match (&v, &i) {
                    (Value::List(l), Value::Int(k, _)) => {
                        let k = *k;
                        if k < 0 || k as usize >= l.len() {
                            return err(
                                Some("E-rt-index"),
                                format!("下标 {k} 越界（长度 {}）", l.len()),
                                sp,
                            );
                        }
                        // B84 边界表「按计算键取下标」：sources 并入键的 sources，taint 取元素自身的位
                        //（B33 第 3 条）。字面下标的键没有来源，与今天相同。
                        Ok(l[k as usize]
                            .clone()
                            .with_prov(&Provenance::sources_only(i.prov().sources)))
                    }
                    (Value::Record(_), Value::Text(k, _)) => v
                        .get(k)
                        .map(|x| x.with_prov(&Provenance::sources_only(i.prov().sources)))
                        .ok_or_else(|| {
                            Fault::Error(RtError::new(
                                Some("E-rt-field"),
                                format!("记录没有字段 {k}"),
                                sp,
                            ))
                        }),
                    _ => err(
                        Some("E-rt-index"),
                        format!("{}[{}] 不可索引", v.type_name(), i.type_name()),
                        sp,
                    ),
                }
            }
            K::Unary { op, value } => {
                let v = self.eval(value, env)?;
                // B84：一元运算输出带操作数的标签（taint 同 B33）
                let t = v.prov();
                match (op, &v) {
                    ("!", Value::Bool(b, _)) => Ok(Value::Bool(!b, t)),
                    // 13 §6：最小整数取负也越界，同样是运行错误
                    ("-", Value::Int(i, _)) => Ok(Value::Int(
                        i.checked_neg().ok_or_else(|| overflow("取负", *i, 0, sp))?,
                        t,
                    )),
                    ("-", Value::Float(f, _)) => Ok(Value::Float(-f, t)),
                    (_, Value::Reading(_)) => err(Some("J-01"), "读数不能做算术", sp),
                    _ => err(
                        Some("E-rt-type"),
                        format!("一元 {op} 不适用于 {}", v.type_name()),
                        sp,
                    ),
                }
            }
            K::Binary { op, left, right } => {
                if op == "&&" || op == "||" {
                    let l = self.eval(left, env)?;
                    return match (op, &l) {
                        ("&&", Value::Bool(false, t)) => Ok(Value::Bool(false, t.clone())),
                        ("||", Value::Bool(true, t)) => Ok(Value::Bool(true, t.clone())),
                        (_, Value::Bool(_, lt)) => {
                            let lt = lt.clone();
                            let r = self.eval(right, env)?;
                            match r {
                                Value::Bool(b, rt) => Ok(Value::Bool(b, prov_join(&lt, &rt))),
                                _ => {
                                    err(Some("E-rt-type"), format!("{op} 右侧要 Bool"), right.span)
                                }
                            }
                        }
                        _ => err(Some("E-rt-type"), format!("{op} 左侧要 Bool"), left.span),
                    };
                }
                let l = self.eval(left, env)?;
                let r = self.eval(right, env)?;
                self.binop(op, l, r, sp)
            }
            K::Call {
                callee,
                args: arguments,
            } => {
                // 语言形式与效应节点：被调用者是名字，与步 12c 前求值源码树里那个名字节点相同
                let f = match callee {
                    Callee::Name(n) => env_lookup(env, n).ok_or_else(|| {
                        Fault::Error(RtError::new(
                            Some("E-rt-name"),
                            format!("未定义的名字 {n}"),
                            view::callee_span(callee, e),
                        ))
                    })?,
                    Callee::Expr(c) => self.eval(c, env)?,
                };
                let mut args = Vec::with_capacity(arguments.len());
                for a in arguments {
                    args.push(self.eval(a, env)?);
                }
                self.apply(f, args, sp)
            }
        }
    }

    /// 二元运算。B33：输出 taint = ∨ 两侧（显式数据流）；列表拼接是搬运，元素保留自身的位。
    pub(in crate::interp) fn binop(&mut self, op: &str, l: Value, r: Value, sp: Span) -> R<Value> {
        // B84：输出标签 = 两侧 join（taint ∨ 同 B33，sources ∪）
        let t = prov_join(&l.prov(), &r.prov());
        let 搬运 = op == "+" && matches!((&l, &r), (Value::List(_), Value::List(_)));
        let v = self.binop_raw(op, l, r, sp)?;
        Ok(if 搬运 { v } else { v.with_prov(&t) })
    }

    pub(in crate::interp) fn binop_raw(
        &mut self,
        op: &str,
        l: Value,
        r: Value,
        sp: Span,
    ) -> R<Value> {
        if matches!(l, Value::Reading(_)) || matches!(r, Value::Reading(_)) {
            return err(
                Some("J-01"),
                format!("读数不能做 {op}：读数不可比、不可算，只能经 cut 离开"),
                sp,
            );
        }
        use Value::*;
        Ok(match (op, &l, &r) {
            // 13 §6：整数行为不随 Rust 构建模式改变。溢出与除零一律是**指向 .jpp 源码的运行错误**，
            // 不是 debug 崩溃 / release 悄悄回绕。用 checked_* 表达，两种构建下同一规则。
            ("+", Int(a, _), Int(b, _)) => Int(
                a.checked_add(*b)
                    .ok_or_else(|| overflow("加法", *a, *b, sp))?,
                Taint::Trusted.into(),
            ),
            ("-", Int(a, _), Int(b, _)) => Int(
                a.checked_sub(*b)
                    .ok_or_else(|| overflow("减法", *a, *b, sp))?,
                Taint::Trusted.into(),
            ),
            ("*", Int(a, _), Int(b, _)) => Int(
                a.checked_mul(*b)
                    .ok_or_else(|| overflow("乘法", *a, *b, sp))?,
                Taint::Trusted.into(),
            ),
            ("/", Int(a, _), Int(b, _)) => {
                if *b == 0 {
                    return err(Some("E-rt-int"), "除以零：Int 除法的除数不能是 0", sp);
                }
                Int(
                    a.checked_div(*b)
                        .ok_or_else(|| overflow("除法", *a, *b, sp))?,
                    Taint::Trusted.into(),
                )
            }
            ("%", Int(a, _), Int(b, _)) => {
                if *b == 0 {
                    return err(Some("E-rt-int"), "取模零：Int 取模的除数不能是 0", sp);
                }
                Int(
                    a.checked_rem(*b)
                        .ok_or_else(|| overflow("取模", *a, *b, sp))?,
                    Taint::Trusted.into(),
                )
            }
            ("+", Float(a, _), Float(b, _)) => Float(a + b, Taint::Trusted.into()),
            ("-", Float(a, _), Float(b, _)) => Float(a - b, Taint::Trusted.into()),
            ("*", Float(a, _), Float(b, _)) => Float(a * b, Taint::Trusted.into()),
            ("/", Float(a, _), Float(b, _)) => Float(a / b, Taint::Trusted.into()),
            ("+", Int(a, _), Float(b, _)) | ("+", Float(b, _), Int(a, _)) => {
                Float(*a as f64 + b, Taint::Trusted.into())
            }
            ("*", Int(a, _), Float(b, _)) | ("*", Float(b, _), Int(a, _)) => {
                Float(*a as f64 * b, Taint::Trusted.into())
            }
            ("-", Int(a, _), Float(b, _)) => Float(*a as f64 - b, Taint::Trusted.into()),
            ("-", Float(a, _), Int(b, _)) => Float(a - *b as f64, Taint::Trusted.into()),
            ("+", Text(a, _), Text(b, _)) => Value::text(&format!("{a}{b}")),
            ("+", List(a), List(b)) => Value::list(a.iter().chain(b.iter()).cloned().collect()),
            ("<", Int(a, _), Int(b, _)) => Bool(a < b, Taint::Trusted.into()),
            ("<=", Int(a, _), Int(b, _)) => Bool(a <= b, Taint::Trusted.into()),
            (">", Int(a, _), Int(b, _)) => Bool(a > b, Taint::Trusted.into()),
            (">=", Int(a, _), Int(b, _)) => Bool(a >= b, Taint::Trusted.into()),
            ("<", Float(a, _), Float(b, _)) => Bool(a < b, Taint::Trusted.into()),
            ("<=", Float(a, _), Float(b, _)) => Bool(a <= b, Taint::Trusted.into()),
            (">", Float(a, _), Float(b, _)) => Bool(a > b, Taint::Trusted.into()),
            (">=", Float(a, _), Float(b, _)) => Bool(a >= b, Taint::Trusted.into()),
            // `equals` 返回 None = 里面有读数，不可比（J-01）。这里以前是 `unwrap_or(false)`，
            // 把「不可比」这个信号吃成了「不相等」——顶上那道 J-01 只拦裸读数，
            // 装进列表或记录就从这条缝里漏过去了。
            ("==", _, _) | ("!=", _, _) => match l.equals(&r) {
                Some(eq) => Bool(if op == "==" { eq } else { !eq }, Taint::Trusted.into()),
                None => {
                    return err(
                        Some("J-01"),
                        format!(
                            "读数不能做 {op}：读数没有可读的值，装进列表或记录也一样。修法：先 cut 成出口再比出口"
                        ),
                        sp,
                    );
                }
            },
            _ => {
                return err(
                    Some("E-rt-type"),
                    format!("二元 {op} 不适用于 {} 与 {}", l.type_name(), r.type_name()),
                    sp,
                );
            }
        })
    }

    pub(in crate::interp) fn apply(&mut self, f: Value, args: Vec<Value>, sp: Span) -> R<Value> {
        match f {
            Value::Fn(c) => self.call_closure(&c, args, sp),
            Value::Builtin(name) => {
                // B33 第 3 点：内置输出 taint = ∨ 输入，在分派处一处统一算。
                // 效应边界与自带规则的内置（按 §2.11 表赋值）、以及只搬运元素的内置不在此列。
                // B84：推广为来源标签的 join（taint ∨ 与 B33 相同，sources ∪），豁免表不变
                let t = if matches!(name, "test" | "select" | "measure" | "fill") {
                    // 文本 → 题面（B84 表「fill ∪ 填入值」；test/select/measure 用计算出的文本造题同一条边，
                    // 解释登记见过程记录 17c）：题的来源并入实参的 sources；taint 不在此并（题面 taint 是 B58/17b）
                    Provenance::sources_only(
                        args.iter()
                            .fold(Provenance::trusted(), |t, a| prov_join(&t, &a.prov()))
                            .sources,
                    )
                } else if 不做数据流合取的内置.contains(&name) {
                    Provenance::trusted()
                } else {
                    args.iter()
                        .fold(Provenance::trusted(), |t, a| prov_join(&t, &a.prov()))
                };
                Ok(self.builtin(name, args, sp)?.with_prov(&t))
            }
            other => err(
                Some("E-rt-name"),
                format!("{} 不可调用", other.type_name()),
                sp,
            ),
        }
    }

    pub(in crate::interp) fn call_closure(
        &mut self,
        c: &Rc<Closure>,
        args: Vec<Value>,
        sp: Span,
    ) -> R<Value> {
        let f = &c.function;
        if args.len() != f.parameters.len() {
            return err(
                Some("E-rt-arity"),
                format!(
                    "{} 需要 {} 个参数，收到 {}",
                    c.name.as_deref().unwrap_or("函数"),
                    f.parameters.len(),
                    args.len()
                ),
                sp,
            );
        }
        let max_depth = self.budget.depth.unwrap_or(DEFAULT_DEPTH);
        if self.depth >= max_depth {
            return err(
                Some("J-06"),
                format!(
                    "调用深度超过 {max_depth}（递归无界）。修法：用 loop(bound, …) 或提高 budget.depth"
                ),
                sp,
            );
        }
        self.depth += 1;
        let env = env_child(&c.env);
        for (p, a) in f.parameters.iter().zip(args) {
            env_define(&env, &p.name, a);
        }
        let returns_exit = f
            .result_type
            .as_ref()
            .map(|t| t.mentions("Exit"))
            .unwrap_or(false);
        self.frames.push(Frame {
            name: c.name.clone().unwrap_or_else(|| "<fn>".into()),
            exits: vec![],
            returns_exit,
        });
        let result = self.eval_block(&f.body, &env);
        let frame = self.frames.pop().unwrap();
        // J-08：这一帧里产生过的出口（含已被 handle 消费的）的来源，带回调用方——
        // 否则 `fn 问人(m) { handle(ask(…), …) }` 这类 helper 一返回，「经 ask」就丢了。
        if !frame.exits.is_empty() {
            let prov = frame.exits.iter().fold(
                self.last_eval_provenance.unwrap_or((false, false)),
                |acc, x| (acc.0 || x.guard_trusted(), acc.1 || x.from_ask.get()),
            );
            self.last_eval_provenance = Some(prov);
        }
        self.depth -= 1;
        let v = result?;
        let mut in_value = HashSet::new();
        collect_exit_ids(&v, &mut in_value);
        // 13 §3 的两个案例，粒度在中间——不是都放过，也不是都拦下：
        //   1. 责任**没有**出现在返回值里 = 最后一份承接信息被丢了（取字段、过滤、切片扔掉了它）→ **错**。
        //   2. 责任**如实出现在返回值里**、只是返回类型没提 Exit → **警告**，报文直接给修法。
        //      它是标注缺失，不是责任丢失；责任继续往上挂，由调用者或程序结束前的检查接着核。
        //      样例的返回类型补齐后这一条升为错（见 INTERFACE.md §七）。
        for e in frame
            .exits
            .into_iter()
            .filter(|e| e.is_unsure() && !e.consumed.get())
        {
            if in_value.contains(&e.id) {
                if !frame.returns_exit {
                    self.trace.warn(format!(
                        "W-untyped-transfer: {} 把 {} 装在返回值里交了出去，但返回类型没提 Exit，调用者从签名上看不出自己收到了一份未决。修法：把返回类型标为含 Exit（如 `-> Exit`、`-> Record<Exit>`）",
                        frame.name,
                        e.label()
                    ));
                }
                *e.consumed_by.borrow_mut() = format!("return_type:{}", frame.name);
                self.frame().exits.push(e);
            } else {
                return err(
                    Some("J-05"),
                    format!(
                        "{} 返回前有未消费的 {}，而且它没出现在返回值里——最后一份承接信息被丢掉了。修法：在函数内 handle/consume，或把它放进返回值并把返回类型标为含 Exit",
                        frame.name,
                        e.label()
                    ),
                    e.site,
                );
            }
        }
        Ok(v)
    }

    pub(in crate::interp) fn loop_(
        &mut self,
        bound: i64,
        init: Value,
        step: &Value,
        sp: Span,
    ) -> R<Value> {
        if bound <= 0 {
            return err(
                Some("J-06"),
                format!("loop 的 bound 必须是正整数，收到 {bound}"),
                sp,
            );
        }
        let Value::Fn(step) = step else {
            return err(
                Some("E-rt-arg"),
                "loop(bound, init, step) 的 step 要是函数 fn(acc, i)",
                sp,
            );
        };
        self.loops.push(LoopCtx {
            seen_keys: HashSet::new(),
            repeated: None,
        });
        let mut acc = init;
        let mut result = None;
        for i in 0..bound {
            let out = self.call_closure(
                step,
                vec![acc.clone(), Value::Int(i, Taint::Trusted.into())],
                sp,
            );
            let out = match out {
                Ok(v) => v,
                Err(e) => {
                    self.loops.pop();
                    return Err(e);
                }
            };
            match out {
                Value::Stop(v) => {
                    result = Some((*v).clone());
                    break;
                }
                v => acc = v,
            }
            if let Some(k) = self.loops.last().and_then(|l| l.repeated.clone()) {
                self.trace.warn(format!(
                    "W-noprogress: 第 {} 轮重复了账本键 {}，循环停止（J-06 键重复即停）",
                    i + 1,
                    头(&k, 8)
                ));
                break;
            }
        }
        self.loops.pop();
        if result.is_none() {
            self.trace
                .warn(format!("W-bound: loop 到 bound={bound} 仍未 stop"));
        }
        Ok(result.unwrap_or(acc))
    }

    // ---------- 材料与状态 ----------

    pub(in crate::interp) fn as_mat(&self, v: &Value, slot: &str, sp: Span) -> R<Mat> {
        match v {
            Value::Mat(m) => Ok((**m).clone()),
            Value::Reading(_) => err(
                Some("J-01"),
                format!("读数不能放进 {slot} 槽：读数只能经 cut 离开，不是材料"),
                sp,
            ),
            Value::Exit(e) => {
                let mut d = BTreeSet::new();
                d.insert(e.q_hash.clone());
                // B59（步 17a）：出口转材料，来源 = 出口的账本键
                Ok(Mat::new(
                    json!({"exit": e.label()}),
                    "",
                    vec![format!("exit:{}", e.q_hash)],
                    e.taint,
                    d,
                )
                .with_from_key([e.ledger_key.borrow().clone()]))
            }
            Value::Duty(_) => err(
                Some("J-05"),
                format!(
                    "未决责任不能直接当材料放进 {slot}：变成材料或 JSON 不消除义务。修法：先 literalize(u, …) 重问，或把 u 包进返回值"
                ),
                sp,
            ),
            Value::State(_)
            | Value::Question(_)
            | Value::Fn(_)
            | Value::Builtin(_)
            | Value::Stop(_) => err(
                Some("E-rt-arg"),
                format!("{} 不能作材料", v.type_name()),
                sp,
            ),
            // B84：失败值转材料，来源随之
            Value::Fail(s, t) => Ok(Mat::new(
                json!({"fail": s.as_ref()}),
                "",
                vec!["fail".into()],
                t.taint,
                BTreeSet::new(),
            )
            .with_from_key(t.sources.iter().cloned())),
            // 计算值进材料（B33 第 5 点）：taint = 值自身的位（容器递归 ∨）。语法字面量求值即 trusted，
            // 所以不需要「字面量兜底」那一臂；成分含不可信内容的计算值 origin 记 computed。
            // trusted 的计算值 origin 仍记 literal：材料哈希不含 origin，但输出里的 origin 保持不变。
            other => {
                // B84：计算值的来源标签（taint 分量与现行 `other.taint()` 逐值相同）；
                // 元素记录的出口、叶子带的来源读数都在 `prov` 里（17a 的结构通道是它的特例）
                let p = other.prov();
                let t = p.taint;
                let origin = if t == Taint::Untrusted {
                    "computed"
                } else {
                    "literal"
                };
                Ok(
                    Mat::new(other.to_json(), "", vec![origin.into()], t, BTreeSet::new())
                        .with_from_key(p.sources.iter().cloned()),
                )
            }
        }
    }

    pub(in crate::interp) fn as_mats(
        &self,
        v: &Value,
        slot: &str,
        sp: Span,
    ) -> R<(Vec<Mat>, bool)> {
        let items: Vec<Value> = match v {
            Value::List(l) => l.iter().cloned().collect(),
            other => vec![other.clone()],
        };
        let has_fail = items.iter().any(|x| matches!(x, Value::Fail(..)));
        let mut out = vec![];
        for it in items {
            out.push(self.as_mat(&it, slot, sp)?);
        }
        Ok((out, has_fail))
    }

    pub(in crate::interp) fn make_state(&self, args: &[Value], sp: Span) -> R<Value> {
        if args.is_empty() || args.len() > 2 {
            return err(
                Some("E-rt-arg"),
                "state(on) 或 state(on, {ctx: […], ref: […], over: […]})",
                sp,
            );
        }
        let (on, f1) = self.as_mats(&args[0], "on", sp)?;
        if on.is_empty() || on.len() > 2 {
            return err(
                Some("J-14"),
                format!("on 恰一个判断对象（或一对），收到 {}", on.len()),
                sp,
            );
        }
        let mut ctx = vec![];
        let mut r#ref = vec![];
        let mut over = vec![];
        let mut fail = f1;
        if let Some(opts) = args.get(1) {
            if !matches!(opts, Value::Record(_)) {
                return err(
                    Some("E-rt-arg"),
                    "state 的第二个参数是记录 {ctx, ref, over}",
                    sp,
                );
            }
            for (k, target) in [("ctx", &mut ctx), ("ref", &mut r#ref), ("over", &mut over)] {
                if let Some(v) = opts.get(k) {
                    let (ms, f) = self.as_mats(&v, k, sp)?;
                    fail |= f;
                    *target = ms;
                }
            }
        }
        let st = State::new(on, ctx, r#ref, over, fail);
        // J-08 诊断（B33 第 8 点）：记下哪些状态含「成分不可信的计算值」材料
        if [&st.on, &st.ctx, &st.r#ref, &st.over].iter().any(|ms| {
            ms.iter()
                .any(|m| m.taint == Taint::Untrusted && m.origin.iter().any(|o| o == "computed"))
        }) {
            self.computed_untrusted_states
                .borrow_mut()
                .insert(st.hash.clone());
        }
        Ok(Value::State(Rc::new(st)))
    }

    /// 捕获状态的指纹（13 §4）。`None` = 这个环境**指纹化不了**，调用方应当禁用跨运行缓存。
    ///
    /// 不无限展开：嵌套方法到 `depth` 就返回 `None`；捕获里有读数、出口、未决责任时也返回 `None`
    /// ——它们要么没有可读的值，要么带着尚未了结的义务，不该参与「结果可复用」的判断。
    pub(in crate::interp) fn env_fingerprint(
        &self,
        env: &Env,
        names: &BTreeSet<String>,
        depth: u32,
    ) -> Option<String> {
        let mut parts: Vec<String> = vec![];
        for n in names {
            let Some(v) = env_lookup(env, n) else {
                continue;
            };
            match &v {
                // 内置名稳定，不进指纹
                Value::Builtin(_) => continue,
                Value::Fn(c) => {
                    if depth == 0 {
                        return None;
                    }
                    let inner =
                        self.env_fingerprint(&c.env, &referenced_names(&c.function), depth - 1)?;
                    parts.push(format!("{n}=fn:{}:{inner}", c.hash));
                }
                Value::Reading(_)
                | Value::Exit(_)
                | Value::Duty(_)
                | Value::State(_)
                | Value::Question(_) => return None,
                other => parts.push(format!("{n}={}", canon(&other.to_json()))),
            }
        }
        Some(hash_of(
            &parts.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        ))
    }
}
