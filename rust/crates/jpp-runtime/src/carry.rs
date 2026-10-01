//! 预算随调用传递、只收紧（C-3，主控 Z0171）。
//!
//! 依据：裁定纸面阶段第五节「共 1」（`地基/规划/骨架比较/裁定-纸面阶段-2026-09-29.md`:87）——「整场一个上限，每一轮
//! 一个上限。事件触发的重跑、跨程序的重发，花费都记在整场上限里，深度也接着算，不重新起算。只收紧不放宽」；
//! 主控 Z0235 的读法：每轮上限 = min(本轮声明, 整场余额)；`骨架候选.md` 2.0.2 C-3 行；研究 15 的 gRPC 截止时间传播
//! （传到下游的是「还剩多少」，两者取小）。过程记录 `地基/过程记录/工程-C3-预算传递.md`。
//!
//! 传的是一份余额：调用、花费、时延预算（判断调用的累计秒数，不是墙钟截止时间，D 组留到原型后）、问人次数，
//! 以及深度已到第几层、上限多少（调用栈深度与跨运行跳数是同一个计数）。`unsure` 与 `absent` 不传。
//!
//! 余额的来路：宿主给整场上限（[`BudgetCarry::session`]），或宿主读回、自己造一份记录（[`BudgetCarry::from_record`]，
//! 公开，`CarryRecord` 字段也公开——宿主是整场上限的来源，造新余额与 `session` 同权）；从一份已有余额出发只能扣减
//! （[`BudgetCarry::after_spending`]、[`BudgetCarry::after_round`]），没有得到更宽余额的方法。「只收紧」由运行时入口保证：
//! 每一趟开跑时生效预算 = 逐项 min(程序声明, 余额)（[`TripCarry::for_trip`]），不是由类型保证。
//!
//! 交回余额是权威余额（主控第三轮读法）：一轮 = 一本账本上的首跑加它的续跑。账本头 `carry` 记这一段的进门余额，
//! `carry_from` 记段的起点，续跑时不改写；每一趟结束写一条 `Spent`（本趟新增的花费与得到回答的问人次数），交回余额 =
//! 进门余额减去本段全部 `Spent`、深度 +1（一轮只 +1 一次）。

use jpp_ir::ir::Budget;
use jpp_ir::ir::Span;
use jpp_ledger::{CarryRecord, Entry, Ledger};

use crate::R;

use crate::DEFAULT_DEPTH;

/// 上游交下来的整场余额。字段私有，只读。
#[derive(Clone, Debug, PartialEq)]
pub struct BudgetCarry(CarryRecord);

impl BudgetCarry {
    /// 宿主给整场上限（链的根）：`escalate` 未写为 0，`depth` 未写为 [`DEFAULT_DEPTH`]，深度从 0 起。
    pub fn session(整场: &Budget) -> BudgetCarry {
        Self::session_with_default(整场, DEFAULT_DEPTH)
    }

    /// 同 [`session`](Self::session)，`depth` 未写时链上限取宿主设的引擎默认（G4b 附录一，复核 Q5；`12` R11 第 5 条
    /// 「宿主直接触发且没声明的取引擎默认」）
    pub fn session_with_default(整场: &Budget, 引擎默认: u32) -> BudgetCarry {
        BudgetCarry(CarryRecord {
            calls: 整场.calls,
            cost: 整场.cost.max(0.0),
            latency_p95: 整场.latency_p95.map(|s| s.max(0.0)),
            escalate: 整场.escalate.unwrap_or(0),
            hop: 0,
            round: 0,
            depth_cap: 整场.depth.unwrap_or(引擎默认),
        })
    }

    /// 宿主读回自己存下的余额（命令行 `--carry-in` 的文件、账本头）。宿主是整场上限的来源，这与 `session` 同权。
    pub fn from_record(r: CarryRecord) -> BudgetCarry {
        BudgetCarry(r)
    }

    pub fn record(&self) -> &CarryRecord {
        &self.0
    }
    pub fn calls(&self) -> u64 {
        self.0.calls
    }
    pub fn cost(&self) -> f64 {
        self.0.cost
    }
    pub fn latency_p95(&self) -> Option<f64> {
        self.0.latency_p95
    }
    pub fn escalate(&self) -> u64 {
        self.0.escalate
    }
    pub fn depth_at(&self) -> u32 {
        self.0.hop
    }
    pub fn depth_cap(&self) -> u32 {
        self.0.depth_cap
    }

