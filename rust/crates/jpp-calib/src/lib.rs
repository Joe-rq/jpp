//! J++ 校准（`20` §2.3 `jpp-calib`，L3）：校准记录与证书、校准库（`commission` 是唯一产线）、
//! `fit` 注册表、真值通道、强度估计。只依赖 `jpp-ir`、`jpp-value`、`jpp-effects`。
//!
//! 运行时与检查器经 `jpp_effects::views::CalibView` 读线，不直接依赖这里的类型（`20` §2.2 第 4 条）；
//! 步 11 起 `CalibStore` 实现 `CalibView`。`jpp-core` 在原路径重导出本 crate 的公共项。

pub mod calib;
pub mod fit;
pub mod strength;
pub mod truth;

pub use calib::*;
pub use fit::*;

use serde_json::Value as Json;

/// **校准库的哈希**（进账本头，J-18）。（步 11 自 `jpp-core/src/effects/profile.rs` 搬入，代码不变；文档里「反面教材就在下面几行」改为直接点名 `behavior_hash`，因为它不再在同一文件）
///
/// 为什么要它：**出口 = f(读数, 线)。读数进了账本，线没有。** 头里的 `profile_hash`
/// 记的是**档案里的缺省线**，而 `cut` 用的是**按键的线**——同一份账本换一批校准记录重放，
/// 读数一样、出口可以不一样，**而账本上看不出**。运行期写入口接上之后，
/// 校准记录在两次运行之间会长，所以这不再是理论上的可变。
///
/// **用排除法，不用列举法。** 覆盖整条记录，只减去一张封闭的「说明字段」清单。
/// 反面教材是 `behavior_hash`：它是列举法，**它自己的注释承认了失效方式**
/// ——「加字段时要同步这里——漏加的后果是『行为变了但摘要没变』」。列举法在有人加新字段时
/// **会说出一个假的「相同」**，而假的「相同」倒向放行那一侧。
///
/// **空库也有哈希，不是 `None`。** 头里的 `None` 只该有一个意思：**这份账本早于这个字段**。
/// 让空库也占 `None`，两种情形在账本上就分不开。
pub fn calib_hash(store: &CalibStore) -> String {
    /// 说明性字段：改了不影响执行，所以不进哈希。
    /// **今天这张表是空的**——`CalibRecord` 每个字段都承载行为（`hi`/`lo`/`n`/`status` 进 `cut`，
    /// `delta` 进 `delta_for`，`unsure_rate` 进 J-10，`set_id` 进 J-16，`samples` 是证据本身）。
    /// **空表不是摆设**：它是加 `note` 那类字段时该动的那一处，
    /// 有了它，新字段的默认归宿是「进哈希」而不是「被忘掉」。
    const 说明字段: &[&str] = &[];

    // BTreeMap：哈希不随写入顺序变，否则同一批线会因写入顺序不同报假 W-header
    let mut sorted: std::collections::BTreeMap<&str, Json> = Default::default();
    for (k, rec) in &store.records {
        let mut j = serde_json::to_value(rec).unwrap_or(Json::Null);
        if let Json::Object(m) = &mut j {
            m.retain(|f, _| !说明字段.contains(&f.as_str()));
        }
        sorted.insert(k.as_str(), j);
    }
    jpp_effects::profile::hash16(&Json::Array(sorted.into_values().collect()))
}

fn cert_view(c: &Cert) -> jpp_effects::views::CertView {
    jpp_effects::views::CertView {
        alpha: c.alpha,
        hi: c.hi,
        cost: c.cost,
        label_source_suspicious: c.label_source.可疑(),
        label_source: format!("{:?}", c.label_source),
        trial: c.grade == CertGrade::Trial,
        n_accepted: c.n_accepted,
    }
}

/// 记录 → 只读视图（运行时只见这个形状）。
pub fn lookup_of(r: &CalibRecord) -> jpp_effects::views::Lookup {
    jpp_effects::views::Lookup {
        key: r.key.clone(),
        hi: r.hi,
        lo: r.lo,
        n: r.n,
        status: r.status.clone(),
        delta: r.delta,
        fixture: r.fixture,
        set_id: r.set_id.clone(),
        truth_gate: r.truth.as_ref().map(|t| t.gate.clone()),
        certs: r.certs.values().map(cert_view).collect(),
        selected: r.选中的证书().map(cert_view),
        rerun_independent: r.rerun_independent,
        scope: r.scope.as_ref().and_then(|s| s.fingerprint.clone()),
    }
}

/// 校准库的只读视图（`20` §2.3）。步 11 起由 `CalibStore` 实现；步 11b 起运行时与 `strength` 只经它读校准。
impl jpp_effects::views::CalibView for CalibStore {
    fn lookup(&self, key: &str) -> Option<jpp_effects::views::Lookup> {
        self.records.get(key).map(lookup_of)
    }
    fn line(&self, key: &str) -> jpp_effects::views::Lookup {
        lookup_of(&self.get(key))
    }
    fn hash(&self) -> String {
        calib_hash(self)
    }
    fn unsure_rate(&self, key: &str) -> Option<f64> {
        self.records
            .get(key)
            .and_then(|r| self.usable_unsure_rate(r))
    }
    fn profile(&self) -> &jpp_effects::Profile {
        &self.profile
    }
    fn drift(&self, key: &str) -> Option<jpp_value::stat::DriftReport> {
        self.drift_of(key)
    }
    fn record_json(&self, key: &str) -> Option<serde_json::Value> {
        self.records
            .get(key)
            .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
    }
    fn chain(&self, key: &str, form_hash: Option<&str>) -> jpp_effects::views::Chain {
        let link = |k: String| jpp_effects::views::Link {
            rec: lookup_of(&self.get(&k)),
            key: k,
        };
        jpp_effects::views::Chain {
            question: link(key.to_string()),
            form: form_hash.map(|h| link(CalibStore::form_key(h))),
            // 类别标签是作者写的键；`fit:` 键与保留命名空间里的键不是类别标签（B34）
            class: (!key.starts_with("fit:") && !key.starts_with('\u{1f}'))
                .then(|| link(CalibStore::class_key(key))),
        }
    }
}

#[cfg(test)]
mod view_tests {
    use super::*;
    use jpp_effects::views::CalibView;

    /// 视图与记录一致：有键时逐字段相同，无键时 `None`（调用方按冷处理）；哈希即 `calib_hash`。
    #[test]
    fn calib_view_matches_the_record() {
        let mut s = CalibStore::new();
        assert!(s.lookup("k").is_none());
        let before = calib_hash(&s);
        let mut r = s.get("k");
        r.hi = 0.7;
        r.lo = 0.3;
        r.n = 40;
        r.status = "上岗".into();
        s.records.insert("k".into(), r);
        let l = s.lookup("k").unwrap();
        assert_eq!((l.hi, l.lo, l.n, l.status.as_str()), (0.7, 0.3, 40, "上岗"));
        assert_eq!(CalibView::hash(&s), calib_hash(&s));
        assert_ne!(CalibView::hash(&s), before);
    }

    /// B34：查找链的类级只给作者写的类别标签；`fit:` 键与保留命名空间里的键没有类级（步 20b）。
    #[test]
    fn chain_has_class_link_only_for_author_labels() {
        let s = CalibStore::new();
        let c = s.chain("k", None);
        assert_eq!(c.class.map(|l| l.key), Some(CalibStore::class_key("k")));
        assert!(s.chain("fit:f", None).class.is_none());
        assert!(s.chain(&CalibStore::form_key("h"), None).class.is_none());
    }
}
