//! pass 开关（步 13a 从 `jpp-core/src/interp/mod.rs` 原样搬来；`20` §2.3：`Passes` 在 `jpp-plan` 唯一定义）。

/// 编译 pass 的开关（`12` §4「每个一个开关，给消融留门」）。
///
/// **是 9 个不是 7 个。** `12` §4 的表写了 7 行，但同文件 v0.1.1 修订记录 1（:610）写着
/// 「§4 **增两个 pass**」——judge 推测提升与循环向量化——而那张表从没改过。同一过期数字
/// 在三处独立写着（依据建造顺序、依据对照表、Python 模块 docstring）。这是**文档缺陷不是
/// 设计分歧**，所以这里按 9 个算，并在 INTERFACE.md 记下这处不一致（总控另走附注提请裁定）。
///
/// **开关是消融的唯一载体**：没有它，「关掉融合成本涨多少」这种账没法再核一次。
/// 七个 pass 里 core 现在落地两个——`fuse`（同状态同层合成一次调用）与 `ledger`
/// （账本键与重放）。其余五个（`lift` / `fission` / `lower` / `schedule` / `plan`）
/// **字段先留着并默认 false**，`enabled()` 对未落地的 pass 恒返回 false，
/// 这样「开关开着但什么也没发生」不会被误读成「这个 pass 在工作」。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passes {
    /// 提升：直线段内不依赖前面结果的 judge 提到同一层（**已落地**，有消融账）
    pub lift: bool,
    /// 融合：同状态、同层的题合成一次调用（已落地）
    pub fuse: bool,
    /// 裂变：超窗的槽按窗切开、`cut` 处按题的操作合回（**步 23b 落地，近似档**：只在题声明
    /// `fission: "approx"` 时起作用，V8 实测一致率 0.897；不声明的题不受开关影响）
    pub fission: bool,
    /// 下沉：select → choice / K-noul，measure → score（**未落地**）
    pub lower: bool,
    /// 调度：层内并发、gen/ask 一登记就发（**未落地**）
    pub schedule: bool,
    /// 预算：计划期估计调用数、层数、费用、时延，可靠下界超预算即拒（**步 22 落地**，`passes/plan.rs`）
    pub plan: bool,
    /// 键与重放：账本键、重放不付费（已落地）
    pub ledger: bool,
    /// judge 推测提升：同状态、静态可达、中间无 do/gen/ask/transform 的 judge 站点
    /// 随首个站点一起发（v0.1.1 修订记录 1，**已落地**；共状态零边际、异状态多花一次调用，
    /// 分情况实测见 INTERFACE）
    pub speculate: bool,
    /// 循环向量化：无 loop-carried 依赖、体内无 do/ask 的 for 体自动成一层
    /// （同上，证明不了要报 W-serial，**未落地**）
    pub vectorize: bool,
    /// 惰性过桥（B94，步 23c）：`cut` 只把读数与线绑定，出口在第一次被检视时才解析、刷新。
    /// 关掉即改前行为：`cut` 当场刷新并解析。
    pub lazy_cut: bool,
    /// 层内挑选（B43、B51-C1，步 22 / B0487）：本层超出剩余预算时真站点先发、跨状态推测先让。
    /// 关掉即消融矩阵第 ⑧ 行「`select_within` 降为登记顺序」（`21` §六·5）
    pub select_within: bool,
    /// 层内挑选按关键路径排（步 30 / B0488，K-135 层数目标）：真站点组里下游层数大的先发。关掉即步 22 的登记顺序；
    /// `select_within` 关时它不起作用。价值密度另设开关（B 段），消融三臂分得清变量
    pub critical_path: bool,
    /// 层内挑选按价值密度排（步 30 B 段 / B0488，B7、B43；`planner_value`）：同下游层数（或关键路径关时同类别）内价值大的
    /// 组先发。与 `critical_path` 分开，消融四臂（都关 / 只关键路径 / 只价值 / 都开）
    pub value_density: bool,
    /// 伴随题的发法（B0492 S5；裁定四十一）：`Off` / `Same` / `Parallel`。默认 `Same`，由 H2 串扰实验定
    /// （`地基/实测/伴随题串扰-H2-2026-09-30/结果.md`：超 δ 的条目 5.9%，低于 20%）
    pub companions: jpp_ir::plan::CompanionMode,
}

impl Default for Passes {
    /// 已落地的默认开，未落地的默认关——默认值不承诺未落地的东西在工作
    fn default() -> Passes {
        Passes {
            lift: true,
            fuse: true,
            // 步 23b：落地即默认开；只有声明 `fission: "approx"` 的题才会被切，不声明的程序输出不变
            fission: true,
            lower: false,
            schedule: false,
            plan: true,
            ledger: true,
            speculate: true,
            vectorize: true,
            lazy_cut: true,
            select_within: true,
            critical_path: true,
            value_density: true,
            // H2 串扰实验（地基/实测/伴随题串扰-H2-2026-09-30，超 δ 5.9% < 20%）定：同请求（裁定四十一）
            companions: jpp_ir::plan::CompanionMode::Same,
        }
    }
}

impl Passes {
    /// 全关：消融的对照臂。`ledger` 也关得掉，关了就不查账本、每次都真发
    pub fn none() -> Passes {
        Passes {
            lift: false,
            fuse: false,
            fission: false,
            lower: false,
            schedule: false,
            plan: false,
            ledger: false,
            speculate: false,
            vectorize: false,
            lazy_cut: false,
            select_within: false,
            critical_path: false,
            value_density: false,
            companions: jpp_ir::plan::CompanionMode::Off,
        }
    }
    /// 这个 pass 现在真的会起作用吗。**未落地的一律 false**，不管开关怎么设——
    /// 否则「开关开着」会被误读成「这个 pass 在工作」。
    pub fn enabled(&self, name: &str) -> bool {
        match name {
            "fuse" => self.fuse,
            "ledger" => self.ledger,
            // 未落地：INTERFACE §七记着它们欠什么
            "lift" => self.lift,
            "speculate" => self.speculate,
            "vectorize" => self.vectorize,
            "lazy_cut" => self.lazy_cut,
            "plan" => self.plan,
            "select_within" => self.select_within,
            "fission" => self.fission,
            "lower" | "schedule" => false,
            "critical_path" => self.critical_path,
            "value_density" => self.value_density,
            _ => false,
        }
    }
    /// 已落地的 pass 名字（给 CLI 与诊断用）
    pub fn landed() -> &'static [&'static str] {
        &[
            "lift",
            "fuse",
            "ledger",
            "speculate",
            "vectorize",
            "lazy_cut",
            "plan",
            "select_within",
            "fission",
            "critical_path",
            "value_density",
        ]
    }
}
