//! 求值器：块、表达式、闭包、二元运算与整数边界、`loop`、材料与状态的构造（20 §2.3 `eval.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;

impl<'a> Interp<'a> {
    pub(crate) fn frame(&mut self) -> &mut Frame {
        self.frames.last_mut().unwrap()
    }

    // ---------- 求值 ----------

    pub(crate) fn eval_block(&mut self, b: &Block, env: &Env) -> R<Value> {
        let env = env_child(env);
        // 伴随题（B0492 S5）：第一次进块就是程序体，记下它的环境（`lib/unsure.jpp` 的 `unsure_companions` 在这里）
        if self.顶层环境.is_none() && self.frames.len() == 1 {
            self.顶层环境 = Some(env.clone());
        }
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
                    // 直线段提升穿过函数调用（B94 下半，步 23c）：本句起的直线段里，同一状态的多次
                    // judge（含包在用户函数里的）先登记，本句里第一次检视时一次发出
                    self.提升过调用(b, value.id, &env);
                    // J-08 的守卫证据随值走（`GuardEv`，B121）：`let` 只绑值，不另记来源。
                    let v = self.eval(value, &env)?;
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

    pub(crate) fn closure(
        &self,
        f: &Function,
        env: &Env,
        name: Option<String>,
        span: Span,
    ) -> Value {
        // 方法身份：降级时按源码函数算好（口径与步 12c 前相同，账本键不变）
        let hash = f.source_hash.clone();
        // B52（步 21）：记下创建时经捕获环境可达的未决责任；是不是 Fn¹ 由创建它的帧返回时判
        let captures = captured_duties(f, env);
        Value::Fn(Rc::new(Closure {
            function: f.clone(),
            env: env.clone(),
            name,
            span,
            hash,
            captures,
            linear: std::cell::RefCell::new(vec![]),
            linear_called: std::cell::Cell::new(false),
        }))
    }

    pub(crate) fn eval(&mut self, e: &Expr, env: &Env) -> R<Value> {
        let sp = e.span;
        match kind(e) {
            K::Integer(i) => Ok(Value::Int(i, Taint::Trusted.into())),
            K::Decimal(d) => Ok(Value::Float(d, Taint::Trusted.into())),
            K::Bool(b) => Ok(Value::Bool(b, Taint::Trusted.into(), GuardEv::EMPTY)),
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
                // 字段值各带各的守卫证据（`GuardEv` 随 `Bool` 叶子走，B121），不另记逐字段来源
                let mut v = Vec::with_capacity(fields.len());
                for (k, it) in fields {
                    v.push((k.clone(), self.eval(it, env)?));
                }
                Ok(Value::record(v))
            }
            K::Function(f) => Ok(self.closure(f, env, None, sp)),
            K::Block(b) => self.eval_block(b, env),
            K::If { condition, yes, no } => {
                // 检视点（B94）：条件里的惰性出口先解析
                let c = self.eval(condition, env)?;
                let c = self.检视(c)?;
                // 未决值传播（B0492 S3）：条件未决，两支都不执行，表达式的值是它
                if let Value::Duty(_) = c {
                    return Ok(c);
                }
                // 刷新点：分支要在已知信息上走，不能让未发出的判断跨过分支边界（12 §2.2:129）
                self.flush("if")?;
                match c {
                    // J-08：这层守卫就是条件值自带的证据（B121）；分支值不带条件的证据
                    // （控制流不传播，B33 第 3 条；B121-1 (b)）
                    Value::Bool(b, _, g) => {
                        self.guards.push(g);
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
                // 检视点（B94）：取惰性出口的字段先解析
                let v = if matches!(v, Value::Cut(_) | Value::Gen(_)) {
                    self.检视(v)?
                } else {
                    v
                };
                // 未决值传播（B0492 S3）：取未决值的字段，结果是它
                if let Value::Duty(_) = v {
                    return Ok(v);
                }
                match &v {
                    Value::Record(_) => {
                        let 拟合中 = self.拟合中 > 0;
                        v.get(field).ok_or_else(|| {
                            // 声明式拟合的统计量记录（B154，步 20j-4）：没有 confidence 是判断器没报，报文说出来
                            let 说明 = if 拟合中 && field == "confidence" {
                                "：这条读数的判断器没有随答案报 confidence（画像 H9；固定观察按夹具给或缺省 p_max，B154）".to_string()
                            } else {
                                String::new()
                            };
                            Fault::Error(RtError::new(
                                Some("E-rt-field"),
                                format!("记录没有字段 {field}{说明}"),
                                sp,
                            ))
                        })
                    }
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
                // 检视点（B94）：下标本身是惰性出口时先解析（列表里的元素原样取出，不解析）
                let i = self.检视(i)?;
                let v = if matches!(v, Value::Cut(_) | Value::Gen(_)) {
                    self.检视(v)?
                } else {
                    v
                };
                // 未决值传播（B0492 S3）：值或下标未决，结果是它
                let ds: Vec<Rc<Exit>> = [&v, &i]
                    .into_iter()
                    .filter_map(crate::undecided::未决)
                    .collect();
                if !ds.is_empty() {
                    return Ok(self.合并未决(ds, sp));
                }
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
                            .with_prov(&Provenance::sources_only(i.prov().sources.as_select())))
                    }
                    (Value::Record(_), Value::Text(k, _)) => v
                        .get(k)
                        .map(|x| {
                            x.with_prov(&Provenance::sources_only(i.prov().sources.as_select()))
                        })
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
                let v = self.检视(v)?;
                // 未决值传播（B0492 S3）
                if let Value::Duty(_) = v {
                    return Ok(v);
                }
                // B84：一元运算输出带操作数的标签（taint 同 B33）
                let t = v.prov();
                match (op, &v) {
                    // J-08：`!` 清空守卫证据（`20` v2 §3.1）
                    ("!", Value::Bool(b, _, _)) => Ok(Value::Bool(!b, t, GuardEv::EMPTY)),
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
                    let l = self.检视(l)?;
                    // 未决值传播（B0492 S3）：强 Kleene。左侧未决时右侧照求：`U && false = false`、`U || true = true`，
                    // 右侧是另一边的真值元（`U && true`、`U || false`）时结果是 U，两侧都未决则合并
                    if let Value::Duty(ld) = &l {
                        let ld = ld.clone();
                        let r = self.eval(right, env)?;
                        let r = self.检视(r)?;
                        return match (op, &r) {
                            ("&&", Value::Bool(false, t, _)) => {
                                Ok(Value::Bool(false, t.clone(), GuardEv::EMPTY))
                            }
                            ("||", Value::Bool(true, t, _)) => {
                                Ok(Value::Bool(true, t.clone(), GuardEv::EMPTY))
                            }
                            (_, Value::Bool(..)) => Ok(l),
                            (_, Value::Duty(rd)) => Ok(self.合并未决(vec![ld, rd.clone()], sp)),
                            _ => err(Some("E-rt-type"), format!("{op} 右侧要 Bool"), right.span),
                        };
                    }
                    return match (op, &l) {
                        // J-08 守卫证据（`20` v2 §3.1）：`&&` 取两侧的并，`||` 清空。
                        // 短路的 `false` 不放行任何东西，证据清空；短路的 `true ||` 同样清空。
                        ("&&", Value::Bool(false, t, _)) => {
                            Ok(Value::Bool(false, t.clone(), GuardEv::EMPTY))
                        }
                        ("||", Value::Bool(true, t, _)) => {
                            Ok(Value::Bool(true, t.clone(), GuardEv::EMPTY))
                        }
                        (_, Value::Bool(_, lt, lg)) => {
                            let (lt, lg) = (lt.clone(), *lg);
                            let r = self.eval(right, env)?;
                            let r = self.检视(r)?;
                            match r {
                                // 未决值传播（B0492 S3）：`true && U`、`false || U` 的结果是 U
                                Value::Duty(_) => Ok(r),
                                Value::Bool(b, rt, rg) => Ok(Value::Bool(
                                    b,
                                    prov_join(&lt, &rt),
                                    if op == "&&" {
                                        lg.join(rg)
                                    } else {
                                        GuardEv::EMPTY
                                    },
                                )),
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
                // 检视点（B94）：运算两侧的惰性出口先解析
                let (l, r) = (self.检视(l)?, self.检视(r)?);
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
    pub(crate) fn binop(&mut self, op: &str, l: Value, r: Value, sp: Span) -> R<Value> {
        // 未决值传播（B0492 S3）：比较、算术、拼接依赖两侧内容，任一侧未决结果就是它，两侧都未决则合并
        let ds: Vec<Rc<Exit>> = [&l, &r]
            .into_iter()
            .filter_map(crate::undecided::未决)
            .collect();
        if !ds.is_empty() {
            return Ok(self.合并未决(ds, sp));
        }
        // B84：输出标签 = 两侧 join（taint ∨ 同 B33，sources ∪）
        let t = prov_join(&l.prov(), &r.prov());
        let 搬运 = op == "+" && matches!((&l, &r), (Value::List(_), Value::List(_)));
        let v = self.binop_raw(op, l, r, sp)?;
        Ok(if 搬运 { v } else { v.with_prov(&t) })
    }

    pub(crate) fn binop_raw(&mut self, op: &str, l: Value, r: Value, sp: Span) -> R<Value> {
        if matches!(l, Value::Reading(_)) || matches!(r, Value::Reading(_)) {
            return err(
                Some("J-01"),
                format!("读数不能做 {op}：读数不可比、不可算，只能经 cut 离开"),
                sp,
            );
        }
        // 依据：B153 (2)（Score 不能读出为数；J-01 型面不变）
        if matches!(l, Value::Score(_)) || matches!(r, Value::Score(_)) {
            return err(
                Some("J-01"),
                format!(
                    "声明式拟合的结果（Score）不能做 {op}：它不是数，只能进 cut 的声明线或同一拟合的 order（B153）。要换算，把换算写进 declare 的闭包"
                ),
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
            // B70（21 步 14c）：Int 与 Float 混用一律提升为 Float，`/` 与比较同 `+ - *` 一条规则；
            // 整数间运算（整数除法、溢出、除零）仍走上面的 Int 臂（13 §6）
            ("/", Int(a, _), Float(b, _)) => Float(*a as f64 / b, Taint::Trusted.into()),
            ("/", Float(a, _), Int(b, _)) => Float(a / *b as f64, Taint::Trusted.into()),
            ("<" | "<=" | ">" | ">=", Int(_, _), Float(_, _))
            | ("<" | "<=" | ">" | ">=", Float(_, _), Int(_, _)) => {
                let as_f = |v: &Value| match v {
                    Int(i, _) => *i as f64,
                    Float(f, _) => *f,
                    _ => unreachable!(),
                };
                let (a, b) = (as_f(&l), as_f(&r));
                let res = match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    _ => a >= b,
                };
                Bool(res, Taint::Trusted.into(), GuardEv::EMPTY)
            }
            ("+", Text(a, _), Text(b, _)) => Value::text(&format!("{a}{b}")),
            ("+", List(a), List(b)) => Value::list(a.iter().chain(b.iter()).cloned().collect()),
            ("<", Int(a, _), Int(b, _)) => Bool(a < b, Taint::Trusted.into(), GuardEv::EMPTY),
            ("<=", Int(a, _), Int(b, _)) => Bool(a <= b, Taint::Trusted.into(), GuardEv::EMPTY),
            (">", Int(a, _), Int(b, _)) => Bool(a > b, Taint::Trusted.into(), GuardEv::EMPTY),
            (">=", Int(a, _), Int(b, _)) => Bool(a >= b, Taint::Trusted.into(), GuardEv::EMPTY),
            ("<", Float(a, _), Float(b, _)) => Bool(a < b, Taint::Trusted.into(), GuardEv::EMPTY),
            ("<=", Float(a, _), Float(b, _)) => Bool(a <= b, Taint::Trusted.into(), GuardEv::EMPTY),
            (">", Float(a, _), Float(b, _)) => Bool(a > b, Taint::Trusted.into(), GuardEv::EMPTY),
            (">=", Float(a, _), Float(b, _)) => Bool(a >= b, Taint::Trusted.into(), GuardEv::EMPTY),
            // `equals` 返回 None = 里面有读数，不可比（J-01）。这里以前是 `unwrap_or(false)`，
            // 把「不可比」这个信号吃成了「不相等」——顶上那道 J-01 只拦裸读数，
            // 装进列表或记录就从这条缝里漏过去了。
            ("==", _, _) | ("!=", _, _) => match l.equals(&r) {
                Some(eq) => Bool(
                    if op == "==" { eq } else { !eq },
                    Taint::Trusted.into(),
                    GuardEv::EMPTY,
                ),
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

    pub(crate) fn apply(&mut self, f: Value, args: Vec<Value>, sp: Span) -> R<Value> {
        match f {
            Value::Fn(c) => self.call_closure(&c, args, sp),
            Value::Builtin(name) => {
                // 检视点（B94，步 23c）：内置与构造的实参里的惰性出口先解析——它们要读出口种类、
                // 把出口装进材料或契约值；列表与记录里的也解析（依据：B94）
                let args = args
                    .into_iter()
                    .map(|a| self.检视(a))
                    .collect::<R<Vec<Value>>>()?;
                // B33 第 3 点：内置输出 taint = ∨ 输入，在分派处一处统一算。
                // 效应边界与自带规则的内置（按 §2.11 表赋值）、以及只搬运元素的内置不在此列。
                // B84：推广为来源标签的 join（taint ∨ 与 B33 相同，sources ∪），豁免表不变
                let t = if matches!(name, "test" | "select" | "measure" | "fill" | "form") {
                    // 文本 → 题面（B84 表「fill ∪ 填入值」；test/select/measure 用计算出的文本造题同一条边，
                    // 解释登记见过程记录 17c）：题的来源并入实参的 sources。题面 taint（B58，步 17b）同一处并：
                    // 题 taint = 填入值、计算出的题面文本与题式模板的 taint 之 ∨，字面题为 Trusted
                    // （`form` 的模板 taint 是 17b 的解释登记 (a)）。依据：B58、B84
                    args.iter()
                        .fold(Provenance::trusted(), |t, a| prov_join(&t, &a.prov()))
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

    /// 声明式拟合的闭包在桥内求值（B153 (2)，步 20j-4）：只见参数与内置——以一份只含内置的新根环境调它，
    /// 不接它自己捕获的环境；求值期间效应、内核构造、出口与责任形式、读答案的刷新点一律拒绝
    /// （`host_builtins.rs::builtin` 入口，`E-fit-declare-effect`）。返回值须是有限的数。
    /// 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一）
    pub(crate) fn 拟合闭包求值(
        &mut self,
        c: &Rc<Closure>,
        args: Vec<Value>,
        sp: Span,
    ) -> R<f64> {
        let 隔离 = Rc::new(Closure {
            function: c.function.clone(),
            env: root_env(),
            name: c.name.clone(),
            span: c.span,
            hash: c.hash.clone(),
            captures: vec![],
            linear: std::cell::RefCell::new(vec![]),
            linear_called: std::cell::Cell::new(false),
        });
        self.拟合中 += 1;
        let r = self.call_closure(&隔离, args, sp);
        self.拟合中 -= 1;
        match r? {
            Value::Int(i, _) => Ok(i as f64),
            Value::Float(f, _) if f.is_finite() => Ok(f),
            other => err(
                Some("E-rt-type"),
                format!(
                    "声明式拟合的闭包要返回一个有限的数，收到 {}（{}）",
                    other.type_name(),
                    other.to_json()
                ),
                sp,
            ),
        }
    }

    /// 闭包调用的本体（C2c 起由 `cells.rs::call_closure` 包一层：成代码单元的调用经单元图）。
    pub(crate) fn call_closure_body(
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
        // B52（步 21）：Fn¹ 只能调用一次。第一次调用里责任按实际去向处置（销账或随结果转交）；
        // 再调用就是把同一份责任处置第二次。依据：B52、`13` §3「捕获未决的方法能否重复调用，
        // 必须以责任转移规则明确处理」
        if !c.linear.borrow().is_empty() {
            if c.linear_called.get() {
                return err(
                    Some("J-05"),
                    format!(
                        "{} 是 Fn¹（它捕获的未决责任 {} 只经它可达），已经调用过一次：那次调用里责任已按实际去向处置，再调用会把同一份责任处置两次。修法：第一次调用的结果里带着责任就用那个结果；要多次续判，让创建它的函数把未决清单也放进返回值（如 {{pending: …, resume: fn …}}），它就不再是唯一路径",
                        c.name.as_deref().unwrap_or("这个方法"),
                        c.linear.borrow().len()
                    ),
                    sp,
                );
            }
            c.linear_called.set(true);
        }
        // G4b（裁定六十四、Z0525）：递归只受程序声明的 budget.depth 限（没声明取引擎默认），不被余额的 depth_cap 收紧
        let max_depth = self.递归上限();
        if self.depth >= max_depth {
            // C-3：上限被上游收紧时报文说清（主控答复第 3 条 (乙)），否则与改前逐字相同
            return err(Some("J-06"), self.深度超限报文(max_depth), sp);
        }
        self.depth += 1;
        self.深度峰 = self.深度峰.max(self.depth);
        let 名 = c.name.clone().unwrap_or_else(|| "<fn>".into());
        // G2 附录三（Z0564）：欠账记号的帧主人带实参哈希，同一函数不同实参的两次调用记号不同
        let 主人 = format!("{名}#{}", crate::violation::实参哈希(&args));
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
            name: 名,
            exits: vec![],
            cuts: vec![],
            returns_exit,
            过桥: 0,
            主人,
        });
        let result = self.eval_block(&f.body, &env);
        // 帧返回前解析本帧的惰性出口（B94）：J-05 与 Fn¹ 按出口种类核
        let result = result.and_then(|v| self.解析本帧().map(|_| v));
        let frame = self.frames.pop().unwrap();
        self.depth -= 1;
        let v = result?;
        let mut in_value = HashSet::new();
        collect_exit_ids(&v, &mut in_value);
        // 13 §3 的两个案例，粒度在中间——不是都放过，也不是都拦下：
        //   1. 责任**没有**出现在返回值里 = 最后一份承接信息被丢了（取字段、过滤、切片扔掉了它）→ **错**。
        //   2. 责任**如实出现在返回值里**、只是返回类型没提 Exit → **警告**，报文直接给修法。
        //      它是标注缺失，不是责任丢失；责任继续往上挂，由调用者或程序结束前的检查接着核。
        //      样例的返回类型补齐后这一条升为错（见 INTERFACE.md §七）。
        // B52：随返回值交出的责任里，只经一个闭包可达的，那个闭包就是它的唯一路径（Fn¹）
        let mut transferred: Vec<Rc<Exit>> = vec![];
        // B162：同一判断的另一个持有者在返回值里、或这份责任的键已解除，也算有去向（一份责任的多个视图）
        let 值键 = crate::duty::值里的键(&v);
        for e in frame
            .exits
            .into_iter()
            .filter(|e| e.is_unsure() && !e.consumed.get())
        {
            if !in_value.contains(&e.id) {
                match self.键的去向(&e, &值键) {
                    // 另一个持有者已消费：这份是同一责任的视图，已解除
                    Some(true) => {
                        *e.consumed_by.borrow_mut() = "view:已解除".into();
                        e.consumed.set(true);
                        continue;
                    }
                    // 同键的持有者在返回值里：随返回值交出，继续挂在调用者那一帧，由上层与程序结束前接着核
                    Some(false) => {
                        transferred.push(e.clone());
                        self.frame().exits.push(e);
                        continue;
                    }
                    None => {}
                }
            }
            if in_value.contains(&e.id) {
                // B115（A-12）：只对具名函数报；函数字面量没有调用者从签名读它，
                // 转移义务落在最近的具名函数或程序返回值上
                if !frame.returns_exit && c.name.is_some() {
                    self.trace.warn(format!(
                        "W-untyped-transfer: {} 把 {} 装在返回值里交了出去，但返回类型没提 Exit，调用者从签名上看不出自己收到了一份未决。修法：把返回类型标为含 Exit（如 `-> Exit`、`-> Record<Exit>`）",
                        frame.name,
                        e.label()
                    ));
                }
                *e.consumed_by.borrow_mut() = format!("return_type:{}", frame.name);
                transferred.push(e.clone());
                self.frame().exits.push(e);
            } else {
                // G2（`12` R9，主控定第 1 条）：函数返回前丢了、不在返回值里的未决不再当场报 J-05；这笔欠账挂到调用者那一帧、
                // 继续往上交，程序结束时还欠着才记违规（报文说明它最初在哪个函数返回前丢的）
                self.丢失处
                    .entry(e.id)
                    .or_insert_with(|| frame.name.clone());
                self.frame().exits.push(e);
            }
        }
        if !transferred.is_empty() {
            self.mark_fn1(&v, &transferred);
        }
        Ok(v)
    }

    /// B52：随返回值交出的责任里，不直接出现在返回值、只经**一个**闭包可达、且该闭包创建时就捕获了它的，
    /// 那个闭包记为 Fn¹。拿不准（经多个闭包、或闭包创建晚于责任可达）不记——不是唯一路径就不收紧。
    fn mark_fn1(&mut self, v: &Value, transferred: &[Rc<Exit>]) {
        let mut direct = HashSet::new();
        let mut via: HashMap<usize, Vec<Rc<Closure>>> = HashMap::new();
        exit_paths(v, &mut direct, &mut via);
        for e in transferred {
            if direct.contains(&e.id) {
                continue;
            }
            let Some([c]) = via.get(&e.id).map(|cs| cs.as_slice()) else {
                continue;
            };
            if !c.captures.contains(&e.id) {
                continue;
            }
            let mut lin = c.linear.borrow_mut();
            if !lin.contains(&e.id) {
                lin.push(e.id);
            }
            c.linear_called.set(false);
            self.fn1_of.insert(
                e.id,
                c.name.clone().unwrap_or_else(|| "（匿名方法）".into()),
            );
        }
    }

    /// J-05 的修法（B95）：先给转交写法，drop 排最后并带条件；责任唯一路径是 Fn¹ 闭包时先说明它
    pub(crate) fn j05_fix(&self, e: &Exit) -> String {
        let fn1 = match self.fn1_of.get(&e.id) {
            Some(n) => format!(
                "它唯一的路径是 Fn¹ 方法 {n}，而 {n} 既没被调用、也没被交出（或调用后又丢了结果）。"
            ),
            None => String::new(),
        };
        format!(
            "{fn1}修法：转交——把它放进返回值（来自契约值 o 的写 undecided(o) 与 unobserved(o)，或 o.pending；元素投影保留 exit 字段），具名函数的返回类型标为含 Exit；或 handle 它、在 unsure 臂 escalate(u, …) 交给人、literalize(u, …) 重问；consume(…, \"drop\") 排最后，只用于不进入任何输出、不参与路由的题"
        )
    }

    pub(crate) fn loop_(&mut self, bound: i64, init: Value, step: &Value, sp: Span) -> R<Value> {
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

    pub(crate) fn as_mat(&self, v: &Value, slot: &str, sp: Span) -> R<Mat> {
        match v {
            Value::Mat(m) => Ok((**m).clone()),
            Value::Reading(_) => err(
                Some("J-01"),
                format!("读数不能放进 {slot} 槽：读数只能经 cut 离开，不是材料"),
                sp,
            ),
            // 依据：B153 (2)（Score 不是材料）
            Value::Score(_) => err(
                Some("J-01"),
                format!(
                    "声明式拟合的结果（Score）不能放进 {slot} 槽：它只能经 cut 的声明线离开（B153）"
                ),
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
                .with_sources(&Sources::value(&e.ledger_key.borrow(), &e.q_hash)))
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
            .with_sources(&t.sources)),
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
                        .with_sources(&p.sources),
                )
            }
        }
    }

    pub(crate) fn as_mats(&self, v: &Value, slot: &str, sp: Span) -> R<(Vec<Mat>, bool)> {
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

    pub(crate) fn make_state(&self, args: &[Value], sp: Span) -> R<Value> {
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
        // E-modality（B126 守卫半，步 24b）：进 state 的材料模态不是文字时，画像 text_only 未测
        // 或为 true 即错（H1 未测按 true，B39）；text_only: false 时模态必须在 modalities_accepted
        // 里。静态面本版不做——字面 mat(…) 恒为文字、S 库渲染类 do 尚未落地（步 25），当前没有
        // 任何路径能让静态检查看到非文字模态；运行期这里已是全集，不产生覆盖缺口。
        if let Some(m) = [&st.on, &st.ctx, &st.r#ref, &st.over]
            .iter()
            .flat_map(|ms| ms.iter())
            .find(|m| m.modality != "text")
        {
            match self.calib.profile().text_only() {
                jpp_effects::Tri::未测 => {
                    return err(
                        Some("E-modality"),
                        format!(
                            "材料模态是 {}：画像没有测过 text_only（H1），未测按 true 处理，只收文字材料",
                            m.modality
                        ),
                        sp,
                    );
                }
                jpp_effects::Tri::真 => {
                    return err(
                        Some("E-modality"),
                        format!(
                            "材料模态是 {}：画像声明 text_only: true，只收文字材料",
                            m.modality
                        ),
                        sp,
                    );
                }
                jpp_effects::Tri::假 => {
                    let accepted = self.calib.profile().modalities_accepted();
                    if !accepted.is_some_and(|a| a.iter().any(|x| x == &m.modality)) {
                        return err(
                            Some("E-modality"),
                            format!(
                                "材料模态是 {}：画像 text_only: false，但 modalities_accepted 没有列出这个模态",
                                m.modality
                            ),
                            sp,
                        );
                    }
                }
            }
        }
        // J-08 诊断（B33 第 8 点）：记下哪些状态含「成分不可信的计算值」材料
        if [&st.on, &st.ctx, &st.r#ref, &st.over].iter().any(|ms| {
            ms.iter()
                .any(|m| m.taint == Taint::Untrusted && m.origin.iter().any(|o| o == "computed"))
        }) {
            self.computed_untrusted_states
                .borrow_mut()
                .insert(st.hash.clone());
        }
        // J-08 诊断（B105）：含「宿主入口、未声明可信」材料的状态，记下入口名。
        // 依据：B105 / B106（地基/附注/2026-09-25-B105-B106裁定.md §一·2）
        if let Some(name) = [&st.on, &st.ctx, &st.r#ref, &st.over]
            .iter()
            .flat_map(|ms| ms.iter())
            .filter(|m| m.taint == Taint::Untrusted && m.origin.iter().any(|o| o == "input"))
            .find_map(|m| self.entry_mat_names.get(&m.hash))
        {
            self.input_untrusted_states
                .borrow_mut()
                .insert(st.hash.clone(), name.clone());
        }
        Ok(Value::State(Rc::new(st)))
    }

    /// 捕获状态的指纹（13 §4）。`None` = 这个环境**指纹化不了**，调用方应当禁用跨运行缓存。
    ///
    /// 不无限展开：嵌套方法到 `depth` 就返回 `None`；捕获里有读数、出口、未决责任时也返回 `None`
    /// ——它们要么没有可读的值，要么带着尚未了结的义务，不该参与「结果可复用」的判断。
    pub(crate) fn env_fingerprint(
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
                // 未取回的生成（步 15h-2）没有指纹；取回后按结果算（下面 `other` 臂的 `to_json`）
                Value::Gen(g) if g.value().is_none() => return None,
                Value::Reading(_)
                | Value::Exit(_)
                | Value::Cut(_)
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
