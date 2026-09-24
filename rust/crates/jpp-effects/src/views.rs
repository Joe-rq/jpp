//! 只读视图 trait（`20` §2.3）：检查器、运行时经它们读校准、拟合、料库与跨运行缓存，
//! 不依赖 `jpp-calib`、`jpp-store` 的具体类型（`20` §2.2 第 4 条、§2.3 `jpp-runtime` 禁止依赖）。
//!
//! 步 9 只声明，没有实现者。签名只用今天已存在的类型：校准键仍是字符串（结构化 `CalibKey` 在步 20a），
//! `ReadingMeta` 尚不存在，所以 `CalibView` 按键查而不是按读数元数据构键（S7 在步 11 落位时再收紧）。
//! 与 `20` §2.3 目标签名的逐处差异记在 `地基/过程记录/工程-步9.md`。

use jpp_ir::key::CacheKey;
use jpp_value::value::Mat;

/// 一张证书的只读视图：桥用它选代价线、判凭据、查标签来源（步 11b）。
#[derive(Clone, Debug, PartialEq)]
pub struct CertView {
    /// 风险目标：放行区里的假放行率上界
    pub alpha: f64,
    /// 认证住的线
    pub hi: f64,
    /// 这条线由哪个代价矩阵定；`None` = 无代价矩阵
    pub cost: Option<(f64, f64)>,
    /// 标签来源可疑（未声明怎样选的标签）：强出口建在它上面要留痕
    pub label_source_suspicious: bool,
    /// 标签来源的描述（告警原文用）
    pub label_source: String,
    /// 试用 α 认证的证书（B72）：出口可路由，不放行不可逆 `do`
    pub trial: bool,
    /// 认证半上的已决条数（上侧；`W-trial-line` 文本用）
    pub n_accepted: usize,
}

/// 一条校准记录的只读视图：`cut` 判序与告警需要的全部字段（步 11b 定全，`20` §2.3）。
/// 记录类型在 `jpp-calib`；运行时只经这个视图读，不依赖记录类型。
#[derive(Clone, Debug, PartialEq)]
pub struct Lookup {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    /// 「上岗」「停岗候选」「停岗」「冷」「待真值」
    pub status: String,
    /// 记录的 δ；`None` = 用画像的 δ
    pub delta: Option<f64>,
    /// 宿主 `put` 写的夹具记录（J-03：不算放行不可逆 `do` 的可信合取项）
    pub fixture: bool,
    /// 保形集标识（J-16：与 fit 训练集不相交）
    pub set_id: String,
    /// 真值通道的门控文本；`None` = 未经真值通道
    pub truth_gate: Option<String>,
    /// 全部证书（按记录内地址顺序）
    pub certs: Vec<CertView>,
    /// 选中的那张证书（α 最小；同 α 取线更高者），见 `jpp-calib` 的 `选中的证书`
    pub selected: Option<CertView>,
    /// 重跑分歧检验：`Some(true)` = 错误独立，`band → 重跑` 可启用（B9/B28）
    pub rerun_independent: Option<bool>,
    /// 认证范围的材料指纹（B68）；`None` = 不核范围
    pub scope: Option<jpp_value::stat::ScopeRanges>,
}

impl Lookup {
    /// 这条线是不是夹具线（B29）：夹具记录，或没有一张证书撑着。
    pub fn fixture_line(&self) -> bool {
        self.fixture || self.selected.is_none()
    }
}

/// 校准库的只读视图。`jpp-calib::CalibStore` 实现它；运行时与 `strength` 只经它读校准（步 11b）。
///
/// 与 `20` §2.3 目标签名 `lookup_for(&ReadingMeta)` 的差距：校准键仍是字符串（结构化 `CalibKey`
/// 在步 20a）；查找链的各级记录由校准侧 `chain` 一次给出（桥不构造键，S7），选哪一级仍由桥按状态判；
/// 类键一级（B34）步 20b 接入。
pub trait CalibView {
    /// 按校准键查记录；库里没有该键时 `None`。
    fn lookup(&self, key: &str) -> Option<Lookup>;
    /// 同上，库里没有时给冷记录（与 `CalibStore::get` 同口径：线是缺省值，不是线）。
    fn line(&self, key: &str) -> Lookup;
    /// 校准库的哈希（进账本头，J-18）
    fn hash(&self) -> String;
    /// 该键可用于 J-10 的未决率（只认上岗、且认证时的 δ 与现在一致）
    fn unsure_rate(&self, key: &str) -> Option<f64>;
    /// 这条记录在这种题型上的 δ：记录自带优先，否则取画像的
    fn delta_for(&self, rec: &Lookup, op: jpp_ir::key::Op) -> f64 {
        let p = self.profile();
        rec.delta.unwrap_or(match op {
            jpp_ir::key::Op::Test => p.delta.0,
            jpp_ir::key::Op::Select => p.delta.1,
            jpp_ir::key::Op::Measure => p.delta.2,
        })
    }
    /// 这次运行加载的能力画像
    fn profile(&self) -> &crate::profile::Profile;
    /// 该键的无标签漂移报告（参照 = 带标注样本，近期 = 无标注样本）
    fn drift(&self, key: &str) -> Option<jpp_value::stat::DriftReport>;
    /// 记录全文（进账本 `calib_used`，只凭账本重放时据此补回）
    fn record_json(&self, key: &str) -> Option<serde_json::Value>;
    /// 这道读数的查找链（B44：题键 → 题式键 → 类键 → 冷；模式键不在链上）。键由校准侧构造。
    /// `key` 是 `cut` 用的有效校准键，同时是类别标签（B34）。
    fn chain(&self, key: &str, form_hash: Option<&str>) -> Chain;
}

/// 查找链上的一级：键与记录（无记录时是冷记录）。
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub key: String,
    pub rec: Lookup,
}

/// 一道读数的查找链（步 11b-2）：题级一定有；题式级在读数由题式填出时才有；
/// 类级（步 20b，B34）在键是作者写的类别标签时才有（`fit:` 键没有）。
#[derive(Clone, Debug, PartialEq)]
pub struct Chain {
    pub question: Link,
    pub form: Option<Link>,
    pub class: Option<Link>,
}

/// 拟合记录的只读视图（`fit`）。记录类型在 `jpp-calib`，这里用关联类型，不反向依赖。
pub trait FitView {
    type Record;
    fn get(&self, fit_ref: &str) -> Option<&Self::Record>;
}

/// 料库端口：内容寻址存材料（步 18 由 `jpp-store` 实现）。
pub trait MatStorePort {
    fn put(&mut self, m: &Mat) -> String;
    fn get(&self, addr: &str) -> Option<Mat>;
    /// 该材料上做过的观察（缓存键）
    fn marks(&self, addr: &str) -> Vec<CacheKey>;
}

/// 跨运行的缓存读数（B40，步 19 启用）。
#[derive(Clone, Debug, PartialEq)]
pub struct CachedReading {
    /// 读数记录（账本条目里的原样 JSON；结构化的 `ReadingRecord` 在步 10 之后）
    pub record: serde_json::Value,
    /// 来源账本与条目
    pub source: String,
}

/// 跨运行读数查找（步 19 由 `jpp-store` 实现）。
pub trait CacheLookup {
    fn get(&self, k: &CacheKey) -> Option<CachedReading>;
}
