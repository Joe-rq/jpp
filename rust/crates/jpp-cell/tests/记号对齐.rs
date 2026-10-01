//! 主控 Z0519：同一笔欠账在 `jpp-cell` 与 `jpp-ledger`（账本 v5 的 `DebtMark`）算出的记号相等。
//! 线上名：帧种类 `program`、`code`；过桥种类 `cut`、`fit`、`cut_score`（`claim` 只在单元图内部）。

use jpp_cell::debt::mark_token;
use jpp_cell::{BridgeKind, FrameKind, UnsureCause};
use jpp_ledger::skeleton::{DebtMark, FrameKind as LFrame, Via};

#[test]
fn 同一笔欠账_两边记号逐字节相等() {
    let frames = [
        (FrameKind::Program, LFrame::Program),
        (FrameKind::Cell, LFrame::Code),
    ];
    let vias = [
        (BridgeKind::Cut, Via::Cut),
        (BridgeKind::Fit, Via::Fit),
        (BridgeKind::CutScore, Via::CutScore),
    ];
    let mut n = 0;
    for (cf, lf) in frames {
        for (cb, lv) in vias {
            for cause in [UnsureCause::Tie, UnsureCause::Absent, UnsureCause::Band] {
                for nth in [1u32, 7] {
                    let key = format!("q{nth}");
                    let ours = mark_token(cf, "主人", cb, nth, &key, cause);
                    let theirs = DebtMark {
                        frame: lf,
                        owner: "主人".into(),
                        via: lv,
                        nth,
                        key: key.clone(),
                        cause: cause.name().into(),
                    }
                    .token();
                    assert_eq!(ours.as_str(), theirs, "{cf:?} {cb:?} {cause:?} {nth}");
                    n += 1;
                }
            }
        }
    }
    assert_eq!(n, 36);
}

#[test]
fn 帧种类与过桥种类的线上名() {
    assert_eq!(
        (FrameKind::Program.tag(), FrameKind::Cell.tag()),
        ("program", "code")
    );
    assert_eq!(
        (
            BridgeKind::Cut.tag(),
            BridgeKind::Fit.tag(),
            BridgeKind::CutScore.tag()
        ),
        (Via::Cut.name(), Via::Fit.name(), Via::CutScore.name())
    );
    assert_eq!(
        BridgeKind::Claim.tag(),
        "claim",
        "claim 线上没有，只在单元图内部"
    );
}
