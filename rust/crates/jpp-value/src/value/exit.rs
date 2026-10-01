//! 出口（`ExitKind`、`Exit`）与挂起（`Pending`）。步 36 G3 从 `value.rs` 原样搬出（只搬不改）。

use super::*;

pub use jpp_ir::cause::UnsureCause;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExitKind {
    Act,
    Ignore,
    Pick(usize),
    At(usize),
    Unsure(Why),
}

/// 未决的原因与细节（`12` §2.13 R15 `Unsure(cause, parts?)`；B197；步 36 G3）。原因是十六种的封闭枚举；
/// 细节是原来拼在冒号后面的东西（`insufficient` 缺的槽名、`fail` 的失败文字），只进标签，不参与分路。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Why {
    pub cause: UnsureCause,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Why {
    pub fn of(cause: UnsureCause) -> Why {
        Why {
            cause,
            detail: None,
        }
    }
    pub fn with(cause: UnsureCause, detail: impl Into<String>) -> Why {
        Why {
            cause,
            detail: Some(detail.into()),
        }
    }
    /// 标签里括号中的那段：`<名>` 或 `<名>:<细节>`（B197：`"unsure(<cause>"` 前缀是冻结接口）
    pub fn text(&self) -> String {
        match &self.detail {
            Some(d) => format!("{}:{d}", self.cause.name()),
            None => self.cause.name().to_string(),
        }
    }
    /// 从原因串读：`<名>` 或 `<名>:<细节>`。名不是十六种之一时取 `absent`：只用于账本里读回的缺席记录原因
    /// （旧账本、`spec_miss` 这类记录原因，步 36 G3 待定项 4，主控定）
    pub fn from_record(s: &str) -> Why {
        let (名, 细节) = match s.split_once(':') {
            Some((a, b)) => (a, Some(b.to_string())),
            None => (s, None),
        };
        match UnsureCause::parse(名) {
            Some(cause) => Why {
                cause,
                detail: 细节,
            },
            None => Why::of(UnsureCause::Absent),
        }
    }
}