    /// 跳数用完：这一轮进门时跨程序触发链的层数不低于上限。G4 起这一趟只停发、照常求值（原 C-3「整轮不开跑」作废）。
    pub fn depth_exhausted(&self) -> bool {
        self.0.hop >= self.0.depth_cap
    }

    /// 本轮生效的上限 = 逐项 min(本轮声明, 整场余额)。`latency_p95` 未写视为无限；`escalate` 未写为 0；
    /// `depth` 未写为 [`DEFAULT_DEPTH`]；`unsure`、`absent` 原样（不传）。
    pub fn tighten(&self, 声明: &Budget) -> Budget {
        let r = &self.0;
        Budget {
            calls: 声明.calls.min(r.calls),
            cost: 声明.cost.min(r.cost),
            depth: Some(声明.depth.unwrap_or(DEFAULT_DEPTH).min(r.depth_cap)),
            escalate: Some(声明.escalate.unwrap_or(0).min(r.escalate)),
            unsure: 声明.unsure,
            absent: 声明.absent.clone(),
            latency_p95: match (声明.latency_p95, r.latency_p95) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
        }
    }

    /// 一轮跑完交给下一轮的余额：各项扣去本轮实际花费，只减不增、减到 0 为止；深度 +1（重跑与串联接着算）。
    pub fn after_spending(&self, calls: u64, usd: f64, secs: f64, asks: u64) -> BudgetCarry {
        let r = &self.0;
        BudgetCarry(CarryRecord {
            calls: r.calls.saturating_sub(calls),
            cost: (r.cost - usd.max(0.0)).max(0.0),
            latency_p95: r.latency_p95.map(|s| (s - secs.max(0.0)).max(0.0)),
            escalate: r.escalate.saturating_sub(asks),
            hop: r.hop.saturating_add(1),
            round: r.round,
            depth_cap: r.depth_cap,
        })
    }

    /// 带着这份余额（本段的进门余额）跑完一轮之后交给下一轮的余额——**从账本算**（主控第二、三轮答复）。首跑、续跑、
    /// 只凭账本重放、失败后都用这一个算法，所以重放给出的余额与首跑逐字节相同：
    /// - 账本头记的进门余额不是这一份：这一轮（这一段）没开跑，余额不变；
    /// - 本段（`carry_from` 起）有 `Spent`：扣去它们记的花费之和、深度 +1；只有 `started: false`（跳数用完没开跑）时不变；
    /// - 本段有带花费的条目却没被 `Spent` 记上（进程中途被杀、账本写不进），或开跑了却一条 `Spent` 都没有：算不出花了
    ///   多少就不再给——调用、花费、时延、问人归 0，深度 +1。绝不因失败放宽余额。
    pub fn after_round(&self, ledger: &Ledger) -> BudgetCarry {
        let Some(h) = ledger
            .header
            .as_ref()
            .filter(|h| h.carry.as_ref() == Some(&self.0))
        else {
            return self.clone();
        };
        let a = 段账(ledger, h.carry_from.unwrap_or(0) as usize, self);
        if a.未记 || a.spent_n == 0 {
            return self.归零();
        }
        if !a.started {
            return self.clone();
        }
        // 本段依次扣 `Spent`、遇 `CarryCap` 取小后的剩余，深度 +1（Z0384：交回不大于宿主给过的更小余额）
        // G4（裁定五十九第 7 条）：交出的是给下游用的余额——派生的下游 = 触发方 + 1；续跑计数归 0
        let mut r = a.剩.clone();
        r.0.hop = self.0.hop.saturating_add(1);
        r.0.round = 0;
        r
    }

    /// 交给下一轮的余额（宿主侧，含失败）：账本头记着这一段的进门余额时按账本算（[`after_round`](Self::after_round)），
    /// 否则（账本头没有余额：这一趟在开跑前就停了，例如计划期被拒、静态检查没过）原样交回宿主给的那份。
    pub fn handed_back(给的: Option<&BudgetCarry>, ledger: &Ledger) -> Option<BudgetCarry> {
        match ledger.header.as_ref().and_then(|h| h.carry.clone()) {
            Some(e) => Some(BudgetCarry(e).after_round(ledger)),
            None => 给的.cloned(),
        }
    }

    fn 归零(&self) -> BudgetCarry {
        BudgetCarry(CarryRecord {
            calls: 0,
            cost: 0.0,
            latency_p95: self.0.latency_p95.map(|_| 0.0),
            escalate: 0,
            hop: self.0.hop.saturating_add(1),
            round: 0,
            depth_cap: self.0.depth_cap,
        })
    }

