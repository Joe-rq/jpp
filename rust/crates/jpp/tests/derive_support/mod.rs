//! 步 28 derive 测试的公用装置：写临时 `.jpp`（从 `lib/derive` 导入）、闭包端口按题面给读数、跑完返回
//! 契约值、账本与判断端口的调用次数。不发请求。
#![allow(dead_code)]

use jpp::effects::{CalibStore, EffectError, FnPort, GenResult, JudgeResult, Ports};
use jpp::ledger::{Entry, Ledger};
use jpp::value::{Answer, Question, State};
use jpp::{ActionRegistry, EntryArgs, Outcome, Session};
use serde_json::Value as Json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct 跑出 {
    pub out: Outcome,
    pub ledger: Ledger,
    pub calls: usize,
    pub gens: usize,
    /// 判断端口实际收到的每道题面（按收到的顺序）
    pub asked: Vec<String>,
}

/// `answer(题面, 题, 状态)` 给读数；`gen_out` 是生成器每次返回的候选（JSON 值）
pub fn 跑(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    gen_out: Vec<Json>,
) -> Result<跑出, String> {
    跑_带校准(src, answer, gen_out, &CalibStore::new())
}

/// 同 [`跑`]，带校准记录（例如题库条目的校准目录 `CalibStore::load`）
pub fn 跑_带校准(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    gen_out: Vec<Json>,
    calib: &CalibStore,
) -> Result<跑出, String> {
    跑_全(src, answer, gen_out, calib, None)
}

/// 同 [`跑_带校准`]，生成器端口声明输出 taint（B149；真生成器缺省 untrusted，夹具缺省不声明）
pub fn 跑_全(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    gen_out: Vec<Json>,
    calib: &CalibStore,
    gen_taint: Option<jpp::value::Taint>,
) -> Result<跑出, String> {
    跑_核(
        src,
        answer,
        move |_p: &str| gen_out.clone(),
        calib,
        gen_taint,
    )
}

/// 同 [`跑`]，生成器按提示给候选（`生成(提示)`；Z0517 的测试要「不同父题唤出不同候选」）
pub fn 跑_按提示(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
) -> Result<跑出, String> {
    跑_核(src, answer, 生成, &CalibStore::new(), None)
}

/// 同 [`跑_按提示`]，生成器端口声明输出 taint（B149；真生成器缺省 untrusted）
pub fn 跑_按提示_带污(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
    gen_taint: jpp::value::Taint,
) -> Result<跑出, String> {
    跑_核(src, answer, 生成, &CalibStore::new(), Some(gen_taint))
}

/// 同 [`跑_按提示`]，带画像（Z0497：读数边界带取画像 delta.<phys>.mid）
pub fn 跑_按提示_画像(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
    profile: jpp::effects::Profile,
) -> Result<跑出, String> {
    let mut calib = CalibStore::new();
    calib.profile = profile;
    跑_核(src, answer, 生成, &calib, None)
}

fn 跑_核(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
    calib: &CalibStore,
    gen_taint: Option<jpp::value::Taint>,
) -> Result<跑出, String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!(
        "target/derive-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.jpp");
    std::fs::write(
        &path,
        format!("import \"../../lib/derive/chain.jpp\";\n{src}"),
    )
    .unwrap();
    let loaded = jpp::syntax::loader::load(&path);
    let _ = std::fs::remove_dir_all(&dir);
    let loaded = loaded.map_err(|e| format!("装载：{e:?}"))?;
    let mut program = jpp::lower(&loaded.program).map_err(|e| format!("lower：{e:?}"))?;
    if 把关.with(|c| c.get()) {
        program.entry.guard = true;
    }
    let acts = ActionRegistry::new();
    let calls = std::cell::RefCell::new(0usize);
    let gens = std::cell::RefCell::new(0usize);
    let asked = std::cell::RefCell::new(Vec::<String>::new());
    let ports = Ports::new()
        .with(FnPort::judge("fixed-0", |s, qs| {
            // 伴随题「最缺哪类」换了 over 槽，同材料的一次调用会按状态分段多调一次闭包；只含伴随元题的那一段不计
            // （过程记录 5.23）
            if qs.iter().any(|q| 伴随中性(q, s).is_none()) {
                *calls.borrow_mut() += 1;
            }
            asked.borrow_mut().extend(qs.iter().map(|q| q.text.clone()));
            Ok::<_, EffectError>(JudgeResult {
                answers: qs
                    .iter()
                    .map(|q| 伴随中性(q, s).unwrap_or_else(|| answer(&q.text, q, s)))
                    .collect(),
                tokens: 0,
                cost: 0.0,
                mode_share: vec![],
                perms: vec![],
                confidence: vec![],
            })
        }))
        .with(FnPort::generate("fixed-0", |p, _c, _n, _r| {
            *gens.borrow_mut() += 1;
            Ok(GenResult {
                outputs: 生成(p),
                taint_out: gen_taint,
                ..Default::default()
            })
        }))
        .with(FnPort::ask("fixed-0", |_s, _q| {
            Err(EffectError("不该 ask".into()))
        }));
    let mut ledger = Ledger::new();
    let out = Session::new(ports, calib, &acts)
        .with_companions(if 关.with(|c| c.get()) {
            jpp::interp::CompanionMode::Off
        } else if 开.with(|c| c.get()) {
            jpp::interp::CompanionMode::Same
        } else {
            伴随()
        })
        .run(&program, &EntryArgs::default(), &mut ledger)
        .map_err(|e| e.render())?;
    let calls = *calls.borrow();
    let gens = *gens.borrow();
    let asked = asked.borrow().clone();
    Ok(跑出 {
        out,
        ledger,
        calls,
        gens,
        asked,
    })
}

