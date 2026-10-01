//! 步 30 B 段测试共用：造「有证书」的校准记录。价值按主会话裁定四十六的第二版从记录的混淆矩阵算，所以夹具改的是样本，
//! 不是 `unsure_rate` 字段。
//!
//! 模板是题库 answers_question 的记录（`bank/entries/1d77f7a203b4258048dfefea/calib`：是非题、题类 attr、上岗、有选中证书、
//! 线 hi 0.56 / lo 0.36、δ 0.04，80 条样本，混淆 [39,0,0] / [1,39,1]）。每条夹具记录：前 `j` 条样本的 p 改为 0.45（落进
//! 未决带），需要小样本时只留前 `keep` 条；缺省经 `CalibStore::load_raw` 装载（不重跑认证）。数见过程记录 工程-步30 §7.9、§10。
#![allow(dead_code)]

use jpp::effects::CalibStore;
use std::path::PathBuf;

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn 模板() -> serde_json::Value {
    let dir = root().join("bank/entries/1d77f7a203b4258048dfefea/calib");
    let f = std::fs::read_dir(&dir)
        .expect("模板目录")
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|x| x == "json"))
        .expect("模板记录");
    serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap()
}

/// 一条夹具记录
pub struct 记 {
    pub key: &'static str,
    /// 前 j 条样本改为未决
    pub j: usize,
    /// 只留前 keep 条
    pub keep: Option<usize>,
    /// 题类（缺省沿用模板的 attr）
    pub kind: Option<&'static str>,
    /// 记录状态（缺省沿用模板的「上岗」；Z0385 用「停岗候选」「停岗」）
    pub status: Option<&'static str>,
}

/// 每项（键, 前 j 条改为未决, 只留前 keep 条），题类 attr，`load_raw` 装载
pub fn 认证库(items: &[(&str, usize, Option<usize>)]) -> CalibStore {
    写库(
        items
            .iter()
            .map(|(k, j, keep)| (k.to_string(), *j, *keep, None, None))
            .collect(),
        false,
    )
}

/// 同上，逐条可改题类；`重跑` 为真时经 `CalibStore::load` 装载（重跑认证，改过样本的记录会降为夹具）
pub fn 认证库2(items: &[记], 重跑: bool) -> CalibStore {
    写库(
        items
            .iter()
            .map(|r| (r.key.to_string(), r.j, r.keep, r.kind, r.status))
            .collect(),
        重跑,
    )
}

#[allow(clippy::type_complexity)]
fn 写库(
    items: Vec<(String, usize, Option<usize>, Option<&str>, Option<&str>)>,
    重跑: bool,
) -> CalibStore {
    // 同一进程里测试并行跑：目录名带进程号与自增序号，不会撞（只用时间戳曾撞过，一个测试删了另一个的目录）
    static 序: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = root().join(format!(
        "target/value-support-{}-{}",
        std::process::id(),
        序.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    // 先清空：之前某次运行在装载前 panic 留下的目录不能被一起装进来（复查 B0488-B 小处）
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (i, (key, j, keep, kind, status)) in items.iter().enumerate() {
        let mut t = 模板();
        t["key"] = serde_json::json!(key);
        if let Some(k) = kind {
            t["kind"] = serde_json::json!(k);
        }
        if let Some(st) = status {
            t["status"] = serde_json::json!(st);
        }
        let s = t["samples"].as_array_mut().unwrap();
        if let Some(m) = keep {
            s.truncate(*m);
        }
        for x in s.iter_mut().take(*j) {
            x["p"] = serde_json::json!(0.45);
        }
        std::fs::write(dir.join(format!("r{i}.json")), t.to_string()).unwrap();
    }
    let s = if 重跑 {
        CalibStore::load(&dir).expect("装载")
    } else {
        CalibStore::load_raw(&dir).expect("装载")
    };
    let _ = std::fs::remove_dir_all(&dir);
    s
}