    /// 逐项取小（C-3 R8）：调用、花费、时延、问人、深度上限取两者较小的；起算深度取自己的（这一轮的）
    fn 逐项取小(&self, 另: &BudgetCarry) -> BudgetCarry {
        let (a, b) = (&self.0, &另.0);
        BudgetCarry(CarryRecord {
            calls: a.calls.min(b.calls),
            cost: a.cost.min(b.cost),
            latency_p95: match (a.latency_p95, b.latency_p95) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            },
            escalate: a.escalate.min(b.escalate),
            hop: a.hop,
            round: a.round,
            depth_cap: a.depth_cap.min(b.depth_cap),
        })
    }

    /// 本段余额扣去已有 `Spent` 后剩多少，深度不动（续跑的这一趟接着这一轮的深度）；续跑计数 = 本段已跑的趟数（G4，
    /// 裁定五十九第 7 条：续跑不加 `hop`，另记 `round`）
    fn 剩余(&self, a: &段账) -> BudgetCarry {
        let mut r = a.剩.clone();
        r.0.hop = self.0.hop;
        r.0.round = self.0.round.saturating_add(a.spent_n as u32);
        r
    }
}

/// 本段账本的花费汇总（C-3）
struct 段账 {
    calls: u64,
    usd: f64,
    secs: f64,
    asks: u64,
    /// 本段 `Spent` 条数
    spent_n: usize,
    /// 有 `started: true` 的 `Spent`
    started: bool,
    /// 最后一条 `Spent` 之后（或一条都没有时整段）有带花费的条目：有一趟被杀，花费没被记上
    未记: bool,
    /// 本段进门余额依次扣 `Spent`、遇 `CarryCap` 取小之后的剩余（深度同进门余额）
    剩: BudgetCarry,
}

/// 带花费的条目：判断、效应（复用的不算）、问人、有尝试的缺席、意向
fn 带花费(e: &Entry) -> bool {
    match e {
        Entry::Judge { reused_from, .. } | Entry::Effect { reused_from, .. } => {
            reused_from.is_none()
        }
        Entry::Ask { .. } | Entry::Intent { .. } => true,
        Entry::Absent { attempts, .. } => *attempts > 0,
        _ => false,
    }
}

fn 段账(ledger: &Ledger, from: usize, 进门: &BudgetCarry) -> 段账 {
    let mut a = 段账 {
        calls: 0,
        usd: 0.0,
        secs: 0.0,
        asks: 0,
        spent_n: 0,
        started: false,
        未记: false,
        剩: 进门.clone(),
    };
    for e in ledger.entries.iter().skip(from) {
        match e {
            Entry::Spent {
                calls,
                usd,
                secs,
                asks,
                started,
            } => {
                a.spent_n += 1;
                a.未记 = false;
                if *started {
                    a.started = true;
                    a.calls += calls;
                    a.usd += usd;
                    a.secs += secs;
                    a.asks += asks;
                    let d = a.剩.0.hop;
                    a.剩 = a.剩.after_spending(*calls, *usd, *secs, *asks);
                    a.剩.0.hop = d;
                }
            }
            Entry::CarryCap {
                calls,
                usd,
                secs,
                asks,
                depth_cap,
            } => {
                let 顶 = BudgetCarry(CarryRecord {
                    calls: *calls,
                    cost: *usd,
                    latency_p95: *secs,
                    escalate: *asks,
                    hop: a.剩.0.hop,
                    round: a.剩.0.round,
                    depth_cap: *depth_cap,
                });
                a.剩 = a.剩.逐项取小(&顶);
            }
            e if 带花费(e) => a.未记 = true,
            _ => {}
        }
    }
    a
}

/// 账本里得到回答的问人次数（与程序 `budget.escalate` 同一口径）
fn 已答(entries: &[Entry]) -> u64 {
    entries
        .iter()
        .filter(|e| {
            matches!(
                e,
                Entry::Ask {
                    answer: Some(_),
                    ..
                }
            )
        })
        .count() as u64
}

