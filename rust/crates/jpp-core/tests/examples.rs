//! 检查器对着前端已经写好的 `.jpp` 源码跑一遍。
//!
//! 这是检查器最要紧的验收：**不误杀**。三份样例（组合、自适应选问、部分候选续解）都必须一条错都不报；
//! 报了就是检查器的问题，不是样例的问题——样例归前端，core 不改它们。
//! 诊断在这里按文件打印出来，前端据此对齐报文格式。

use std::path::Path;

use jpp_core::check;
use jpp_frontend::{lower, parse};

fn report_for(file: &str) -> (String, jpp_core::Report) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(file);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()));
    let parsed =
        parse(&source).unwrap_or_else(|d| panic!("{} 解析失败：{}", file, d.render(file, &source)));
    let core = lower(&parsed)
        .unwrap_or_else(|d| panic!("{} lower 失败：{}", file, d.render(file, &source)));
    (source, check(&core))
}

#[test]
fn 样例源码一条错都不该报() {
    for file in ["composition.jpp", "adaptive.jpp", "partial.jpp"] {
        let (source, report) = report_for(file);
        let errors: Vec<String> = report
            .errors()
            .iter()
            .map(|d| {
                format!(
                    "  {} @ {}",
                    d.render(),
                    &source[d.span.start..d.span.end.min(source.len())]
                )
            })
            .collect();
        assert!(
            errors.is_empty(),
            "{file} 被检查器误杀：\n{}",
            errors.join("\n")
        );
        if !report.warnings().is_empty() {
            println!("{file} 的提示：");
            for w in report.warnings() {
                println!(
                    "  {} @ {}",
                    w.render(),
                    &source[w.span.start..w.span.end.min(source.len())]
                );
            }
        }
    }
}

/// 前端给的三个错误样例：检查器或解析器要认出来，且位置落在源码里。
#[test]
fn 错误样例要被认出来() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/errors");
    let path = dir.join("missing-budget.jpp");
    let source = std::fs::read_to_string(&path).expect("读得到缺预算样例");
    let core = lower(&parse(&source).expect("语法本身没问题")).expect("lower 通过");
    let report = check(&core);
    let d = report
        .find("E12")
        .unwrap_or_else(|| panic!("缺预算应当报 E12：\n{}", report.render()));
    assert!(d.span.end <= source.len(), "位置要落在源码里");
}
