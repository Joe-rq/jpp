//! 运行时加规划的宿主侧组装（步 14a，B74）：原 `jpp_core::interp` 的路径与用法不变。
//!
//! `20` §2.2 第 3 条：运行时（`jpp-runtime`）不依赖规划（`jpp-plan`）。步 13a 时解释器在 `run` 入口
//! 按自身的 `passes` 现算计划，钩子默认 `jpp_plan::Hooks`，那是临时依赖（`COORDINATION.md`「14a 撤」）。
//! 撤掉之后，pass 开关与算计划这一步留在宿主：本模块的 [`Interp`] 包着运行时的解释器，带 `passes`，
//! `run` 前一刻用 `jpp_plan::plan` 算出计划、连同 `jpp_plan::Hooks` 交给运行时。算法与时点都与
//! 步 13a 相同（入口处、按当时的开关算），所以外部行为不变。`Session`（`session/`）走同一条路径。

use std::ops::{Deref, DerefMut};

use jpp_effects::Ports;
use jpp_ir::ir::{Budget, Program};
pub use jpp_ir::plan::CompanionMode;
pub use jpp_ir::plan::{Estimate, JudgePrice, Plan, PlanCtx, SitePlan, UnknownReason};
use jpp_ledger::LedgerPort;
pub use jpp_plan::Passes;
pub use jpp_plan::plan_with;
pub use jpp_runtime::*;

/// 运行时解释器加 pass 开关。字段与方法经 `Deref` 取运行时的；构造与 `run` 在这里包一层。
pub struct Interp<'a> {
    inner: jpp_runtime::Interp<'a>,
    /// 编译 pass 开关（`12` §4；定义在 `jpp-plan`）。`run` 入口据它算出计划
    pub passes: Passes,
    /// 判断器的实际单价（步 22，宿主按后端给；缺省「没说」，计划期不按费用拒）
    judge_price: JudgePrice,
}

