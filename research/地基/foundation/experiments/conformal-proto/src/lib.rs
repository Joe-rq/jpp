//! 保形弃权域的**实验原型**。它自己**不再带一份保形实现**。
//!
//! # 为什么这个文件只剩装载器
//!
//! 这份原型先于内核写成，`certify` / `binomial_upper` / `n_needed_zero_error` /
//! `drift` / `cluster_subsample` / `Certificate` / `DriftReport` 当初都在这里。
//! 内核落地之后（`jpp_core::conformal`），**这里那几份就成了拷贝，而拷贝会静默分叉**
//! ——而且已经分叉过一次：
//!
//! ```text
//! proto:  binomial_upper(k, n, delta)
//! core:   binomial_upper(k, n, conf_delta)
//! ```
//!
//! **那次改名不是整理，是一条判断**：保形的 `δ` 是**置信水平**（「这个上界有多大把握」），
//! 档案里的 `δ` 是**迟滞带宽**（「读数抖多少不算变」）。**两个完全不同的保证，
//! 不许共用一个名字。** 这条判断在内核里做了，**而它永远不会因为拷贝没跟上而断编译**
//! ——两小时内内核改字段打断本 crate 编译两次，都是响亮的、当场修掉的；
//! **这一处是静默的，它在同一个文件里躺了一整轮没人看见。**
//!
//! 所以：**凡内核里已经有的，一律引用不复制。** 本 crate 只保留它自己独有的东西——
//! E-CAL 那批真读数的装载，以及 `tests/` 里那几条回归保护。
//!
//! 逐个对过的结果见 `交付-与内核的对照.md`：**除了参数名，八处实现逐字节相同**。

pub use jpp_core::conformal::*;

use jpp_core::effects::{CalibStore, LiteralMode, Sample};

/// E-CAL 那批真读数的夹具：`(读数 p, 这条读数蕴含的判断对不对, 对象段)`。
///
/// 由 `raw/e_cal/readings.jsonl` 按 `e_cal_写校准记录.py` 的同一套解码生成
/// （noul 73 / choice 74 / score 55 条）。**对象段是簇 id**——可交换性在我们这里
/// 不是被时间打破的，是被材料复用打破的。
pub fn ecal(题型: &str) -> Vec<(f64, bool, String)> {
    let j: serde_json::Value = serde_json::from_str(include_str!("../tests/ecal_fixture.json")).expect("夹具是合法 JSON");
    j[题型]
        .as_array()
        .unwrap_or_else(|| panic!("夹具里没有题型 {题型}"))
        .iter()
        .map(|r| (r[0].as_f64().expect("p"), r[1].as_i64().expect("label") == 1, r[2].as_str().expect("簇").to_string()))
        .collect()
}

/// 只要 `(p, 对不对)` 那两列。
pub fn ecal2(题型: &str) -> Vec<(f64, bool)> {
    ecal(题型).into_iter().map(|(p, l, _)| (p, l)).collect()
}

/// 把一个题型的夹具整批折进一条校准记录（走内核的运行期写入口 `absorb`）。
/// **带着簇 id**：声明按对象段认证却没有簇 id 是错，不是降级。
pub fn 装进(store: &mut CalibStore, key: &str, 题型: &str) {
    for (p, l, seg) in ecal(题型) {
        store
            .absorb(key, Sample {
                p: Some(p),
                label: Some(u8::from(l)),
                perms: 0,
                mode_share: None,
                mode: LiteralMode::default(),
                phys: 题型.to_string(),
                cluster: Some(seg),
            })
            .expect("夹具里的样本都合法");
    }
}
