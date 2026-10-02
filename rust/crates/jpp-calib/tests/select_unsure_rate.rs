//! Z0308：select 记录经 `calib-import`（`import_labels`）认证后，`unsure_rate` 要与 `cut` 同口径。
//!
//! 缺陷：导入折样本时写死 `perms: 0, mode_share: None`，K 元单侧上岗又拿空表当众数，
//! 于是 select 记录的率不论读数恒为 1.0（`经验unsure率` 把没测置换的样本记未决）。
//! 修后：标注行带两序读数的置换测量（`perms`、`mode_share`）进样本，单侧上岗按样本众数测率。
//!
//! 「重放实测」：对记录里每条带标注样本，用运行时同一个判序 `jpp_value::bridge::decide`（线取记录、
//! δ 取选中证书、众数取样本）走一遍，数出 `Unsure` 的占比——率必须等于它。
//!
//! 依据：主控板 Z0308；`地基/过程记录/工程-Z0308-select未决率.md` 第三节预注册 T1–T7。

use jpp_calib::CalibStore;
use jpp_calib::truth::{CertifyMethod, ImportOptions, LabelRow, SeqImport, import_labels};
use jpp_effects::views::CalibView;
use jpp_value::bridge::{CutInput, decide};
use jpp_value::value::{Answer, ExitKind};
use serde_json::{Value as Json, json};

fn 选项(certify: CertifyMethod) -> ImportOptions {
    ImportOptions {
        alpha: 0.1,
        conf_delta: 0.1,
        spot_check_min: 0.9,
        spot_check_conf: 0.95,
        abstain_warn: 0.1,
        batch: "z0308".into(),
        seed: 7,
        extent_min_disagree: 3,
        extent_same_dir: 0.8,
        extent_same_tier: 2.0 / 3.0,
        scope_quantiles: (0.01, 0.99),
        scope_margins: Default::default(),
        class_min_sources: 2,
        alpha_trial: None,
        sequential: match certify {
            CertifyMethod::Sequential => Some(SeqImport {
                batch: 10,
                weights: [0.8, 0.1, 0.05, 0.05],
                coverage_target: None,
                two_ends: false,
                frame: None,
            }),
            _ => None,
        },
        certify,
        step: None,
    }
}

fn 库() -> CalibStore {
    let mut s = CalibStore::new();
    s.profile.delta = jpp_effects::Field::known((0.04, 0.0781, 0.1141), "测试：发行画像的三列");
    s
}

/// 第 i 条读数：p 从 0.55 均匀到 1.0；p < 0.7 的一半 argmax 错，其余全对。
fn 读数(i: usize, n: usize) -> (f64, usize, usize) {
    let p = 0.55 + 0.45 * i as f64 / (n - 1) as f64;
    let truth = i % 3;
    let pick = if p < 0.7 && i.is_multiple_of(2) {
        (truth + 1) % 3
    } else {
        truth
    };
    (p, pick, truth)
}

/// `n` 条 select 行；`perm(i)` 给第 i 条的置换测量（`None` = 没测）。
fn 行(n: usize, perm: impl Fn(usize) -> Option<(usize, f64)>) -> Vec<LabelRow> {
    (0..n)
        .map(|i| {
            let (p, pick, truth) = 读数(i, n);
            let mut v = json!({"key": "k", "op": "select", "item": format!("m{i:03}"), "p": p,
                               "pick": pick, "label": truth, "source": "computed"});
            if let Some((k, s)) = perm(i) {
                v["perms"] = json!(k);
                v["mode_share"] = json!(s);
            }
            serde_json::from_value(v).unwrap()
        })
        .collect()
}