/// 这一趟怎么带余额跑（C-3）：由宿主给的余额与开跑前的账本算出，运行时、规划器、命令行三处共用这一个函数。
#[derive(Clone, Debug)]
pub struct TripCarry {
    /// 本趟生效预算用的余额：`tighten` 它得到 min(程序声明, 余额)。续跑时是本段剩余（深度取这一轮的，不再 +1）；
    /// 问人一项已加上账本里已答的次数（运行时拿「账本已答 + 本趟新问」来比，所以不重复计）
    pub effective: BudgetCarry,
    /// 本段的进门余额（写进账本头 `carry`）
    pub segment: BudgetCarry,
    /// 本段起点（写进账本头 `carry_from`）
    pub from: u64,
    /// 宿主给的余额与账本记录的交回余额不同时的告警（`W-carry`），或显式重新授权开新段时的告警（`W-carry-reauth`）
    pub warning: Option<String>,
    /// 前一趟被杀、花费没被记上：开跑前补写的结清条目（把本段剩余全部记为已花）
    pub settle: Option<Entry>,
    /// 宿主给的余额（没开重新授权）在某一项上小于账本剩余：开跑前写的只收紧记录（Z0384）
    pub cap: Option<Entry>,
}

impl TripCarry {
    /// `给的` 是宿主给的余额；`ledger` 是开跑前的账本；`audit` 为真是只凭账本重放（余额取账本头，不看宿主给的）；
    /// `reauthorize` 为真是宿主显式重新授权（主控 R1：重新授权只靠显式开关，不靠「对不上账本」推断）。
    /// 返回 `None` = 这一趟不带余额。
    ///
    /// 账本头记着本段余额时（续跑同一轮）：
    /// - 没开重新授权：给的等于账本算出的交回余额 → 正常续跑；不等（更早、更大、更小、别的文件）→ 一律按旧文件处理，
    ///   出 `W-carry`，按账本里这一轮的剩余收紧；前一趟被杀、花费没被记上时补一条结清；
    /// - 开了重新授权：从这一趟起开新段，进门余额 = 给的（深度接着这一轮的），出 `W-carry-reauth` 写明上一段花了多少、
    ///   新段从哪起。
    ///
    /// 账本头没有余额（首跑、账本原来不带余额）时开关不起作用，给的就是本段的进门余额。
    pub fn for_trip(
        给的: Option<&BudgetCarry>,
        ledger: &Ledger,
        audit: bool,
        reauthorize: bool,
    ) -> Option<TripCarry> {
        Self::for_trip_declared(给的, ledger, audit, reauthorize, None)
    }

    /// 同 [`for_trip`](Self::for_trip)，另带程序声明的 `budget.depth`（G4b，`12` R11 第 5 条：下游上限 =
    /// min(自己声明的, 触发方的)）：声明小于本段剩余的深度上限时，本趟生效余额的 `depth_cap` 取小，并出一条只收紧的
    /// `CarryCap`（其余四项取本段剩余原值），交回余额因此也从账本算出取小后的上限。审计重放不出（账本里已有）。
    pub fn for_trip_declared(
        给的: Option<&BudgetCarry>,
        ledger: &Ledger,
        audit: bool,
        reauthorize: bool,
        声明深度: Option<u32>,
    ) -> Option<TripCarry> {
        let mut t = Self::for_trip_inner(给的, ledger, audit, reauthorize)?;
        let Some(d) = 声明深度 else {
            return Some(t);
        };
        if t.effective.0.depth_cap <= d {
            return Some(t);
        }
        t.effective.0.depth_cap = d;
        if audit {
            // 首跑写过的那条 `CarryCap` 在账本里；审计重放从账本头的进门余额起算，这里只把生效上限取小
            return Some(t);
        }
        t.cap = Some(match t.cap.take() {
            Some(Entry::CarryCap {
                calls,
                usd,
                secs,
                asks,
                depth_cap,
            }) => Entry::CarryCap {
                calls,
                usd,
                secs,
                asks,
                depth_cap: depth_cap.min(d),
            },
            _ => {
                // 本段剩余：新段是进门余额本身，续跑是账本算出的剩余（问人一项不含本趟加上的已答数）
                let r = match ledger.header.as_ref().and_then(|h| h.carry.clone()) {
                    Some(c) if !reauthorize => {
                        let e = BudgetCarry(c);
                        let a = 段账(ledger, t.from as usize, &e);
                        e.剩余(&a)
                    }
                    _ => t.segment.clone(),
                };
                Entry::CarryCap {
                    calls: r.0.calls,
                    usd: r.0.cost,
                    secs: r.0.latency_p95,
                    asks: r.0.escalate,
                    depth_cap: d,
                }
            }
        });
        Some(t)
    }

