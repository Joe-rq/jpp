//! `fit` 注册表。（步 4c 自 `effects.rs` 原样搬出）

use std::collections::HashMap;
use std::rc::Rc;

/// `fit` 注册表（`12`:319「每个 `FitRef` 的训练集 id、特征键、指纹种类、错误率、版本；
/// 注册约束见 §2.9」，:314「**只能训练产生**」）。
///
/// `fit` 是第七种形式之外的**桥**：它把跨题的多个读数合成**一个仍然是读数的东西**，
/// 因而仍要过线。作者不用它也能合并两道题（`cut` 出两个出口再写 `if`），
/// 但那样一来**合并这一步的不确定性就消失了**——两个 `act` 合出来的结论看着和一个 `act`
/// 一样确定，而它其实经过了一个没有校准过的函数。
pub struct FitRecord {
    /// 特征：`(校准键, 指纹种类)`，**逐项**要与输入读数相同（J-04）
    pub features: Vec<(String, String)>,
    /// 训练样本数（J-16：`n ≥ max(50, 20×特征数)`）
    pub n: u64,
    /// 训练集 id（J-16：训练集 ≠ 保形集）
    pub trained_from: String,
    #[allow(clippy::type_complexity)]
    pub f: Rc<dyn Fn(&[f64]) -> f64>,
}

#[derive(Default)]
pub struct FitRegistry {
    pub fits: HashMap<String, Rc<FitRecord>>,
}

impl FitRegistry {
    pub fn new() -> FitRegistry {
        FitRegistry::default()
    }
    /// 不核约束的登记（测试与内部用）
    pub fn register(
        &mut self,
        name: &str,
        features: &[(&str, &str)],
        n: u64,
        trained_from: &str,
        f: impl Fn(&[f64]) -> f64 + 'static,
    ) {
        let features = features
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        self.fits.insert(
            name.to_string(),
            Rc::new(FitRecord {
                features,
                n,
                trained_from: trained_from.into(),
                f: Rc::new(f),
            }),
        );
    }
    /// 核 J-16 的样本数约束再登记：`n ≥ max(50, 20×特征数)`。
    /// **样本不够就不该用它下结论**——这条在登记时拦，比在调用时拦早。
    pub fn register_checked(
        &mut self,
        name: &str,
        features: &[(&str, &str)],
        n: u64,
        trained_from: &str,
        f: impl Fn(&[f64]) -> f64 + 'static,
    ) -> Result<(), String> {
        let need = std::cmp::max(50, 20 * features.len() as u64);
        if n < need {
            return Err(format!(
                "J-16: fit {name} 的训练样本 n={n} 不足 max(50, 20×{}) = {need}",
                features.len()
            ));
        }
        self.register(name, features, n, trained_from, f);
        Ok(())
    }
}
