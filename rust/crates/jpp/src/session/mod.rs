//! 宿主接口 `Session`（`20` §2.3 L6 `session/`，B74；`21` 步 14a）：`compile`、`explain`、`run`、
//! `replay`、`resume`、`feed_calib`。原 `jpp_core::run*` 与 CLI `run_io.rs` 里「头与补回」「出料」两段
//! 合并逻辑搬到这里，行为逐字节不变；`jpp::run*` 保留为它们的薄包装（测试与宿主不改）。
//!
//! 模块约束（`20` §2.3「禁止依赖」，`评估①裁定` §十第 12(c) 条，`deps.py` 核）：本模块不读写固定
//! 路径，不引用 `std::fs`、`std::env`。文件、目录与报文都归 `cli/`；校准记录合并策略（`--calib`
//! 与 `--fixtures` 谁优先）暂留 CLI，步 18 搬入 `store/`。
//!
//! 宿主入口（B105、B106，步 14b）：`compile(p, &EntryDecl)` 是 `Program.entry` 的唯一写入处，检查器从它
//! 知道入口名；`run`/`resume`/`replay` 收 [`EntryArgs`]（值条目、材料条目、`purpose`）。

use crate::check::{self, Report};
use crate::effects::{CalibRecord, CalibStore, FitRegistry, Sample};
use crate::interp::{ActionRegistry, EntryArgs, Interp, Outcome, RtError};
use crate::ir::EntryDecl;
use crate::ledger::{Ledger, LedgerPort, TraceCtx};
use crate::{Error, Program};
use jpp_effects::Ports;

/// 一次会话：判断端口、校准视图、动作表，以及可选的 `fit` 表。
pub struct Session<'a> {
    /// 按效应实例索引的端口表（步 15b）
    ports: Ports<'a>,
    calib: &'a CalibStore,
    actions: &'a ActionRegistry,
    fits: Option<&'a FitRegistry>,
    /// 跨运行缓存（步 19，B151；取代 15h-2 的 `--gen-cache`）
    cache: Option<&'a dyn crate::effects::CacheLookup>,
    /// 生成器模型与画像哈希（步 19：进账本头、生成物缓存键带模型）
    生成器: (Option<String>, Option<String>),
    /// 默认链取材料的第二、三级（Z0398）
    料库: Option<std::rc::Rc<dyn crate::effects::CategoryStore>>,
    宿主取材料: Option<std::rc::Rc<dyn crate::effects::MaterialSource>>,
    /// 标准库与题库版本（步 27，B48）：进账本头的 `lib_version`、`bank_version`
    版本: (Option<String>, Option<String>),
    /// 判断器的实际单价（步 22，B42 计划期费用下界）：宿主按实际后端给，缺省「没说」
    judge_price: crate::interp::JudgePrice,
    /// 料库标记存储（步 29，B20 第 1 种）：给了就把 `mat_marks`、`mat_mark` 两个宿主变换装进 S 库
    marks: Option<std::rc::Rc<dyn jpp_lib::MarkStore>>,
    /// 宿主给的本段追踪上下文（C-2）；不给由 [`Session::段上下文`] 按账本状态推导
    trace: Option<TraceCtx>,
    /// 上游交下来的整场余额（C-3）：首跑与续跑用它收紧本轮预算；审计重放不看它，取账本头里记的那份
    carry: Option<crate::interp::BudgetCarry>,
    /// 显式重新授权（C-3 R1）
    carry_reauthorize: bool,
    /// 伴随题的发法（B0492 S5）：`None` = 用 `Passes` 的默认值
    companions: Option<crate::interp::CompanionMode>,
    /// 引擎默认深度上限（G4b，裁定六十四）：`None` = 运行时的现值 256
    depth_cap_default: Option<u32>,
    /// 单元图开关（C2b，步 41；H1a 对照臂）：`None` = 运行时缺省（开）
    cells: Option<bool>,
}