    fn for_trip_inner(
        给的: Option<&BudgetCarry>,
        ledger: &Ledger,
        audit: bool,
        reauthorize: bool,
    ) -> Option<TripCarry> {
        let 头 = ledger.header.as_ref().and_then(|h| {
            h.carry
                .clone()
                .map(|c| (BudgetCarry(c), h.carry_from.unwrap_or(0)))
        });
        if audit {
            let (e, from) = 头?;
            let 前 = 已答(&ledger.entries[..(from as usize).min(ledger.entries.len())]);
            let mut eff = e.clone();
            eff.0.escalate = e.0.escalate.saturating_add(前);
            return Some(TripCarry {
                effective: eff,
                segment: e,
                from,
                warning: None,
                settle: None,
                cap: None,
            });
        }
        let x = 给的?;
        let 现有 = 已答(&ledger.entries);
        let 新段 = |seg: BudgetCarry| {
            let mut eff = seg.clone();
            eff.0.escalate = seg.0.escalate.saturating_add(现有);
            TripCarry {
                effective: eff,
                segment: seg,
                from: ledger.entries.len() as u64,
                warning: None,
                settle: None,
                cap: None,
            }
        };
        let Some((e, from)) = 头 else {
            return Some(新段(x.clone()));
        };
        let a = 段账(ledger, from as usize, &e);
        let 交回 = e.after_round(ledger);
        if reauthorize {
            // 宿主显式重新授权：从这一趟起开新的一段，深度接着这一轮的
            let mut seg = x.clone();
            seg.0.hop = e.0.hop;
            let mut t = 新段(seg);
            // G4：新段仍是这一轮的续跑，续跑计数接着算（只进本趟生效余额，不改账本头记的进门余额）
            t.effective.0.round = e.0.round.saturating_add(a.spent_n as u32);
            t.warning = Some(format!(
                "W-carry-reauth: 显式重新授权。上一段（账本第 {from} 条起）进门余额 calls {}、cost {}、escalate {}，已记花费 calls {}、cost {}、问人 {}{}，账本算出的交回余额 calls {}、cost {}；新段从账本第 {} 条起，进门余额 calls {}、cost {}、escalate {}，深度接着这一轮的第 {} 层。以后续跑传新段交回的余额",
                e.0.calls,
                e.0.cost,
                e.0.escalate,
                a.calls,
                a.usd,
                a.asks,
                if a.未记 {
                    "（另有一趟的花费没被记上）"
                } else {
                    ""
                },
                交回.0.calls,
                交回.0.cost,
                t.from,
                t.segment.0.calls,
                t.segment.0.cost,
                t.segment.0.escalate,
                t.segment.0.hop
            ));
            return Some(t);
        }
        // 没开重新授权：对不上账本交回余额的，一律按旧文件处理，以账本为准
        let 对不上 = *x != 交回;
        let 剩 = e.剩余(&a);
        let (剩, settle) = if a.未记 {
            let 结清 = Entry::Spent {
                calls: 剩.0.calls,
                usd: 剩.0.cost,
                secs: 剩.0.latency_p95.unwrap_or(0.0),
                asks: 剩.0.escalate,
                started: true,
            };
            let mut 零 = e.归零();
            零.0.hop = e.0.hop;
            零.0.round = e.0.round.saturating_add(a.spent_n as u32);
            (零, Some(结清))
        } else {
            (剩, None)
        };
        // R8：对不上时这一趟逐项取小（调用、花费、时延、问人、深度上限；起算深度仍取这一轮的，已用跳数不取宿主给的）。
        // Z0384：给的在某一项上更小时，写一条只收紧的 `CarryCap`，交回余额因此也不超过给的
        let mut eff = if 对不上 {
            剩.逐项取小(x)
        } else {
            剩.clone()
        };
        let 更小 = 对不上
            && (eff.0.calls < 剩.0.calls
                || eff.0.cost < 剩.0.cost
                || eff.0.escalate < 剩.0.escalate
                || eff.0.depth_cap < 剩.0.depth_cap
                || match (eff.0.latency_p95, 剩.0.latency_p95) {
                    (Some(a), Some(b)) => a < b,
                    (Some(_), None) => true,
                    _ => false,
                });
        let cap = 更小.then(|| Entry::CarryCap {
            calls: eff.0.calls,
            usd: eff.0.cost,
            secs: eff.0.latency_p95,
            asks: eff.0.escalate,
            depth_cap: eff.0.depth_cap,
        });
        let warning = 对不上.then(|| {
            format!(
                "W-carry: 续跑给的余额（calls {}，cost {}，escalate {}，hop {}）与账本记录的交回余额（calls {}，cost {}，escalate {}，hop {}）不同，以账本的交回余额为准，本趟按账本剩余与给的余额逐项取小跑：calls {}，cost {}，escalate {}{}。续跑请传上一趟交回的余额（--carry-out 的文件）；要追加额度，显式重新授权（--carry-reauthorize，Rust 宿主 Session::reauthorize_carry）",
                x.0.calls,
                x.0.cost,
                x.0.escalate,
                x.0.hop,
                交回.0.calls,
                交回.0.cost,
                交回.0.escalate,
                交回.0.hop,
                eff.0.calls,
                eff.0.cost,
                eff.0.escalate,
                if 更小 { "（给的更小，账本记下这份更小的余额，交回不超过它）" } else { "" }
            )
        });
        eff.0.escalate = eff.0.escalate.saturating_add(现有);
        Some(TripCarry {
            effective: eff,
            segment: e,
            from,
            warning,
            settle,
            cap,
        })
    }
}

