//! `fission` pass 的消融（步 23b，B0476；预注册 `地基/过程记录/工程-步23b.md` §六·2 第 2 条）。
//!
//! 两处分工（主控 2026-09-29 答复第 4 条）：本文件测计划期一半——开关语义、`Plan.fission` 置位、切点纯函数
//! （复现 V8、window-over 的块数与块长）、打分题计数合回；运行时层（开关关掉与不声明同输出、各合回规则的出口、
//! select 第二层）在 `crates/jpp/tests/fission_ablation.rs`。断言写死值。

use jpp_ir::key::canon;
use jpp_plan::passes::fission::{CutKind, cut_once, measure_count, split};
use jpp_plan::{Passes, plan};
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// V8 的估计（`材料.py::tok`）：字符数 ÷ 1.3 取整加一
fn v8_tok(s: &str) -> usize {
    (s.chars().count() as f64 / 1.3) as usize + 1
}

/// 运行时的估计（`Mat::tokens()`）：规范化 JSON 的字符数 ÷ 1.3 取整加一（文本材料带引号，换行转义算两个字符）
fn rt_tok(s: &str) -> usize {
    (canon(&Json::String(s.to_string())).chars().count() as f64 / 1.3) as usize + 1
}

#[test]
fn 开关_落地即默认开_全关臂关() {
    assert!(Passes::default().enabled("fission"));
    assert!(!Passes::none().enabled("fission"));
    assert!(Passes::landed().contains(&"fission"));
    let off = Passes {
        fission: false,
        ..Passes::default()
    };
    assert!(!off.enabled("fission"));
}

#[test]
fn 开关写进计划() {
    struct 无名;
    impl jpp_ir::ir::NameTable for 无名 {
        fn classify(&self, _: &str) -> jpp_ir::ir::NameClass {
            jpp_ir::ir::NameClass::Plain
        }
        fn slots(&self, _: jpp_effects::EffectId) -> Vec<&'static str> {
            vec![]
        }
    }
    let ast = jpp_syntax::parse("budget {calls: 1, cost: 0.01};\n1").expect("解析");
    let p = jpp_syntax::lower(&ast, &无名).expect("降级");
    assert!(plan(&p, &Passes::default()).fission);
    assert!(!plan(&p, &Passes::none()).fission);
}

/// 预注册 §六·2 第 2 条前半：V8 估计、W = 250，六份材料的前块、后块、切点种类逐字节同 `材料.json`
#[test]
fn 切点_复现v8六份材料() {
    let p = root().join("../实测/V8-裂变-2026-09-29/材料.json");
    if !p.exists() {
        // 公开仓库没有研究区的 实测/ 目录：跳过（tools/sync-rust-from-research.sh 改写）
        eprintln!("跳过：{} 不在本仓库", p.display());
        return;
    }
    let mats: Vec<Json> =
        serde_json::from_str(&std::fs::read_to_string(&p).expect("材料.json")).unwrap();
    assert_eq!(mats.len(), 6);
    for m in &mats {
        let (f, b, k) = cut_once(m["whole"].as_str().unwrap(), 250, &v8_tok);
        assert_eq!(f, m["front"].as_str().unwrap(), "{} 前块", m["id"]);
        assert_eq!(b, m["back"].as_str().unwrap(), "{} 后块", m["id"]);
        assert_eq!(
            k.name(),
            m["cut_kind"].as_str().unwrap(),
            "{} 切点种类",
            m["id"]
        );
        assert!(matches!(k, CutKind::Sentence | CutKind::Word));
    }
}

/// 预注册 §六·2 第 2 条后半：运行时估计、W = 500，window-over 三篇 4 / 8 / 8 块与各块估计
#[test]
fn 切点_window_over三篇() {
    let p = root().join("examples/fixtures/window-over.json");
    let fx: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let mut texts: Vec<String> = vec![];
    for o in fx["observations"].as_array().unwrap() {
        let t = o["on"][0].as_str().unwrap().to_string();
        if !texts.contains(&t) {
            texts.push(t);
        }
    }
    let got: Vec<Vec<usize>> = texts
        .iter()
        .map(|t| split(t, 500, &rt_tok).iter().map(|b| rt_tok(b)).collect())
        .collect();
    assert_eq!(
        got,
        vec![
            vec![311, 377, 401, 417],
            vec![346, 343, 295, 237, 324, 270, 346, 282],
            vec![345, 345, 368, 251, 441, 364, 375, 381],
        ]
    );
    assert_eq!(
        texts.iter().map(|t| rt_tok(t)).collect::<Vec<_>>(),
        vec![1502, 2429, 2858]
    );
}

#[test]
fn 打分计数合回_强克莱尼() {
    // 最多档票数 > 次多档 + 未决块数才已决
    assert_eq!(measure_count(&[3, 3, 3, 1], 1), Ok(3));
    assert_eq!(measure_count(&[3, 3, 1], 1), Err(false));
    assert_eq!(measure_count(&[3, 1], 0), Err(true));
    assert_eq!(measure_count(&[], 2), Err(false));
}