impl From<UnsureCause> for Why {
    fn from(c: UnsureCause) -> Why {
        Why::of(c)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct Exit {
    pub id: usize,
    /// 出口来自哪种题（handle 的穷尽分支按它定）
    pub op: Op,
    pub kind: ExitKind,
    pub q_hash: String,
    pub state_hash: String,
    pub taint: Taint,
    pub site: Span,
    /// 这个出口是不是经 `ask` 来的（J-08「或经 `ask`」；人答是 trusted，§2.11）
    pub from_ask: Cell<bool>,
    pub consumed: Cell<bool>,
    pub consumed_by: RefCell<String>,
    /// **这条线是哪一级的**：`""` = 没用上线（冷 / 停岗 / fail）、`"题级"` = 这道题自己的、
    /// `"模式级"` = 借的同类先验（`12`:136）。
    ///
    /// **它必须对 handler 可见**，理由与 J-15 那一位同：**模式级的线不能冒充题级的线**。
    /// 看不见来源，handler 就只能把「这道题测过 200 条」和「这类题测过 200 条、这道题
    /// 一条没有」当同一件事办——**那是把两种证据强度压平**。
    pub line_source: String,
    /// **J-15 的那一位**：本次路径上有没有一个**被声明为判据、却没有被测量的量**。
    /// `None` = 该出口引用的判据都测过；`Some(载体)` = 那个量没测过（载体只作诊断）。
    ///
    /// **它不是 `cause`，这是要点。** `cause` 的本分是**路由键**（§5 handler 库按它分流）。
    /// 把「测没测过」塞进 `cause` 会得到 `cold` / `no_perm` / `no_ece` / `no_klimit`……
    /// 而 **`cold` 的存在正是证据：那条路已经走过一次，然后停了**。每多一个 `cause`，
    /// handler 库就多一条要记得加的路由，**而漏加路由的失效方式是静默的**——`otherwise`
    /// 兜住，没人知道。所以它是**正交的一位**：跟着出口走，`cause` 不动。
    ///
    /// 判别法（`12` §2.11）：这个区别是「这一格特有的」还是「会在很多格上重复出现的」？
    /// 今天已经有五个载体同处「没测过」：线未测（`cold`）、置换未测（`mode_share == None`）、
    /// 档案字段未测（`choice_same_call_perm_crosstalk`）、`k_limit` 120–250 档未测、
    /// ECE 未过检。重复出现 → 加维度，不加 `cause`。
    pub untested: Option<String>,
    /// 这个出口来自哪一条账本记录（`cut` 时写入；其余出口为空）。
    /// 组合封闭性契约的证据只存这个键，不存读数或材料的副本（B17 不变量 3）。
    pub ledger_key: RefCell<String>,
    /// **这个出口的线等级**（步 20a-1，`LineGrade`，`20` v2 §3.4）：`cut` 出口一律有值（没用上线即
    /// `Cold`）；`None` = 出口不来自 `cut`（`ask` 与构造派生的出口），没有「线」可谈，放行由它自己的
    /// 来源决定（taint、`from_ask`）。取代原来的 `fixture_line`（B29）、`class_line`（B75）、
    /// `trial_line`（B72）三个布尔位；等级派生读有效 α（B89，见 `jpp-calib::cert_view`）。
    pub grade: Cell<Option<LineGrade>>,
    /// **这条线是停岗候选**（B25）：正交位，可与任何等级叠加；出口照常路由，不放行不可逆 `do`。
    pub suspend_candidate: Cell<bool>,
    /// **材料在这条线的认证范围之外**（B68）：正交位（补遗 12(a)：范围外不是等级，是这次使用失去了保证）；
    /// 出口照常路由，不放行不可逆 `do`。
    pub scope_out: Cell<bool>,
    /// **线的证书没有记录认证带宽**（B104-1：按 δ 平移过的旧证书，`selection` 在而 `selection.delta` 缺）：
    /// 出口照常路由，不算放行不可逆 `do` 的可信合取项，直到 `load`（步 20c）重跑写回 δ 或重新导入。
    pub delta_unknown: Cell<bool>,
    /// **线的认证范围未知**（B104-2：记录没有材料指纹）：出口照常路由，不算放行不可逆 `do` 的可信合取项，
    /// 直到带文本重新导入或经 B91 扩展并入。
    pub scope_unknown: Cell<bool>,
    /// **窗口未测**（Z0364，主会话裁定四十九 (c)；形状同 `scope_out`、B156 的 `crosstalk_untested`）：读数来自声明了
    /// `fission: "approx"` 的题、而画像没有测过窗口——本想按窗切、因窗口未测（H6）没切，超窗与否无从核对。
    /// 出口照常路由，不算放行不可逆 `do` 的可信合取项（`--guard` 下不单独放行）。不取「最小已知值」（A7：内核不编数）。
    pub window_untested: Cell<bool>,
    /// **读数落在画像边界带内**（Z0497，线 A N2）：无线是非题按判断器的回答出了已决出口，而读数离 0.5 大于 0、小于画像中段 δ
    /// （`delta.noul.mid`，Z0398 默认链同一个带）——补信息后仍在带内、或没补（没有取法 / `--guard`）时置位。只记录：
    /// 不改出口种类、不算可信合取项、不拦放行；`chain` 在作者没给 `near` 时按它决定已决出口要不要再派生。
    pub near_boundary: Cell<bool>,
    /// **合成出口的分量**（B131，步 25-2b）：由内核合成构造 `compose` 从这些出口按封闭规则派生，是合成出口的
    /// 谱系入口；非合成出口为空。不序列化。放行合取与谱系穿过它在步 25-9 落（此前合成出口一律按冷线不放行，25-1）。
    pub parts: RefCell<Vec<Rc<Exit>>>,
    /// **宿主接受了作者声明线放行**（B128，步 20j-1 加位、恒假；20j-2 由 `EntryArgs.accept.declared_lines`
    /// 置位并按 `20` v2 §3.4 改写 [`Exit::releases`]）。只对 `LineGrade::Declared` 的出口有意义。
    pub host_accepts_declared: Cell<bool>,
    /// **计入联合界的 α**（B161，步 25d）：`cut` 出口的线有证书、等级为正式或题式级、不在范围外、范围与带宽都已知时
    /// 取证书的 `alpha_eff`；`ask` 出口取 0（人答即真值，B31）；其余 `None`（按 1 计、算一个未知），包括试用、临时上岗、
    /// 声明线。不序列化。
    pub alpha: Cell<Option<f64>>,
    /// **合成出口的联合界**（B161）：`(alpha_bound, n_unknown)`，`alpha_bound = min(1, Σ 分量 α)`；非合成出口为 `None`。
    /// 不序列化；读法内置 `cert` 读它。
    pub bound: Cell<Option<(f64, u32)>>,
}

impl Exit {
    /// **放行的唯一判定点**（步 20a-1；补遗 12(a)）：
    /// `releases() = grade ∈ {Certified, Form} ∧ ¬scope_out ∧ ¬suspend_candidate ∧ untested = None
    /// ∧ ¬delta_unknown ∧ ¬scope_unknown ∧ ¬window_untested`。
    ///
    /// 不看 taint：taint 是材料的属性，由 [`Exit::guard_trusted`] 合取。报告 `exits` 表的 `releases`
    /// 就是这个函数的值。`grade` 为 `None` 的出口（不来自 `cut`）没有线，等级一项不适用，只看正交位。
    ///
    /// 步 20j-2（B128；`20` v2 §3.4）：`grade = Declared` 时等级一项取 [`Exit::host_accepts_declared`]（宿主接受作者
    /// 声明线放行），其余等级照 `LineGrade::releases`。`None` 仍为真——`None ⇒ 假` 与合成出口的分量合取是 B131
    /// （库轨 25-9）的改动，按 COORDINATION 先合入者写。
    pub fn releases(&self) -> bool {
        // 合成出口（B131，步 25-9）：全部分量放行之合取；分量来自 `ask`（人答即真值，B31）也算放行。
        // 分量的正交位与谱系各由分量自己的 `releases()` 与 `guard.rs::谱系` 穿 `parts` 核
        {
            let parts = self.parts.borrow();
            if !parts.is_empty() {
                return parts.iter().all(|p| p.from_ask.get() || p.releases());
            }
        }
        let 等级 = match self.grade.get() {
            // B131：`grade: None` 的已决出口不再等于放行——只剩 `ask` 出口（经 `via_ask` 放行）；
            // 没有分量的合成出口（`compose([], …)`）落在这里，不放行（B140：空分量集合不取真）
            None => self.from_ask.get(),
            Some(LineGrade::Declared) => self.host_accepts_declared.get(),
            Some(g) => g.releases(),
        };
        等级
            && !self.scope_out.get()
            && !self.suspend_candidate.get()
            // J-15：本次路径上有未测的判据，不放行（步 20a-1 起；此前不查）
            && self.untested.is_none()
            // B104：认证带宽未记录、认证范围未知都不放行
            && !self.delta_unknown.get()
            && !self.scope_unknown.get()
            // 裁定四十九 (c)：声明了裂变而画像没测窗口，超窗与否不可证，不放行
            && !self.window_untested.get()
    }
    /// J-08 的可信合取项：出口已决，状态可信，且 [`Exit::releases`]。
    ///
    /// 未决出口不是放行判定：unsure 臂拿到的是责任，不是判定，不论 taint 与等级（B121-2，
    /// 步 16-0）。此前这里不看出口种类，正式线上的 `Unsure(band)` 也 `releases()`，于是可信
    /// 材料上 unsure 臂里的不可逆 `do` 被放行（`tests/bypass_j08_unsure_arm.rs`）。
    /// 依据：B121（地基/附注/2026-09-25-B121守卫证据裁定.md §二）
    pub fn guard_trusted(&self) -> bool {
        !self.is_unsure() && self.taint == Taint::Trusted && self.releases()
    }
    pub fn is_unsure(&self) -> bool {
        matches!(self.kind, ExitKind::Unsure(_))
    }
    /// 这个出口引用的判据里没被测量的那个量（J-15 的那一位）。**读取不转移责任。**
    /// 与 `cause()` 正交：`cause()` 回答「往哪条路由走」，这个回答「那条路上的判据测没测过」。
    pub fn untested(&self) -> Option<&str> {
        self.untested.as_deref()
    }
    /// 未决的原因名（十六种之一，B197；步 36 G3 起不带细节）。**它是路由键**（§5 handler 库按它分流），不承载
    /// 「测没测过」——那一位见 [`Exit::untested`]。读取不转移责任。
    pub fn cause(&self) -> String {
        match &self.kind {
            ExitKind::Unsure(w) => w.cause.name().to_string(),
            other => format!("{other:?}"),
        }
    }
    /// 未决的原因与细节（步 36 G3）；已决出口为 `None`
    pub fn why(&self) -> Option<&Why> {
        match &self.kind {
            ExitKind::Unsure(w) => Some(w),
            _ => None,
        }
    }
    /// 未决原因的细节（`insufficient` 缺的槽名、`fail` 的失败文字）；没有为 `None`
    pub fn detail(&self) -> Option<&str> {
        self.why().and_then(|w| w.detail.as_deref())
    }
    pub fn label(&self) -> String {
        match &self.kind {
            ExitKind::Act => "act".into(),
            ExitKind::Ignore => "ignore".into(),
            ExitKind::Pick(k) => format!("pick({k})"),
            ExitKind::At(l) => format!("at({l})"),
            // 那一位进 label，因为 `exit_kind(e)` 是程序**自己带进返回值**的审计面
            // （出口不进 `Ledger`——那里只有 Judge/Effect/Ask 三种条目）。
            // 步 36 G3（B197、裁定六十六）：`untested` 不再是原因，只作正交位挂在路由键后面
            ExitKind::Unsure(w) => match &self.untested {
                Some(carrier) => format!("unsure({}|untested:{carrier})", w.text()),
                None => format!("unsure({})", w.text()),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pending {
    pub cause: String,
    pub key: String,
    pub site: Span,
    pub detail: String,
}