impl TripCarry {
    /// 这一趟没带余额、账本头记着本段余额时要不要先补结清（C-3 Z0384 复核 B1）：本段有带花费的条目没被 `Spent` 记上
    /// （前一趟被杀）时，返回把本段剩余全部记为已花的结清条目；否则 `None`。与带余额的一趟同一条规则——不带余额的一趟
    /// 收尾会写自己的 `Spent`，若不先结清，那条 `Spent` 会把「未记」盖掉，被杀那趟的花费就漏了。
    pub fn settle_without_carry(ledger: &Ledger) -> Option<Entry> {
        let h = ledger.header.as_ref()?;
        let e = BudgetCarry(h.carry.clone()?);
        let a = 段账(ledger, h.carry_from.unwrap_or(0) as usize, &e);
        if !a.未记 {
            return None;
        }
        let 剩 = e.剩余(&a);
        Some(Entry::Spent {
            calls: 剩.0.calls,
            usd: 剩.0.cost,
            secs: 剩.0.latency_p95.unwrap_or(0.0),
            asks: 剩.0.escalate,
            started: true,
        })
    }
}

/// 计划期拒绝（`E-budget-plan`）的修法改写（C-3 G2）：报文以「修法：放宽 budget」结尾、且生效预算在调用数、花费或
/// 时延上严于程序声明时，改指上游余额——改源码里的 `budget` 没有用。其余返回 `None`（原样用）。
pub fn rewrite_plan_rejection(msg: &str, 声明: &Budget, 生效: &Budget) -> Option<String> {
    const 旧: &str = "修法：放宽 budget";
    let 更严 = 生效.calls < 声明.calls
        || 生效.cost < 声明.cost
        || match (生效.latency_p95, 声明.latency_p95) {
            (Some(a), Some(b)) => a < b,
            (Some(_), None) => true,
            _ => false,
        };
    if !更严 || !msg.ends_with(旧) {
        return None;
    }
    Some(format!(
        "{}修法：卡住的是上游余额——本轮生效预算按上游余额收紧到 calls {} / cost {}（程序声明 calls {} / cost {}），改源码里的 budget 没有用；要宿主给更大的整场余额（--carry-in），或减少链上前几轮的花费",
        &msg[..msg.len() - 旧.len()],
        生效.calls,
        生效.cost,
        声明.calls,
        声明.cost
    ))
}

impl<'a> crate::Interp<'a> {
    /// 宿主交进上游余额（C-3）：`None` 与不调相同。审计重放不看它，改取账本头里记的那份（B35）。
    /// 与 `set_gate` 同形（取 `&mut`），宿主的 `jpp::interp::Interp` 经解引用直接调，不必另包一层。
    pub fn set_carry(&mut self, c: Option<BudgetCarry>) {
        self.上游 = c;
    }

    /// 宿主显式重新授权（C-3 R1）：续跑同一轮时，宿主给的余额从这一趟起开新段，而不是按账本收紧。
    pub fn set_carry_reauthorize(&mut self, on: bool) {
        self.重新授权 = on;
    }

    fn 本趟(&self) -> Option<TripCarry> {
        TripCarry::for_trip_declared(
            self.上游.as_ref(),
            self.ledger.view(),
            self.audit.on,
            self.重新授权,
            self.声明深度.unwrap_or(self.budget.depth),
        )
    }