/// 用运行时的判序把记录里每条带标注样本走一遍，返回 `Unsure` 的占比（四位小数，与记录同一舍入）。
fn 重放实测(s: &CalibStore) -> f64 {
    let r = s.get("k");
    let c = r.选中的证书().expect("上岗记录有证书");
    let delta = CalibStore::cert_delta(&r, c);
    let lab: Vec<_> = r
        .samples
        .iter()
        .filter(|x| x.label.is_some() && x.p.is_some())
        .collect();
    let u = lab
        .iter()
        .filter(|x| {
            let p = x.p.unwrap();
            let (ex, _) = decide(&CutInput {
                fail: None,
                absent: None,
                line: Some((r.hi, r.lo)),
                cost_requested: false,
                alpha_requested: false,
                answer: Some(Answer::Choice(vec![p, (1.0 - p) / 2.0, (1.0 - p) / 2.0])),
                delta,
                mode_share: x.mode_share,
            });
            matches!(ex, ExitKind::Unsure(_))
        })
        .count();
    (u as f64 / lab.len() as f64 * 10000.0).round() / 10000.0
}

fn 导入(rows: &[LabelRow], certify: CertifyMethod) -> CalibStore {
    let mut s = 库();
    let rep = import_labels(&mut s, rows, &选项(certify)).unwrap();
    assert_eq!(rep[0].status, "上岗", "{certify:?}: {}", rep[0].truth.gate);
    s
}

const N: usize = 120;

/// T1、T2：两序众数全一致的 select 行，三条单侧认证路径（固定序、拆分、序贯）导入后率 = 重放实测，且不是 1。
#[test]
fn two_order_select_rate_equals_replay_on_all_one_sided_paths() {
    let rows = 行(N, |_| Some((2, 1.0)));
    for m in [
        CertifyMethod::FixedSequence,
        CertifyMethod::Split,
        CertifyMethod::Sequential,
    ] {
        let s = 导入(&rows, m);
        let r = s.get("k");
        assert!(
            r.samples
                .iter()
                .all(|x| x.perms == 2 && x.mode_share == Some(1.0)),
            "{m:?}: 置换测量随样本进记录"
        );
        let u = r.unsure_rate.expect("上岗即测率");
        assert_eq!(u, 重放实测(&s), "{m:?}");
        assert!(u < 1.0 && u > 0.0, "{m:?}: 率随读数变，不是恒 1：{u}");
        // T6：J-10 经视图取到的就是这个数（δ 绑定一致，不当未知）
        assert_eq!(CalibView::unsure_rate(&s, "k"), Some(u), "{m:?}");
    }
}

/// T3：部分行众数 < 1（运行期出 tie）、部分没测（untested）：率仍 = 重放实测，且比全一致时高。
#[test]
fn mixed_mode_share_and_untested_rows_follow_cut() {
    let 全一致 = 导入(&行(N, |_| Some((2, 1.0))), CertifyMethod::FixedSequence);
    let s = 导入(
        &行(N, |i| match i % 5 {
            0 => Some((2, 0.5)),
            1 => None,
            _ => Some((2, 1.0)),
        }),
        CertifyMethod::FixedSequence,
    );
    let u = s.get("k").unsure_rate.unwrap();
    assert_eq!(u, 重放实测(&s));
    assert!(
        u > 全一致.get("k").unsure_rate.unwrap(),
        "tie 与 untested 记未决"
    );
    // 线只看 (p, argmax 是否对)，不读众数：两份导入的线相同
    assert_eq!(s.get("k").hi, 全一致.get("k").hi);
}

/// T4：不带置换测量的 select 行（真的没测）：cut 出 untested，率仍是 1.0，行为不变。
#[test]
fn untested_select_rows_keep_rate_one() {
    let s = 导入(&行(N, |_| None), CertifyMethod::FixedSequence);
    let r = s.get("k");
    assert!(
        r.samples
            .iter()
            .all(|x| x.perms == 0 && x.mode_share.is_none())
    );
    assert_eq!(r.unsure_rate, Some(1.0));
    assert_eq!(重放实测(&s), 1.0);
}

fn 导入错(rows: Vec<Json>) -> String {
    let rows: Vec<LabelRow> = rows
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    import_labels(&mut 库(), &rows, &选项(CertifyMethod::FixedSequence)).unwrap_err()
}

