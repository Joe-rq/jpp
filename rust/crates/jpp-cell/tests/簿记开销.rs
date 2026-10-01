//! 预注册第三节 3.2：每单元的簿记成本（合成的「T1 形」图，不调解释器、求值体是常数时间）。
//!
//! N 个源单元（主体），每个主体一条四级代码单元链 `view/i ← s/i`、`short/i ← view/i`（= view/10）、`out/i ← short/i`、
//! `edge/i ← out/i`，一个聚合单元 `edges` 读全部 `edge/i`，一个程序读 `edges`。
//! 计数是确定的（`计数_精确`，常规测试）；墙钟与字节在 `#[ignore]` 的 `簿记开销_实测` 里打出来：
//! `scripts/cargoq test -p jpp-cell --release --test 簿记开销 -- --ignored --nocapture`。

use jpp_cell::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static A: Counting = Counting;

#[derive(Clone, Debug, PartialEq)]
struct N(i64);
impl CellValue for N {
    fn content_hash(&self) -> String {
        self.0.to_string()
    }
    fn unsure_exits(&self, _out: &mut Vec<(Option<DebtTok>, UnsureCause)>) {}
}

fn num(v: Option<N>) -> i64 {
    v.map(|x| x.0).unwrap_or(0)
}

struct Net {
    n: usize,
}
impl Program<N> for Net {
    fn attempt(&self, c: &mut Ctx<'_, N>) -> Result<N, N> {
        let n = self.n;
        let v = c
            .code("edges", move |c| {
                let mut sum = 0;
                for i in 0..n {
                    sum += chain(c, i)?.0;
                }
                Ok(N(sum))
            })
            .map_err(|_| N(-1))?;
        Ok(v)
    }
}

fn chain(c: &mut Ctx<'_, N>, i: usize) -> Result<N, Pend> {
    c.code(&format!("edge/{i}"), move |c| {
        let o = c.code(&format!("out/{i}"), move |c| {
            let s = c.code(&format!("short/{i}"), move |c| {
                let v = c.code(&format!("view/{i}"), move |c| {
                    Ok(N(num(c.source(&format!("s/{i}")))))
                })?;
                Ok(N(v.0 / 10))
            })?;
            Ok(s)
        })?;
        Ok(o)
    })
}

fn build(n: usize) -> CellGraph<N> {
    let mut g = CellGraph::new();
    g.add_program("net", Rc::new(Net { n }));
    g.host_event(
        (0..n)
            .map(|i| (format!("s/{i}"), N(i as i64 * 10)))
            .collect(),
    );
    g
}

fn none(_k: &str, _r: &[String]) -> FlushAnswer<N> {
    FlushAnswer::Absent
}

fn delta(a: &Stats, b: &Stats) -> Stats {
    Stats {
        marked: b.marked - a.marked,
        computed: b.computed - a.computed,
        recomputed_same: b.recomputed_same - a.recomputed_same,
        verified_clean: b.verified_clean - a.verified_clean,
        hits: b.hits - a.hits,
        attempts: b.attempts - a.attempts,
    }
}

#[test]
fn 计数_精确() {
    let n = 100;
    let mut g = build(n);
    g.quiesce(&mut none);
    assert_eq!(g.stats().computed, 4 * n as u64 + 1, "从零：4N + 1");
    assert_eq!(g.stats().attempts, 1);
    // 改一个主体、链上各级值都变
    let s0 = g.stats().clone();
    g.host_event(vec![("s/0".into(), N(1000))]);
    let s1 = g.stats().clone();
    g.quiesce(&mut none);
    let d = delta(&s1, g.stats());
    assert_eq!(delta(&s0, &s1).marked, 6, "标脏 5 个代码单元加 1 个程序");
    assert_eq!((d.computed, d.verified_clean), (5, 0));
    assert_eq!(g.published("net").len(), 2);
    // 改一个主体、short 截断（1000 → 1001，short 仍是 100）
    let s0 = g.stats().clone();
    g.host_event(vec![("s/0".into(), N(1001))]);
    let s1 = g.stats().clone();
    g.quiesce(&mut none);
    let d = delta(&s1, g.stats());
    assert_eq!(delta(&s0, &s1).marked, 6);
    assert_eq!((d.computed, d.recomputed_same, d.verified_clean), (2, 1, 3));
    assert_eq!(g.published("net").len(), 2, "程序不再有新版本");
}

fn med(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

#[test]
#[ignore = "墙钟实测，release 下手动跑"]
fn 簿记开销_实测() {
    for n in [10_000usize, 100_000] {
        let (mut fresh, mut mark, mut verify, mut bytes) = (vec![], vec![], vec![], vec![]);
        for _ in 0..3 {
            let before = LIVE.load(Ordering::Relaxed);
            let mut g = build(n);
            let t = Instant::now();
            g.quiesce(&mut none);
            let cells = g.stats().computed as f64;
            fresh.push(t.elapsed().as_secs_f64() * 1e6 / cells);
            bytes.push((LIVE.load(Ordering::Relaxed) - before) as f64 / g.code_cells() as f64);
            // 标脏：一次宿主事件改一个主体（5 个代码单元 + 1 个程序）；重复多次取每节点
            let rounds = 2000;
            let t = Instant::now();
            let s0 = g.stats().marked;
            for r in 0..rounds {
                g.host_event(vec![("s/0".into(), N(1000 + (r % 2) as i64))]);
                g.quiesce(&mut none);
            }
            let _ = s0;
            // 截断场景：每轮 view 重算、short 截断，out、edge、edges 核依赖后没变；edges 的核依赖读 N 条
            let per_round = t.elapsed().as_secs_f64() * 1e6 / rounds as f64;
            verify.push(per_round);
            // 只标脏的成本：不修复，只发宿主事件
            let mut g2 = build(n);
            g2.quiesce(&mut none);
            let t = Instant::now();
            let m0 = g2.stats().marked;
            for r in 0..rounds {
                g2.host_event(vec![(format!("s/{}", r % n), N(-(r as i64) - 1))]);
            }
            let marked = (g2.stats().marked - m0) as f64;
            mark.push(t.elapsed().as_secs_f64() * 1e6 / marked);
        }
        println!(
            "N={n}: 从零每代码单元 {:.3} µs；每代码单元常驻 {:.0} B；标脏每节点 {:.3} µs；截断一轮（view、short 重算，out、edge、edges 核后没变，edges 读 N 条）{:.1} µs = {:.4} µs × N",
            med(fresh),
            med(bytes),
            med(mark),
            med(verify.clone()),
            med(verify) / n as f64
        );
    }
}