    /// 宿主设引擎默认深度上限（G4b，裁定六十四）：缺省 [`DEFAULT_DEPTH`]。审计重放不看它，取账本头的
    pub fn set_depth_cap_default(&mut self, d: u32) {
        self.深度默认 = d;
    }

    /// 本趟用的引擎默认深度上限（审计重放取账本头的，没有就是首跑没用到默认，取宿主设的）
    pub(crate) fn 深度默认(&self) -> u32 {
        self.深度默认
    }

    /// J-06 的递归上限（G4b）：程序声明的 `budget.depth`，没声明取引擎默认；不被余额收紧
    pub(crate) fn 递归上限(&self) -> u32 {
        self.声明深度
            .unwrap_or(self.budget.depth)
            .unwrap_or(self.深度默认)
    }

    /// 本趟生效余额是否已到深度上限（G4 附录二，复核 C1）：到限的一趟只停发、照常求值，一道题都不发，
    /// 宿主算计划时据此不按必经下界拒绝（`E-budget-plan` 的前提「运行一定会超预算」不成立）。
    pub fn depth_stop_ahead(&self) -> bool {
        self.本趟().is_some_and(|t| t.effective.depth_exhausted())
    }

    /// 本趟生效的预算 = 逐项 min(程序声明, 本趟余额)；没有余额时就是程序声明。宿主算计划（可靠下界，`E-budget-plan`）
    /// 前取它（主控第二轮答复第 2 条：上游余额已不够必经下界的，计划期就拒）。
    pub fn effective_budget(&self, 声明: &Budget) -> Budget {
        match self.本趟() {
            Some(t) => t.effective.tighten(声明),
            None => 声明.clone(),
        }
    }

    /// `run` 入口、写账本头之前调（C-3）：定这一趟怎么带余额跑；有余额时把 `budget` 收成 min(本轮声明, 本趟余额)、
    /// 起算深度设为这一轮的深度，告警进 trace；结清条目留到头写完再记（[`记结清`](Self::记结清)）。
    pub(crate) fn 进门收紧(&mut self) -> Option<TripCarry> {
        // G4b：程序声明的深度在收紧之前记下（递归上限只取它）；审计重放的引擎默认取账本头的
        self.声明深度 = Some(self.budget.depth);
        // G4b 附录一（复核 C1）：审计重放不读宿主配置——账本头有就取它，缺就取 G4b 之前的历史默认 256
        if self.audit.on {
            self.深度默认 = self
                .ledger
                .view()
                .header
                .as_ref()
                .and_then(|h| h.depth_cap_default)
                .unwrap_or(DEFAULT_DEPTH);
        }
        self.本趟起答 = 已答(&self.ledger.view().entries);
        // G4 一·5：没带余额时的续跑计数 = 本趟开始前账本里已有的不同段数（续跑一律开新段）
        self.本趟轮 = self.ledger.view().span_count() as u32;
        let Some(t) = self.本趟() else {
            self.缺余额告警();
            self.无余额结清 = if self.audit.on {
                None
            } else {
                TripCarry::settle_without_carry(self.ledger.view())
            };
            return None;
        };
        let 声明深度 = self.budget.depth.unwrap_or(DEFAULT_DEPTH);
        self.budget = t.effective.tighten(&self.budget);
        // G4（裁定五十九第 7 条，主控定第 2 条）：J-06 的调用栈每趟从 0 起算，只管递归；`hop` 只和 `depth_cap` 比
        self.depth = 0;
        self.深度上游 = Some((声明深度, t.effective.depth_cap(), t.effective.depth_at()));
        self.本趟轮 = t.effective.record().round;
        // G4：跨程序触发链到限，这一趟只停发、照常求值（取代 C-3 的「整轮不开跑」）
        self.深度停 = t.effective.depth_exhausted();
        if let Some(w) = &t.warning {
            self.trace.warn(w.clone());
        }
        Some(t)
    }

