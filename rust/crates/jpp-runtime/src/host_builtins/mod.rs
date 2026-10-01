//! 内置分派表与通用内置（列表、文本、出口读出等）；内核构造的臂在 `constructs/`（20 §2.3 `host_builtins.rs`）。
//! 步 4a 从 `interp.rs` 机械拆出，只搬代码、不改逻辑（21 §三·3）。

use super::*;
use jpp_effects::EffectSpec;
use jpp_value::bridge::DeclaredLine;
use jpp_value::stat::Stat;

// 步 36 G3：按内置的类拆成子模块（只搬不改）；`crate::host_builtins::{解析统计量, 解析策略, 核选项}` 路径照旧
mod collections;
mod cut_opts;
mod effects;
mod gates;
mod judge_cut;
mod misc;
mod unsure;

use cut_opts::*;
pub(crate) use cut_opts::{核选项, 解析策略, 解析统计量};

impl<'a> Interp<'a> {
    // ---------- 内置 ----------

    pub(crate) fn builtin(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
        // 声明式拟合的闭包体内（B153 (2)，步 20j-4）：效应、内核构造、出口与责任形式、读答案的刷新点一律拒绝。
        // 依据：B153 (2)（地基/附注/2026-09-26-批6裁定.md §一：体内不得出现效应、构造或刷新点）
        if self.拟合中 > 0 && 拟合内禁(name) {
            return err(
                Some("E-fit-declare-effect"),
                format!(
                    "声明式拟合（或 cut 的 feasible）的闭包里不能调用 {name}：闭包在桥内求值，只做计算（算术、比较、if、max / min 等纯内置），不发判断、不调效应、不跑构造、不造出口（B153）。修法：把 {name} 移到 fit 之外，结果经 extra 传入"
                ),
                sp,
            );
        }
        // 未决值传播（B0492 S3）：按值计算的内置与效应，实参里有未决值时结果就是它（效应不发出、账本记 Skip）
        if let Some(v) = self.未决实参(name, &args, sp) {
            return Ok(v);
        }
        // 效应经注册表取 `EffectSpec`，按字段分派（步 15a，`20` A2），不在本表按效应名分支
        if let Some(s) = jpp_effects::by_name(name) {
            return self.effect_builtin(s, name, args, sp);
        }
        // 内核构造经注册表分派：按 `ConstructSpec.privileges` 造能力令牌再调实现（B57，步 25-2）
        if let Some(c) = crate::caps::construct(name) {
            return self.call_construct(c, name, args, sp);
        }
        match name {
            "state" => self.make_state(&args, sp),
            "cut" => self.b_cut(name, args, sp),
            "handle" => self.b_handle(name, args, sp),
            "consume" => self.b_consume(name, args, sp),
            "mat" => self.b_mat(name, args, sp),
            "content" => self.b_content(name, args, sp),
            "unsure" => self.b_unsure(name, args, sp),
            // **J-15 那一位对 handler 可见**，不是只进 trace（`12` §2.11 硬要求一）。
            // 理由是路由真的不同：`tie` 的既定去向是「逐候选 noul」，而没测过的那条路
            // （K-noul）**本来就是逐候选 noul，路过去是空转**。handler 看不见那一位，
            // 就只能把两种情形当同一件事办——**那正是要消除的东西**。
            //
            // 返回 `""` 表示「都测过了」，返回载体名表示「那个量没测」。与 `unsure_cause`
            // 一样是**只读快照，不转移责任**（见 tests/duty.rs：读原因不算处理）。
            "taint" => self.b_taint(name, args, sp),
            "line_source" => self.b_line_source(name, args, sp),
            "cert" => self.b_cert(name, args, sp),
            "untested" => self.b_untested(name, args, sp),
            "near_boundary" => self.b_near_boundary(name, args, sp),
            // Z0514：对一个已有的未决出口走 J-05 默认链（与 handle 缺 unsure 臂同一份实现），回 {exit, end}
            "unsure_default" => self.b_unsure_default(args, sp),
            "unsure_cause" => self.b_unsure_cause(name, args, sp),
            // 一批题的信息值（B7、B43；步 28）：经规划器钩子算，实现在 jpp-plan::value 一处
            "gate_info" => self.b_gate_info(name, args, sp),
            // 切分点与信道容量（B7 后半；步 30）：经规划器钩子算，实现在 jpp-plan::value 一处
            "split_point" => self.b_split_point(name, args, sp),
            // 验题闸门的已知答案一半（04-v0 §5.4；步 28）：读数过「≥3 act + ≥3 ignore、gap ≥ 0.20」等判据
            "known_answers" => self.b_known_answers(name, args, sp),
            // 验题闸门第②段的结构代理（步 28）：候选状态的材料不越出来源状态
            "state_within" => self.b_state_within(name, args, sp),
            // 合法去向之一：把责任交给明确关联的人工请求（效应 ask）
            "escalate" => self.b_escalate(name, args, sp),
            // 合法去向之一：接走旧责任、按更字面的题重问，产生新的待处理出口（效应 judge）
            "literalize" => self.b_literalize(name, args, sp),
            // J-05 默认链（B0492 S2）：取材料来源的声明；库代码记一轮补信息与细化
            "unsure_source" => self.b_unsure_source(name, args, sp),
            "refine" => self.b_refine(name, args, sp),
            "pending" => self.b_pending(name, args, sp),
            "fail" => self.b_fail(name, args, sp),
            "is_fail" => self.b_is_fail(name, args, sp),
            // 账本键取用：读法内置，与 taint / unsure_cause 同类（B138 (3)，步 25-2c 由构造表移来）
            "key_of" => self.b_key_of(name, args, sp),
            "action_fact" => self.b_action_fact(name, args, sp),
            "exit_kind" => self.b_exit_kind(name, args, sp),
            "loop" => self.b_loop(name, args, sp),
            "stop" => self.b_stop(name, args, sp),
            "len" => self.b_len(name, args, sp),
            "map" | "filter" => self.b_map(name, args, sp),
            "fold" => self.b_fold(name, args, sp),
            "range" => self.b_range(name, args, sp),
            "append" => self.b_append(name, args, sp),
            "concat" => self.b_concat(name, args, sp),
            "slice" => self.b_slice(name, args, sp),
            "contains" => self.b_contains(name, args, sp),
            "sum" => self.b_sum(name, args, sp),
            "min" | "max" => self.b_min(name, args, sp),
            "abs" => self.b_abs(name, args, sp),
            "floor" => self.b_floor(name, args, sp),
            "reverse" => self.b_reverse(name, args, sp),
            "keys" => self.b_keys(name, args, sp),
            "has" => self.b_has(name, args, sp),
            "with" => self.b_with(name, args, sp),
            "text" => self.b_text(name, args, sp),
            "join" => self.b_join(name, args, sp),
            "print" => self.b_print(name, args, sp),
            // 文本与数据内置（B157）+ 带种子伪随机（B158）：一支多名转发，实现在 `builtins_text.rs`
            // （步 7t，`21` 原文「步 7c」；与已造的旧步 7c——B83 续接命中记录——撞号改称）。
            "split" | "lower" | "upper" | "trim" | "replace" | "starts_with" | "ends_with"
            | "index_of" | "chars" | "regex_match" | "regex_find" | "sort" | "sort_by"
            | "parse_json" | "to_json" | "hash" | "date_parse" | "date_format" | "date_add"
            | "rand" | "rand_int" | "shuffle" => self.text_builtin(name, args, sp),
            _ => err(Some("E-rt-name"), format!("未知内置 {name}"), sp),
        }
    }
    /// 账本键取用（B17 不变量 3：证据只存账本键）。步 25-2c 起是读法内置，不是内核构造（B138 (3)）。
    #[allow(unused_variables)]
    pub(crate) fn b_key_of(&mut self, name: &'static str, args: Vec<Value>, sp: Span) -> R<Value> {
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
        // 账本键：出口取它来自的那条账本记录；读数取自己的键；契约值取它的证据列表
        arity(1)?;
        match &args[0] {
            Value::Exit(e) | Value::Duty(e) => Ok(Value::text(&e.ledger_key.borrow())),
            Value::Reading(r) => Ok(Value::text(&r.ledger_key)),
            v if is_outcome(v) => Ok(v.get("evidence").unwrap_or(Value::list(vec![]))),
            other => err(
                Some("E-rt-arg"),
                format!("key_of 收出口、读数或契约值，收到 {}", other.type_name()),
                sp,
            ),
        }
    }
}