/// 账本的判断条目：(账本键, 题哈希, parents, hop)
pub fn 判断条目(l: &Ledger) -> Vec<(String, String, Vec<String>, u32)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                key,
                jkey,
                parents,
                hop,
                ..
            } => Some((
                key.clone(),
                jkey.as_ref().map(|k| k.q.clone()).unwrap_or_default(),
                parents.clone(),
                *hop,
            )),
            _ => None,
        })
        .collect()
}

/// 账本判断条目的校准引用：(题哈希, calib_ref)
pub fn 判断引用(l: &Ledger) -> Vec<(String, jpp_ir::key::CalibRef)> {
    l.entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge {
                jkey, calib_ref, ..
            } => Some((
                jkey.as_ref().map(|k| k.q.clone()).unwrap_or_default(),
                calib_ref.as_deref().cloned()?,
            )),
            _ => None,
        })
        .collect()
}

pub fn transform_条目数(l: &Ledger) -> usize {
    l.entries
        .iter()
        .filter(|e| matches!(e, Entry::Effect { .. }))
        .count()
}

/// 常用的读出：每个值元素的 by、细化、结论与路径（「by:出口」）
pub const 读出: &str = "{value: map(r.value, fn(v) { {by: v.by, refined: if has(v, \"refined\") { v.refined } else { unit },
  conclusion: if has(v, \"conclusion\") { v.conclusion } else { unit },
  path: map(v.path, fn(p) { p.by + \":\" + exit_kind(p.exit) })} }),
  pending: map(r.pending, fn(p) { p.cause }), carried: r.pending, hops: r.detail.hops, per_hop: r.detail.per_hop,
  rejected: r.detail.rejected, unchosen: r.detail.unchosen, unasked: len(r.detail.unasked)}";

/// 伴随题开关与中性读数约定：只在 `tests/common` 定一处，这里引用（主控 2026-09-30）
#[path = "../common/mod.rs"]
mod 公共;
pub use 公共::{伴随, 伴随中性, 伴随参数};

/// 同 [`跑`]，固定关伴随题（数题数、条目数的测试用；主控 2026-09-30：第二、三类保持关）
pub fn 跑_关(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    gen_out: Vec<Json>,
) -> Result<跑出, String> {
    关.with(|c| c.set(true));
    let r = 跑(src, answer, gen_out);
    关.with(|c| c.set(false));
    r
}

thread_local! {
    static 关: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static 把关: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static 开: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// 同 [`跑_按提示_画像`]，强制开伴随题（不看 JPP_TEST_COMPANIONS；Z0497 复核：钉住伴随题开着的默认路径）
pub fn 跑_按提示_画像_伴随(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
    profile: jpp::effects::Profile,
) -> Result<跑出, String> {
    开.with(|c| c.set(true));
    let r = 跑_按提示_画像(src, answer, 生成, profile);
    开.with(|c| c.set(false));
    r
}

/// 同 [`跑_按提示_画像`]，固定关伴随题（Z0497：测补信息后的读数带，伴随题第一轮的「题不清 / 两可」路由会先停下补信息）
pub fn 跑_按提示_画像_关(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
    profile: jpp::effects::Profile,
) -> Result<跑出, String> {
    关.with(|c| c.set(true));
    let r = 跑_按提示_画像(src, answer, 生成, profile);
    关.with(|c| c.set(false));
    r
}

/// 同 [`跑_按提示_画像`]，开 `--guard`（Z0497 的测试用）
pub fn 跑_按提示_画像_把关(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
    profile: jpp::effects::Profile,
) -> Result<跑出, String> {
    把关.with(|c| c.set(true));
    let r = 跑_按提示_画像(src, answer, 生成, profile);
    把关.with(|c| c.set(false));
    r
}

/// 同 [`跑`]，开 `--guard`（Z0514 的测试用）
pub fn 跑_把关(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    gen_out: Vec<Json>,
) -> Result<跑出, String> {
    把关.with(|c| c.set(true));
    let r = 跑(src, answer, gen_out);
    把关.with(|c| c.set(false));
    r
}

/// 同 [`跑_按提示`]，固定关伴随题（N-T6 的测试用：默认链的类别不让伴随题先选）
pub fn 跑_按提示_关(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
) -> Result<跑出, String> {
    关.with(|c| c.set(true));
    let r = 跑_按提示(src, answer, 生成);
    关.with(|c| c.set(false));
    r
}

/// 同 [`跑_按提示`]，开 `--guard`（N-T6 的测试用）
pub fn 跑_按提示_把关(
    src: &str,
    answer: impl Fn(&str, &Question, &State) -> Answer,
    生成: impl Fn(&str) -> Vec<Json>,
) -> Result<跑出, String> {
    把关.with(|c| c.set(true));
    let r = 跑_按提示(src, answer, 生成);
    把关.with(|c| c.set(false));
    r
}

/// 前提派生的候选补上 `if_false: {falls_to: "unanswerable"}`（derive-16、17、18：每道前提要自报不成立时落到哪；老测试不测这一项，缺的补 none，写了的不动）
pub fn 补if_false(v: Vec<Json>) -> Vec<Json> {
    v.into_iter()
        .map(|mut x| {
            if let Some(o) = x.as_object_mut() {
                o.entry("if_false").or_insert(serde_json::json!({"falls_to": "unanswerable"}));
            }
            x
        })
        .collect()
}