    /// 续跑时账本头记着这一轮的余额、宿主却没给（C-3 R4、D1、Z0384 R11）：这一趟不受整场上限约束，出一条告警；账本头照旧
    /// 带着余额（`outcome.rs` 组头处照抄），这一趟收尾照样写 `Spent`，下一趟带余额时从余额里扣
    fn 缺余额告警(&mut self) {
        if self.audit.on {
            return;
        }
        let Some(e) = self
            .ledger
            .view()
            .header
            .as_ref()
            .and_then(|h| h.carry.clone())
        else {
            return;
        };
        let 交回 = BudgetCarry(e).after_round(self.ledger.view());
        self.trace.warn(format!(
            "W-carry-missing: 账本记着这一轮的整场余额（交回余额 calls {}、cost {}、escalate {}、hop {}），这一趟没给余额，不受整场上限约束。账本头照旧带着这份余额；这一趟的花费照记进账本，下一趟带余额续跑时从余额里扣。要接着这条链，续跑时传上一趟交回的余额（--carry-in）",
            交回.0.calls, 交回.0.cost, 交回.0.escalate, 交回.0.hop
        ));
    }

    /// 账本头写完后补记结清条目（前一趟被杀、花费没被记上时）
    pub(crate) fn 记结清(&mut self, t: &Option<TripCarry>) -> R<()> {
        // Z0384 B1：不带余额的一趟在前一趟被杀时补的结清
        if let Some(e) = self.无余额结清.take() {
            self.即刻记账(e, Span::default())?;
        }
        if let Some(e) = t.as_ref().and_then(|t| t.settle.clone()) {
            self.即刻记账(e, Span::default())?;
        }
        // Z0384：给的余额更小时的只收紧记录，写在结清之后（本段依次算剩余时先扣结清、再取小）
        if let Some(e) = t.as_ref().and_then(|t| t.cap.clone()) {
            self.即刻记账(e, Span::default())?;
        }
        Ok(())
    }

    /// 本趟实际花费进账本（C-3）：带余额且不是审计重放时写一条 `Spent`，即刻落盘。数的是本趟计数器：新发的调用
    /// （含 `gen`、`do`、`ask` 与重试）、花费、判断调用的累计时延；问人数的是本趟新增的**得到回答的**次数
    /// （与程序 `budget.escalate` 同一口径，已问未答不算）。
    pub(crate) fn 记花费(&mut self, started: bool) -> R<()> {
        if self.audit.on {
            return Ok(());
        }
        let e = Entry::Spent {
            calls: self.cost.calls,
            usd: self.cost.usd,
            secs: self.latency_spent,
            asks: 已答(&self.ledger.view().entries).saturating_sub(self.本趟起答),
            started,
        };
        self.即刻记账(e, Span::default())
    }

    /// J-06 深度报文（C-3，主控答复第 3 条 (乙)）：分清卡住的是上游收紧的上限还是程序自己声明的 `budget.depth`，
    /// 起算深度来自上游时一并说出；不带上游余额时与改前逐字相同。
    pub(crate) fn 深度超限报文(&self, max_depth: u32) -> String {
        // G4：调用栈每趟从 0 起算；G4b：递归上限只取程序声明（或引擎默认），不被余额收紧，不再有「上游收紧」一支
        format!("调用深度超过 {max_depth}（递归无界）。修法：用 loop(bound, …) 或提高 budget.depth")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 声明(calls: u64, cost: f64) -> Budget {
        Budget {
            calls,
            cost,
            depth: None,
            escalate: None,
            unsure: None,
            absent: None,
            latency_p95: None,
        }
    }

    #[test]
    fn 逐项取小() {
        let c = BudgetCarry::session(&Budget {
            depth: Some(5),
            escalate: Some(2),
            latency_p95: Some(3.0),
            ..声明(10, 1.0)
        });
        let b = c.tighten(&声明(100, 0.5));
        assert_eq!((b.calls, b.cost), (10, 0.5));
        assert_eq!(
            (b.depth, b.escalate, b.latency_p95),
            (Some(5), Some(0), Some(3.0))
        );
        let b = c.tighten(&Budget {
            depth: Some(2),
            escalate: Some(9),
            latency_p95: Some(1.0),
            ..声明(4, 5.0)
        });
        assert_eq!((b.calls, b.cost), (4, 1.0));
        assert_eq!(
            (b.depth, b.escalate, b.latency_p95),
            (Some(2), Some(2), Some(1.0))
        );
    }

    #[test]
    fn 扣减到零为止深度加一() {
        let c = BudgetCarry::session(&声明(3, 1.0)).after_spending(5, 2.0, 1.0, 1);
        assert_eq!(
            (c.calls(), c.cost(), c.escalate(), c.depth_at()),
            (0, 0.0, 0, 1)
        );
        assert_eq!(c.latency_p95(), None);
    }
}