impl<'a> Session<'a> {
    /// 按端口表建会话（步 15b、15c，`20` §2.3 `Ports`）：宿主为程序用到的每个效应实例注册一个端口。
    pub fn new(
        ports: Ports<'a>,
        calib: &'a CalibStore,
        actions: &'a ActionRegistry,
    ) -> Session<'a> {
        Session {
            ports,
            calib,
            actions,
            fits: None,
            cache: None,
            生成器: (None, None),
            料库: None,
            宿主取材料: None,
            版本: (None, None),
            judge_price: crate::interp::JudgePrice::NotGiven,
            marks: None,
            trace: None,
            carry: None,
            carry_reauthorize: false,
            companions: None,
            depth_cap_default: None,
            cells: None,
        }
    }

    /// 引擎默认深度上限（G4b，裁定六十四，`12` R11 第 5 条）：宿主配置，用到时写进账本头 `depth_cap_default`，
    /// 审计重放以账本头为准、不读这里
    pub fn with_depth_cap_default(mut self, d: u32) -> Self {
        self.depth_cap_default = Some(d);
        self
    }

    /// 用本会话设的引擎默认造根余额（G4b 附录一，复核 Q5）：程序没声明 `depth` 时链上限取引擎默认
    pub fn root_carry(&self, 整场: &crate::Budget) -> crate::interp::BudgetCarry {
        crate::interp::BudgetCarry::session_with_default(
            整场,
            self.depth_cap_default.unwrap_or(jpp_runtime::DEFAULT_DEPTH),
        )
    }

    /// 本趟运行这一段的追踪上下文（C-2，账本行外壳的 `trace`）：宿主用 [`TraceCtx::enter`] 从调用者的上下文推导好再给。
    /// 不给时由会话按账本推导（[`Session::段上下文`]）：首跑是由（程序、入口参数）推导的根，续跑是上一段的新一段，
    /// 审计重放沿用账本最后一段。宿主给了、而账本里已有带追踪的条目（续跑）时，宿主给的只用来核对追踪编号，续跑一律开新段（复核 G1）。追踪上下文不进 `EntryArgs`、不进 `entry_hash`：它是身份，不是输入。
    pub fn with_trace(mut self, ctx: TraceCtx) -> Self {
        self.trace = Some(ctx);
        self
    }

    /// 程序标识：程序 IR 规范化 JSON 的哈希。同一份程序（含入口声明）永远得到同一个值；默认根上下文与默认标签用它。
    pub fn program_id(program: &Program) -> String {
        let j = serde_json::to_value(program).expect("程序可序列化");
        jpp_ir::key::hash_of(&["program", &jpp_ir::key::canon(&j)])
    }

    /// 本趟这一段的追踪上下文，加一条要带出的告警（C-2；主控 Z0245 定：默认开，语言自己生成，确定性；
    /// Z0249 与复核 G1：续跑一律开新段）。
    /// - 审计重放：宿主给了用宿主的，否则沿用账本最后一个带追踪的条目的上下文（重放不追加条目；旧账本没有追踪时不盖章）；
    /// - **续跑（账本里已有带追踪的条目）：宿主给没给追踪参数都一样**——追踪编号取账本里的，新段 = 账本里最后一段的
    ///   `enter("resume#<账本里已有的不同段数>")`（父段 = 账本里的上一段，序号让推导仍是确定的）。宿主给的上下文
    ///   在这里只用来核对追踪编号：不一致时返回 `W-trace-mismatch`（以账本为准），不静默混段。有意的代价：崩溃后重跑的账本
    ///   与一次跑完的账本不再逐字节相同，换来每一趟有自己的段；
    /// - 账本里没有带追踪的条目（首跑、今天的旧账本、截到只剩头行）：宿主给了用宿主的；没给就是根上下文，追踪编号由
    ///   （程序标识、入口参数哈希）推导，标签是程序标识。同一程序、同一入口参数的两次独立运行得到同一个上下文
    ///   （重放要逐字节一致）；要分开，宿主自己给种子。
    pub fn 段上下文(
        &self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &dyn LedgerPort,
        replay: bool,
    ) -> (Option<TraceCtx>, Option<String>) {
        let view = ledger.view();
        if replay {
            return (
                self.trace.clone().or_else(|| view.last_trace().cloned()),
                None,
            );
        }
        if let Some(prev) = view.last_trace() {
            let 告警 = self.trace.as_ref().filter(|h| h.trace != prev.trace).map(|h| {
                format!(
                    "W-trace-mismatch: 宿主给的追踪编号 {} 与账本里已有的 {} 不一致；续跑以账本为准，新段接在账本的上一段之后",
                    h.trace, prev.trace
                )
            });
            return (
                Some(prev.enter(&format!("resume#{}", view.span_count()))),
                告警,
            );
        }
        if let Some(c) = &self.trace {
            return (Some(c.clone()), None);
        }
        let id = Self::program_id(program);
        let seed = format!("{id}\x1f{}", entry.hash().unwrap_or_default());
        (Some(TraceCtx::start(&seed, &id)), None)
    }

    /// 上游余额（C-3，主控 Z0171；裁定纸面阶段第五节「共 1」）：本轮生效上限 = 逐项 min(程序声明, 余额)，
    /// 起算深度接着余额里的深度；跑完 `Outcome.carry` 给出交给下一轮的余额。整场上限由宿主给
    /// （`BudgetCarry::session`），链是否延续由宿主是否传余额决定。`None` 与不调相同。
    pub fn with_carry(mut self, carry: Option<crate::interp::BudgetCarry>) -> Self {
        self.carry = carry;
        self
    }

    /// 显式重新授权（C-3 R1，主控定：重新授权只靠显式开关）：续跑同一轮时，`with_carry` 给的余额从这一趟起开新段
    /// （出 `W-carry-reauth` 写明上一段花了多少），而不是按账本的交回余额收紧。不开时，对不上账本交回余额的一律按旧文件
    /// 处理（`W-carry`）。账本里没有余额时不起作用。
    pub fn reauthorize_carry(mut self, on: bool) -> Self {
        self.carry_reauthorize = on;
        self
    }

    /// 判断器的实际单价（步 22）：固定观察给 `Known(0)`，真机给画像价格，画像没有价格给 `Untested`
    /// （计划期报 `W-cost-unknown`）。不设时计划期不按费用拒（只按调用数与时延）。
    /// 伴随题的发法（B0492 S5；CLI `--companions`）
    /// 单元图开关（C2b，步 41；`21` §六·5 消融矩阵的一格、H1a 对照臂）：关掉时解释器不调用 `jpp-cell`，
    /// 判断与效应的同运行复用回到 C2 以前的两张表
    pub fn with_cells(mut self, on: bool) -> Self {
        self.cells = Some(on);
        self
    }

    pub fn with_companions(mut self, mode: crate::interp::CompanionMode) -> Self {
        self.companions = Some(mode);
        self
    }

    pub fn with_judge_price(mut self, price: crate::interp::JudgePrice) -> Self {
        self.judge_price = price;
        self
    }

    /// 装上料库标记存储（步 29，B20 第 1 种；临时存储，骨架最终裁定后换）：`.jpp` 经 `transform("mat_marks", …)`
    /// 读、`transform("mat_mark", …)` 写材料上的标记（`lib/skeletons/select.jpp`）。不装则两个变换不存在，
    /// 库里 `select` 自动退化为每层照发。只凭账本的审计重放不需要装（变换结果取自账本）。
    pub fn with_mat_store(mut self, store: std::rc::Rc<dyn jpp_lib::MarkStore>) -> Self {
        self.marks = Some(store);
        self
    }

    /// 带跨运行缓存（步 19，B151 两段式）：判断、生成、变换按不含调用位置的缓存键命中即不调用，
    /// 在本账本写复用条目。宿主从缓存目录里的账本建索引（`jpp::store::CacheIndex`）。只凭账本的审计重放不查缓存。
    pub fn with_cache(mut self, cache: &'a dyn crate::effects::CacheLookup) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 默认链取材料的第二级：程序自己的料库（Z0398，裁定五十五）。今天没有真实现，测试用闭包作替身
    pub fn with_category_store(
        mut self,
        s: std::rc::Rc<dyn crate::effects::CategoryStore>,
    ) -> Self {
        self.料库 = Some(s);
        self
    }

    /// 默认链取材料的第三级：宿主取材料端口（Z0398，裁定五十五）。今天没有真实现，测试用闭包作替身
    pub fn with_material_source(
        mut self,
        s: std::rc::Rc<dyn crate::effects::MaterialSource>,
    ) -> Self {
        self.宿主取材料 = Some(s);
        self
    }

    /// 生成器身份（步 19）：模型与画像哈希进账本头，生成物的缓存键带模型。宿主没有真实生成器时不设。
    pub fn with_gen(mut self, model: Option<String>, profile_hash: Option<String>) -> Self {
        self.生成器 = (model, profile_hash);
        self
    }

    /// 标准库与题库版本（步 27，B48）：宿主按装载的源文件算好交进来（`jpp::store::bank::versions_of`），进账本头。
    pub fn with_versions(mut self, lib: Option<String>, bank: Option<String>) -> Self {
        self.版本 = (lib, bank);
        self
    }

    /// 带 `fit` 表（`12` §6.0 的 fit 桥要用它）
    pub fn with_fits(mut self, fits: &'a FitRegistry) -> Self {
        self.fits = Some(fits);
        self
    }

    /// 表层程序降到 IR（步 12d）：`jpp_syntax::lower` 配上外观层的名字表。缺预算等在这里报（`J-07a`）。
    ///
    /// 宿主入口声明（B106）：这里是 `Program.entry` 的唯一写入处；名字重复报 `E-entry-dup`，与内置名
    /// 相同报 `E-entry-name`（降级诊断，位置为整个程序）。无入口传 `&EntryDecl::default()`。
    /// 依据：B106（地基/附注/2026-09-25-B105-B106裁定.md §三）
    pub fn compile(
        p: &crate::syntax::ast::Program,
        entry: &EntryDecl,
    ) -> Result<Program, Vec<crate::syntax::Diagnostic>> {
        let mut prog = crate::syntax::lower(p, &crate::names::CurrentNames)?;
        let whole = crate::syntax::ast::Span {
            start: prog.span.start,
            end: prog.span.end,
        };
        let mut errs = vec![];
        let mut seen = std::collections::HashSet::new();
        for e in &entry.params {
            if !seen.insert(e.name.as_str()) {
                errs.push(crate::syntax::Diagnostic::new(
                    format!(
                        "E-entry-dup: 宿主入口参数 {} 给了两次（依据：B105 / B106）",
                        e.name
                    ),
                    whole,
                ));
            } else if crate::interp::BUILTINS.contains(&e.name.as_str()) {
                errs.push(crate::syntax::Diagnostic::new(
                    format!(
                        "E-entry-name: 宿主入口参数 {} 与内置名相同，程序里会遮蔽内置（依据：B105 / B106）",
                        e.name
                    ),
                    whole,
                ));
            }
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        prog.entry = entry.clone();
        // 「无作者去向」站点（B0492 S4，供 S2c 的运行时默认链）：检查器算、写进 IR 的唯一写入处
        prog.unsure_default_sites = check::unsure_default_sites(&prog);
        // 结构化站点表（B0630）：判断键与效应键的站点取它，写进 IR 的唯一写入处；lib 区间由 loader 填进表层程序
        prog.site_keys = jpp_ir::site_key::site_keys(&prog, &p.lib_ranges);
        Ok(prog)
    }

    /// 执行前的静态检查（CLI `check` 与 `run` 执行前那次）：有画像带画像。入口名从 `Program.entry` 读（B106）。
    pub fn explain(program: &Program, profile: Option<&crate::effects::Profile>) -> Report {
        match profile {
            Some(p) => check::check_with_profile(program, p),
            None => check::check(program),
        }
    }

    /// [`Self::explain`] 的动作表增强版（步 24c，B108 已知限制的收口）：CLI `check`（无 `--input`
    /// 也照样有）与 `run` 的预跑诊断用它，把宿主已知的动作表交给检查器，让 J-08 静态子面对可逆
    /// 动作不报、对不可逆动作报 error，而不是没有表时一律降成 `W-guard-untrusted`。
    pub fn explain_with_actions(
        program: &Program,
        profile: Option<&crate::effects::Profile>,
        actions: &check::ActionTable,
    ) -> Report {
        check::explain_with_actions(program, profile, actions)
    }

    /// [`Self::explain_with_actions`] 再带整本校准记录（L7 2026-09-28）：CLI `check` 用它，J-10 静态面
    /// （K-084/K-160）因此在 `jpp check` 上可达。`run` 不用它（`go` 执行前那次已带记录，避免 J-10 报两遍）。
    pub fn explain_with_calib_actions(
        program: &Program,
        profile: Option<&crate::effects::Profile>,
        calib: &CalibStore,
        actions: &check::ActionTable,
    ) -> Report {
        check::explain_with_calib_actions(program, profile, calib, actions)
    }

    /// 首跑：检查（带整本校准记录，J-10 静态面）无错再执行。`ledger` 为空账本。
    pub fn run(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut dyn LedgerPort,
    ) -> Result<Outcome, Error> {
        self.go(program, entry, ledger, false)
    }

    /// 续跑（`--resume`）：已记录的不付费、继续往下；与首跑同一路径，`ledger` 为上一趟的账本。
    pub fn resume(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut dyn LedgerPort,
    ) -> Result<Outcome, Error> {
        self.go(program, entry, ledger, false)
    }

    /// 审计重放（B35；21 步 3）：只凭账本重现首跑。账本里记过的调用照记录计入预算，缺的记录报
    /// `E-replay`（致命，不进 cause）。
    pub fn replay(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut dyn LedgerPort,
    ) -> Result<Outcome, Error> {
        self.go(program, entry, ledger, true)
    }

    /// 跳过静态检查直接执行——只给检查器本身的对照测试用。
    pub fn run_unchecked(
        self,
        program: &Program,
        ledger: &mut dyn LedgerPort,
    ) -> Result<Outcome, RtError> {
        let budget = program.budget.clone();
        Interp::new(self.ports, ledger, self.calib, self.actions, budget).run(program)
    }

    fn go(
        self,
        program: &Program,
        entry: &EntryArgs,
        ledger: &mut dyn LedgerPort,
        replay: bool,
    ) -> Result<Outcome, Error> {
        // **档案走到检查器**（`12` §1.2）。传 `check(program)` 会让降级规则永远够不着真实运行
        // ——那样管道就只在测试里通，而**一个只在测试里通的管道是构造，不是功能**。
        // **整本记录走到检查器**，不只是档案：J-10 的静态那一半要各题的 `unsure_rate`，
        // 而那住在 `CalibRecord` 里。**与 §1.2 那根「档案到不了检查器」的管道是同一种缺结构**，
        // 只是这次缺的是记录不是档案。
        // 放行把关（意图汇编 11a）：检查器与运行时都只读 `Program.entry.guard`。宿主在 `EntryArgs` 上开了把关、编译时
        // 却没经 `decl()` 带进 `Program` 的，这里补上——宿主明说要把关就照做，不因少传一次而静默关掉
        let 补把关;
        let program = if entry.guard && !program.entry.guard {
            let mut p = program.clone();
            p.entry.guard = true;
            补把关 = p;
            &补把关
        } else {
            program
        };
        // 入口名在 `Program.entry` 里（B106，`compile` 写入），检查器自己读，这里不再另传名字表
        // 动作表也走到检查器（B108，步 24-0）：J-08 静态子面在不可逆动作上报 error，必然被拦的程序
        // 在花调用之前停下
        let table = check::ActionTable {
            actions: (self.actions.actions.values())
                .map(|a| {
                    let facts = check::ActionFacts {
                        reversible: a.reversible,
                        output_untrusted: a.taint_out == crate::interp::TaintOut::Untrusted,
                        // B164：`no_sandbox` 的权威来源是 `jpp::actions::check_table()`（CLI
                        // `check`/`run` 的预检查都走那张表，已经在这之前拦下）；`interp::Action`
                        // 本身没有这一位（加它要扩 `jpp-runtime`，跨轨，本次不做），这里的
                        // `ActionTable` 只是 `Session::go` 内部的二次检查，未接这一位是已知、
                        // 记录在案的范围边界，不是疏漏——运行期本身（`exec_py_core` 等）仍会
                        // 独立拒绝执行并返回 `Fail(NoSandbox)`，双重覆盖已经够。
                        no_sandbox: false,
                    };
                    (a.name.clone(), facts)
                })
                .collect(),
            // `mat_shape`（B51-R2，步 24g，接入步 24h，主会话已批准）：`Action.mat_shape` 字段
            // 在 `jpp-runtime` 上本就公开可读，动作声明过形状的才进表；`W-diag-shape`（诊断层
            // 静态消费者，`jpp-check/src/diag/b13.rs::shape_check`）据此在检查期判断材料形状
            // 是否够回答。
            shapes: (self.actions.actions.values())
                .filter_map(|a| a.mat_shape.clone().map(|s| (a.name.clone(), s)))
                .collect(),
        };
        let report = check::check_with_calib_actions(program, self.calib, &table);
        if !report.is_ok() {
            return Err(Error::Check(report));
        }
        let budget = program.budget.clone();
        // C-2：这一趟追加的条目都盖上本段的追踪上下文（行外壳 `trace`）；在 `Interp` 借走账本之前设
        let (段, 追踪告警) = self.段上下文(program, entry, &*ledger, replay);
        ledger.set_trace(段);
        // S 库的宿主侧（步 26，A-11：注册点 `jpp_lib::s_library`，主会话裁定 2026-09-29 第十四条）：
        // 运行期诊断闸门，只提示不改走向（B47；批 9 裁定 :42、:136）
        let s库 = jpp_lib::s_library_with(program, self.marks.clone());
        let mut it = match self.fits {
            Some(f) => Interp::with_fits(self.ports, ledger, self.calib, self.actions, f, budget),
            None => Interp::new(self.ports, ledger, self.calib, self.actions, budget),
        };
        if let Some(m) = self.companions {
            it.passes.companions = m;
        }
        if let Some(c) = self.cells {
            it.set_cells(c);
        }
        // 伴随题序言（B0492 S5，主控 2026-09-30 路 A）：把 `lib/unsure.jpp` 降级交给运行时。Z0398 起一律装：
        // 通用类别表 `unsure_lacks` 是默认链候选的末级，不随伴随题开关变（过程记录 5.19）；伴随题登不登记仍看发法
        it = it.with_prelude(unsure_prelude());
        // 宿主入口（B105）：运行入口绑定条目、`entry_hash` 进账本头；空入口与不设相同
        if !entry.is_empty() {
            it = it.with_entry(entry.clone());
        }
        if replay {
            it = it.audit_replay();
        }
        if let Some(c) = self.cache
            && !replay
        {
            it = it.with_cache(c);
        }
        it = it.with_gen(self.生成器.0.clone(), self.生成器.1.clone());
        if let Some(s) = self.料库.clone() {
            it = it.with_category_store(s);
        }
        if let Some(s) = self.宿主取材料.clone() {
            it = it.with_material_source(s);
        }
        it = it.with_versions(self.版本.0.clone(), self.版本.1.clone());
        // C-3：审计重放由运行时取账本头里的余额，宿主给的只在首跑与续跑用
        it.set_carry(self.carry.clone());
        it.set_carry_reauthorize(self.carry_reauthorize);
        if let Some(d) = self.depth_cap_default {
            it.set_depth_cap_default(d);
        }
        it.set_gate(s库.gate());
        it.set_transforms(s库.transforms());
        it = it.with_judge_price(self.judge_price);
        let mut out = it.run(program).map_err(Error::Runtime)?;
        if let Some(w) = 追踪告警 {
            out.trace.warnings.insert(0, w);
        }
        Ok(带出静态告警(out, &report))
    }

    /// **只凭账本重放时补回当时的线**（头与补回；步 14a 自 CLI `run_io.rs` 搬来）：账本记着那一趟
    /// `cut` 实际查到的校准记录。`calib` 里已有的键（这次显式给了的）优先；没给的键才从账本补。
    /// 返回补回的键（`\u{1f}` 换成 `:`，按账本顺序），报文由宿主打印。
    pub fn restore_calib(calib: &mut CalibStore, ledger: &Ledger) -> Result<Vec<String>, String> {
        let mut 补回: Vec<String> = vec![];
        for (k, v) in &ledger.calib_used {
            // B142（步 20j-1）：作者声明线不是校准记录，重放从源码重算它
            if calib.records.contains_key(k) || k.starts_with(jpp_ledger::DECLARED_PREFIX) {
                continue;
            }
            let rec: CalibRecord = serde_json::from_value(v["record"].clone())
                .map_err(|e| format!("账本里的校准记录 {k:?} 读不成：{e}"))?;
            calib.records.insert(k.clone(), rec);
            补回.push(k.replace('\u{1f}', ":"));
        }
        Ok(补回)
    }

    /// **重放要用记录时那个 model_id**（头）：旧账本没有 `header` 时退回 `"fixed-0"`。
    pub fn replay_model_id(ledger: &Ledger) -> String {
        ledger
            .header
            .as_ref()
            .map(|h| h.model_id().to_string())
            .unwrap_or_else(|| "fixed-0".to_string())
    }

    /// **出料**：把这一趟的证据折进记录，标出停岗候选（`jpp_calib::feed::feed`）。落盘归宿主。
    pub fn feed_calib(
        calib: &CalibStore,
        evidence: &[(String, Sample)],
        suspend_candidates: &[&str],
    ) -> Result<(CalibStore, Vec<String>), String> {
        jpp_calib::feed::feed(calib, evidence, suspend_candidates)
    }
}

/// **J-10 的静态告警要带到调用者手里。** 它只是告警，`report.is_ok()` 仍为真，
/// 这份报告若在这里丢掉，调用者就永远看不到「跑之前就知道 unsure 预算不够」这句话。
/// 只带 J-10：别的静态告警 CLI 在执行前已经用 `check`/`check_with_profile` 打过，
/// 而只有要整本校准记录的 J-10 必须走这一处（`check_with_calib`）。
/// 放在 `trace.warnings` 最前面，表示它们先于任何调用成立。运行期出错时这份告警不随 `RtError` 带出。
fn 带出静态告警(mut out: Outcome, report: &Report) -> Outcome {
    let 静态: Vec<String> = report
        .warnings()
        .iter()
        .filter(|d| d.rule == "J-10")
        .map(|d| format!("{}: @{} {}", d.rule, d.span.start, d.message))
        .collect();
    if !静态.is_empty() {
        out.trace.warnings.splice(0..0, 静态);
    }
    out
}

/// 题面（或题式模板）是不是伴随题式的形状：`lib/unsure.jpp` 五道题式都以「题「」「把题「」「判断题「」开头。
/// 账本与线上请求都不带题是不是伴随题，只能按题面认（待标清单去伴随题行、测试替身给中性读数都用它；过程记录 5.22）
pub fn 是伴随题面(text: &str) -> bool {
    text.starts_with("题「") || text.starts_with("把题「") || text.starts_with("判断题「")
}

/// 伴随题的标准题式（B0492 S5，主控 2026-09-30 路 A）：`lib/unsure.jpp` 原文，编译期嵌入，只此一份来源。
/// 宿主（CLI）把它算进 `lib_version`（路径 `unsure.jpp`；Z0398 起一律装载，重放按账本头挑）。
pub const UNSURE_PRELUDE: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../lib/unsure.jpp"));

/// 序言降级成 IR 程序（给它一个空预算，序言里只有 `form` 与 `let`）
pub fn unsure_prelude() -> Program {
    let src = format!("budget {{calls: 0, cost: 0}};\n{UNSURE_PRELUDE}\n0\n");
    let ast = crate::syntax::parse(&src).expect("lib/unsure.jpp 可解析");
    crate::syntax::lower(&ast, &crate::names::CurrentNames).expect("lib/unsure.jpp 可降级")
}
