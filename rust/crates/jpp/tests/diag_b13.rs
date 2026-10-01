//! B13 诊断层第一批：每条规则一条命中、一条不命中（`21` 步 5）。
//! 依据：B13（`12` §3 J-17 后「诊断层规则集第一批」）。

use jpp::Span;
use jpp::check::diag::{DiagCx, QuestionLit, diagnose_fill, diagnose_question};

fn q(op: &'static str, text: &'static str) -> Vec<String> {
    let span = Span { start: 0, end: 0 };
    diagnose_question(&QuestionLit::new(op, text, false, span), &DiagCx::default())
        .into_iter()
        .map(|d| d.rule)
        .collect()
}
fn f(template: &str, fills: &[(&str, &str)]) -> Vec<String> {
    let span = Span { start: 0, end: 0 };
    let fills: Vec<(String, Option<String>)> = fills
        .iter()
        .map(|(k, v)| (k.to_string(), Some(v.to_string())))
        .collect();
    diagnose_fill("test", template, &fills, span, &DiagCx::default())
        .into_iter()
        .map(|d| d.rule)
        .collect()
}

#[test]
fn 排除条款_命中与不命中() {
    assert!(
        q("test", "这段话是否直接写出了钢琴？（只是话题相关不算）")
            .contains(&"W-diag-exclusion".to_string())
    );
    assert!(
        !q("test", "这段话的内容是否与钢琴这个话题相关？")
            .contains(&"W-diag-exclusion".to_string())
    );
}

#[test]
fn 一题两问_命中与不命中() {
    assert!(
        q("test", "候选人是否会写网页，并且是否懂数据库？")
            .contains(&"W-diag-two-judgments".to_string())
    );
    assert!(!q("test", "候选人是否会写网页？").contains(&"W-diag-two-judgments".to_string()));
}

#[test]
fn 开放问句配是非题_命中与不命中() {
    assert!(q("test", "这段话讲的是什么城市？").contains(&"W-diag-open-question".to_string()));
    assert!(q("test", "Why did the build fail?").contains(&"W-diag-open-question".to_string()));
    assert!(
        !q("test", "这段话是否提到了什么城市名？").contains(&"W-diag-open-question".to_string())
    );
    // select 本来就是在候选里挑，开放问句合法
    assert!(!q("select", "哪个候选最合适？").contains(&"W-diag-open-question".to_string()));
}

#[test]
fn 提到类须声明外延_命中与不命中() {
    assert!(q("test", "这段话是否提到了钢琴？").contains(&"W-diag-mention-scope".to_string()));
    assert!(
        !q("test", "这段话是否直接写出了钢琴，或用代称明确指向它？")
            .contains(&"W-diag-mention-scope".to_string())
    );
}

#[test]
fn 元题_命中与不命中() {
    assert!(q("test", "这段材料是否有助于判断作者立场？").contains(&"W-diag-meta".to_string()));
    assert!(!q("test", "这段有观点吗？").contains(&"W-diag-meta".to_string()));
}

#[test]
fn 抽象概念配直接写出_命中与不命中() {
    assert!(
        f("这段话是否直接写出了{c}？", &[("c", "职业焦虑")])
            .contains(&"W-diag-abstract-direct".to_string())
    );
    assert!(
        !f("这段话是否直接写出了{c}？", &[("c", "钢琴")])
            .contains(&"W-diag-abstract-direct".to_string())
    );
    // 话题相关类谓词配抽象概念不报
    assert!(
        !f("这段话是否与「{c}」这个话题相关？", &[("c", "职业焦虑")])
            .contains(&"W-diag-abstract-direct".to_string())
    );
}

#[test]
fn 填法缺槽多槽_命中与不命中() {
    assert!(f("{a}是否在{b}里？", &[("a", "苹果")]).contains(&"W-diag-fill".to_string()));
    assert!(
        f("{a}是否在清单里？", &[("a", "苹果"), ("x", "多余")])
            .contains(&"W-diag-fill".to_string())
    );
    assert!(
        !f("{a}是否在{b}里？", &[("a", "苹果"), ("b", "清单")])
            .contains(&"W-diag-fill".to_string())
    );
}

#[test]
fn 模板槽名不参与词面匹配() {
    // 槽名 mention 不应触发「提到」规则
    let span = Span { start: 0, end: 0 };
    let r: Vec<String> = diagnose_question(
        &QuestionLit::new("test", "{mention}是否成立？", true, span),
        &DiagCx::default(),
    )
    .into_iter()
    .map(|d| d.rule)
    .collect();
    assert!(r.is_empty(), "{r:?}");
}

/// Z0534：英文元题按形状认（帮助、用处一类词 + 判断、回答、决定一类词），中文同一形状；两面
#[test]
fn 元题_按形状_英文与中文() {
    for t in [
        "Does this material help judge whether the plan is feasible?",
        "Would this passage be useful for deciding the case?",
        "这段话对确定责任人有用吗？",
    ] {
        assert!(q("test", t).contains(&"W-diag-meta".to_string()), "{t}");
    }
    for t in [
        "Does the plan list a budget?",
        "Is the material in English?",
        "这段有观点吗？",
    ] {
        assert!(!q("test", t).contains(&"W-diag-meta".to_string()), "{t}");
    }
}

/// Z0534：英文一题两问按形状认（两处 whether；both … and；and / or 后接助动词起的第二个问句）；名词并列不算
#[test]
fn 一题两问_按形状_英文() {
    for t in [
        "Is the company hiring now and does the candidate fit the open role?",
        "Whether the plan is funded and whether it is staffed?",
        "Is it both cheap and fast?",
    ] {
        assert!(
            q("test", t).contains(&"W-diag-two-judgments".to_string()),
            "{t}"
        );
    }
    for t in [
        "Does it build payments and ledgers?",
        "Is the office in Taipei and Tokyo?",
        "Is the plan feasible?",
    ] {
        assert!(
            !q("test", t).contains(&"W-diag-two-judgments".to_string()),
            "{t}"
        );
    }
}

/// Z0534 复核收窄：评判类真题不判元题（帮助类词没有直接支配判断动词，或判断类是名词、是 answer）
#[test]
fn 元题_评判类真题不误拒() {
    for t in [
        "Does the response help answer the user's question?",
        "Is the answer relevant to the question the user asked?",
        "Does the reviewer's comment help the authors decide what to fix?",
        "Is the evidence relevant to the court's decision?",
    ] {
        assert!(!q("test", t).contains(&"W-diag-meta".to_string()), "{t}");
    }
    // 已知误报：与「help decide」这类元题形状相同，按题面分不开（复核-线A-Z0534 第 88 行），这里钉住现状
    assert!(
        q("test", "Did the manager help decide the budget?").contains(&"W-diag-meta".to_string())
    );
    // 收窄后仍拦的真元题
    for t in [
        "Is this passage helpful in deciding the case?",
        "Would this help to judge the claim?",
        "Is it relevant for assessing the risk?",
    ] {
        assert!(q("test", t).contains(&"W-diag-meta".to_string()), "{t}");
    }
}