impl<'a> Interp<'a> {
    /// 不带 fit 表的入口（绝大多数程序不用 fit）。效应调用只经端口表（步 15b、15c）。
    pub fn new(
        ports: Ports<'a>,
        ledger: &'a mut dyn LedgerPort,
        calib: &'a dyn jpp_effects::views::CalibView,
        actions: &'a ActionRegistry,
        budget: Budget,
    ) -> Interp<'a> {
        Interp {
            inner: jpp_runtime::Interp::new(ports, ledger, calib, actions, budget),
            passes: Passes::default(),
            judge_price: JudgePrice::NotGiven,
        }
    }

    pub fn with_fits(
        ports: Ports<'a>,
        ledger: &'a mut dyn LedgerPort,
        calib: &'a dyn jpp_effects::views::CalibView,
        actions: &'a ActionRegistry,
        fits: Fits<'a>,
        budget: Budget,
    ) -> Interp<'a> {
        Interp {
            inner: jpp_runtime::Interp::with_fits(ports, ledger, calib, actions, fits, budget),
            passes: Passes::default(),
            judge_price: JudgePrice::NotGiven,
        }
    }

    /// 审计重放（B35）：见运行时同名方法
    pub fn audit_replay(self) -> Self {
        Interp {
            inner: self.inner.audit_replay(),
            passes: self.passes,
            judge_price: self.judge_price,
        }
    }

    /// 宿主入口参数（B105，步 14b）：见运行时同名方法
    pub fn with_entry(self, entry: EntryArgs) -> Self {
        Interp {
            inner: self.inner.with_entry(entry),
            passes: self.passes,
            judge_price: self.judge_price,
        }
    }

    /// 跨运行缓存（步 19）：见运行时同名方法
    pub fn with_cache(self, cache: &'a dyn jpp_effects::views::CacheLookup) -> Self {
        Interp {
            inner: self.inner.with_cache(cache),
            passes: self.passes,
            judge_price: self.judge_price,
        }
    }

    /// 生成器身份（步 19）：见运行时同名方法
    pub fn with_gen(self, model: Option<String>, profile_hash: Option<String>) -> Self {
        Interp {
            inner: self.inner.with_gen(model, profile_hash),
            passes: self.passes,
            judge_price: self.judge_price,
        }
    }

    /// 标准库与题库版本（步 27，B48）：见运行时同名方法
    pub fn with_versions(mut self, lib: Option<String>, bank: Option<String>) -> Self {
        self.inner = self.inner.with_versions(lib, bank);
        self
    }

    /// 默认链取材料的第二级（Z0398）：见运行时同名方法
    pub fn with_category_store(mut self, s: std::rc::Rc<dyn jpp_effects::CategoryStore>) -> Self {
        self.inner = self.inner.with_category_store(s);
        self
    }

    /// 默认链取材料的第三级（Z0398）：见运行时同名方法
    pub fn with_material_source(mut self, s: std::rc::Rc<dyn jpp_effects::MaterialSource>) -> Self {
        self.inner = self.inner.with_material_source(s);
        self
    }

    /// 伴随题序言（B0492 S5）：见运行时同名方法。`Session` 总会交；不经 `Session`、直接嵌入 `Interp` 的宿主拿不到序言
    /// （没有标准伴随题式与通用类别表），要自己交 `jpp::session::unsure_prelude()`（Z0398 复核，过程记录 5.23）
    pub fn with_prelude(mut self, p: jpp_ir::ir::Program) -> Self {
        self.inner = self.inner.with_prelude(p);
        self
    }

    /// 判断器的实际单价（步 22）：固定观察 `Known(0)`，真机为画像价格，画像没价格为 `Untested`
    pub fn with_judge_price(mut self, price: JudgePrice) -> Self {
        self.judge_price = price;
        self
    }

    /// 一次运行：按 `passes`、画像与运行语境算出计划（`jpp_plan::plan_with`），连同 `jpp_plan::Hooks`
    /// 交给运行时。计划被拒（步 22，B42：可靠下界超预算）即返回 `E-budget-plan`，一次调用都不发；
    /// 计划期告警（`W-cost`、`W-cost-unknown`）先进 `trace.warnings`。
    pub fn run(mut self, program: &Program) -> Result<Outcome, RtError> {
        let (ledger_empty, cache_off, profile) = self.inner.plan_inputs();
        let ctx = PlanCtx {
            ledger_empty,
            cache_off,
            judge_price: self.judge_price,
        };
        // C-3（主控第二轮答复第 2 条）：可靠下界按生效预算 min(程序声明, 上游余额) 核，上游余额已不够必经下界的
        // 计划期就拒（`E-budget-plan`）。`jpp-plan` 读的就是 `Program.budget`，这里只换一份预算交给它；没有余额时不克隆
        let 生效 = self.inner.effective_budget(&program.budget);
        let 换了预算;
        let 计划用 = if 生效 != program.budget {
            换了预算 = Program {
                budget: 生效,
                ..program.clone()
            };
            &换了预算
        } else {
            program
        };
        let plan = jpp_plan::plan_with(计划用, &self.passes, Some(profile), &ctx);
        // G4 附录二（复核 C1）：本趟已到深度上限时只停发、照常求值，一道题都不发，不按下界拒绝
        if let Some(d) = &plan.rejected
            && !self.inner.depth_stop_ahead()
        {
            // C-3 G2：上游收紧时修法改指上游余额（改源码里的 budget 没有用）
            let msg =
                jpp_runtime::rewrite_plan_rejection(&d.message, &program.budget, &计划用.budget)
                    .unwrap_or_else(|| d.message.clone());
            return Err(RtError::new(Some(&d.code), msg, d.span));
        }
        for w in &plan.warnings {
            self.inner.trace.warn(w.message.clone());
        }
        self.inner.run(program, plan, &jpp_plan::Hooks)
    }
}

impl<'a> Deref for Interp<'a> {
    type Target = jpp_runtime::Interp<'a>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Interp<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