/// T5：校验——成对、只在 select、取值范围、复核行不得带。
#[test]
fn permutation_fields_are_validated() {
    let sel = |extra: Json| {
        let mut v = json!({"key": "k", "op": "select", "item": "a", "p": 0.9, "pick": 0,
                           "label": 0, "source": "computed"});
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let e = 导入错(vec![sel(json!({"mode_share": 1.0}))]);
    assert!(e.contains("成对"), "{e}");
    let e = 导入错(vec![sel(json!({"perms": 2}))]);
    assert!(e.contains("成对"), "{e}");
    let e = 导入错(vec![sel(json!({"perms": 2, "mode_share": 1.5}))]);
    assert!(e.contains("0..=1"), "{e}");
    let e = 导入错(vec![sel(json!({"perms": 0, "mode_share": 1.0}))]);
    assert!(e.contains("perms 须 ≥ 1"), "{e}");
    let e = 导入错(vec![
        json!({"key": "k", "item": "a", "p": 0.9, "label": true,
                               "source": "computed", "perms": 2, "mode_share": 1.0}),
    ]);
    assert!(e.contains("test 行不收 perms"), "{e}");
    let e = 导入错(vec![
        json!({"key": "k", "op": "measure", "item": "a", "p": 0.9, "pick": 1,
                               "label": 1, "source": "computed", "perms": 2, "mode_share": 1.0}),
    ]);
    assert!(e.contains("measure 行不收 perms"), "{e}");
    // 复核行带置换测量：读数泄露给复核者
    let e = 导入错(vec![
        json!({"key": "k", "op": "select", "item": "a", "p": 0.9, "pick": 0, "label": 0,
               "source": "model:a", "perms": 2, "mode_share": 1.0}),
        json!({"key": "k", "op": "select", "item": "a", "label": 0, "source": "model:b",
               "spot_check": "s1", "perms": 2, "mode_share": 1.0}),
    ]);
    assert!(e.contains("E-review-leak") && e.contains("置换测量"), "{e}");
}

/// T5（续）：复核行进线（取序排在标注行前面）时，置换测量与读数一起从标注行回接，不丢。
#[test]
fn review_row_backfills_permutation_from_annotation_row() {
    let rows: Vec<LabelRow> = (0..N)
        .flat_map(|i| {
            let (p, pick, truth) = 读数(i, N);
            [
                json!({"key": "k", "op": "select", "item": format!("m{i:03}"), "p": p, "pick": pick,
                       "label": truth, "source": "model:a", "perms": 2, "mode_share": 1.0}),
                json!({"key": "k", "op": "select", "item": format!("m{i:03}"), "label": truth,
                       "source": "model:b", "spot_check": "s1"}),
            ]
        })
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    let mut s = 库();
    let rep = import_labels(&mut s, &rows, &选项(CertifyMethod::FixedSequence)).unwrap();
    let r = s.get("k");
    assert_eq!(r.samples.len(), N, "{}", rep[0].truth.gate);
    assert!(
        r.samples
            .iter()
            .all(|x| x.perms == 2 && x.mode_share == Some(1.0))
    );
}

/// T7：存盘再装载，装载重跑认证逐位复现，率不被改写。
#[test]
fn load_rerun_reproduces_rate_without_rewrite() {
    let s = 导入(&行(N, |_| Some((2, 1.0))), CertifyMethod::FixedSequence);
    let dir = std::env::temp_dir().join(format!("jpp-z0308-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    s.save(&dir).unwrap();
    let t = CalibStore::load(&dir).unwrap();
    assert!(
        t.load_report.iter().all(|l| !l.contains("unsure_rate")),
        "{:?}",
        t.load_report
    );
    assert_eq!(t.get("k").unsure_rate, s.get("k").unsure_rate);
    assert_eq!(t.get("k"), s.get("k"));
    let _ = std::fs::remove_dir_all(&dir);
}
