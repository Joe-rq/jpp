"""模型档案测试组。

  .venv/bin/python -m foundation.profile.run --model jev-1.13.0 --budget 0.50 --out foundation/profile/profiles/jev-1.13.0.json
  .venv/bin/python -m foundation.profile.run --model jev-1.13.0 --dry-run          # 只打印每项调用数与预算估计
  .venv/bin/python -m foundation.profile.run --only window,delta --dry-run

每项 = 一个已有实验脚本的包装（不重写实验）。跑前打印预算估计；累计超 --budget 的项跳过并在档案里标「未测」。
真跑后由 build_from_raw 把 raw/ 结果汇总进档案；本文件只负责调度与预算。
"""
from __future__ import annotations
import argparse, json, subprocess, sys, time
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]          # 仓库根（地基/）
PROFILES = Path(__file__).resolve().parent / "profiles"
PRICE = 0.042 / 1e6                                  # $/input token（文档 D1；输出免费）


@dataclass
class Item:
    key: str            # 档案字段组
    label: str
    module: str         # 被包装的实验模块
    argv: list[str]
    calls: int          # 预估调用数（按原实验实跑数）
    tokens: int         # 预估输入 token 总量
    fills: list[str]    # 填档案的哪些字段
    note: str = ""

    @property
    def usd(self) -> float:
        return self.tokens * PRICE


# 预估数全部取自各实验的实跑记录（e*_results.json / raw/*/summary*.json / run_stats.json）。
ITEMS: list[Item] = [
    Item("window", "窗口常数：带主张填充剂量-反应（E5）+ 中性填充（E5-neutral），文字渲染",
         "foundation.experiments.e5_dilution", [], 132 + 36, 409_000 + 180_000,
         ["window.noul_claim_bearing", "window.noul_neutral"],
         "e5_neutral_filler_check 作为第二段自动接跑；表示 = text"),
    Item("window_repr", "窗口常数按表示分列：text_slots vs json_slots，带主张语境剂量 ~100/~1000/~1800（E-JSON + E-JSON-hi）",
         "foundation.experiments.e_audit4", ["json"], 224 + 112, 95_000 + 187_000,
         ["window.text_slots", "window.json_slots"],
         "--repr text|json 选臂（默认两臂都跑）；e_audit5 高剂量作为第二段接跑；JSON 上限（3k/5k/8k）尚未包装，档案标未测"),
    Item("delta", "重跑噪声 δ：同题同状态立即重跑 ×5（E1）",
         "foundation.experiments.e1_repeat", [], 100, 150_000,
         ["delta.*", "flip_rate.choice_argmax"]),
    Item("batch", "批不变性：单独 vs 同批 10/50/200 题（E2）",
         "foundation.experiments.e2_isolation", [], 120, 400_000,
         ["batch_invariance.noul", "batch_invariance.choice_label"]),
    Item("cost", "成本回归：题数 1/10/50/200 的 token 与时延（E4v2）",
         "foundation.experiments.e4_width_v2", [], 20, 61_000,
         ["cost.tokens_per_question", "cost.latency_by_n"],
         "固定开销 271 与系数 0.88 由 E8 的 1,088 次调用回归，随 E8 段重算"),
    Item("concurrency", "并发与时延：一元筛 c8/c32（E8 q1）",
         "foundation.experiments.proto_recursive", ["--q", "q1", "--conc", "32"], 290, 160_000,
         ["concurrency.*", "cost.intercept", "cost.state_coef", "cost.question_coef"],
         "另跑 --conc 8 一遍取吞吐下界；并发 ≥64 未包装（E10 遇 SSL EOF，需先定状态大小）"),
    Item("choice_k", "choice 的 K × 候选长度（E10 组 1）",
         "foundation.experiments.e10_window", ["g1", "--conc", "16"], 240, 290_000,
         ["k_limit.*", "window.choice", "position_bias.choice"]),
    Item("score_anchor", "score 锚数收敛 + 批不变性（E10 组 2/3）",
         "foundation.experiments.e10_window", ["g2", "--conc", "16"], 400 + 240, 260_000 + 214_000,
         ["anchors.*", "batch_invariance.score", "window.score"],
         "g3 作为第二段自动接跑"),
    Item("position_bias", "首位偏置：由 choice_k 的置换记录统计，不另发调用",
         "", [], 0, 0, ["position_bias.*"], "E9/E9b/E9c 的代码候选偏置在 raw/e9* 里，档案引用但不重跑"),
]

SECOND_STAGE = {  # 某些项分两个脚本
    "window": ("foundation.experiments.e5_neutral_filler_check", []),
    "window_repr": ("foundation.experiments.e_audit5", ["json"]),
    "score_anchor": ("foundation.experiments.e10_window", ["g3", "--conc", "16"]),
}


def run_item(model: str, it: Item, log) -> bool:
    if not it.module:
        return True
    stages = [(it.module, it.argv)] + ([SECOND_STAGE[it.key]] if it.key in SECOND_STAGE else [])
    for mod, argv in stages:
        cmd = [sys.executable, "-m", "foundation.profile._boot", model, mod, *argv]
        log(f"  $ {' '.join(cmd[1:])}")
        t = time.time()
        r = subprocess.run(cmd, cwd=ROOT)
        log(f"  → 退出 {r.returncode}，{time.time() - t:.0f} s")
        if r.returncode != 0:
            return False
    return True


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="jev-1.13.0")
    ap.add_argument("--budget", type=float, default=0.50, help="本次总预算（美元）")
    ap.add_argument("--out", default=None)
    ap.add_argument("--only", default=None, help="逗号分隔的项 key")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--repr", choices=["text", "json", "both"], default="both",
                    help="window_repr 项跑哪一臂（E-JSON 脚本同一次调用产出两臂；选单臂时只汇总该臂）")
    a = ap.parse_args()
    out = Path(a.out) if a.out else PROFILES / f"{a.model}.json"
    keys = a.only.split(",") if a.only else [i.key for i in ITEMS]
    items = [i for i in ITEMS if i.key in keys]

    print(f"模型 {a.model} · 预算 ${a.budget:.2f} · {'干跑' if a.dry_run else '实跑'}")
    print(f"{'项':<14}{'调用':>6}{'token':>10}{'预算$':>9}  填字段")
    total_c = total_t = 0
    plan = []
    for it in items:
        print(f"{it.key:<14}{it.calls:>6}{it.tokens:>10}{it.usd:>9.4f}  {', '.join(it.fills)}")
        if it.note:
            print(f"{'':<14}  注：{it.note}")
        total_c += it.calls; total_t += it.tokens
        plan.append(it)
    print(f"{'合计':<14}{total_c:>6}{total_t:>10}{total_t * PRICE:>9.4f}")
    if a.dry_run:
        return

    spent = 0.0
    status: dict[str, str] = {}
    for it in plan:
        if spent + it.usd > a.budget:
            print(f"跳过 {it.key}：累计 ${spent:.3f} + 预估 ${it.usd:.3f} > 预算 ${a.budget:.2f}")
            status[it.key] = "未测（超预算跳过）"
            continue
        print(f"跑 {it.key} …")
        ok = run_item(a.model, it, print)
        spent += it.usd
        status[it.key] = "已跑" if ok else "失败"
    from foundation.profile.build_from_raw import build
    prof = build(a.model)
    if a.repr != "both":
        drop = "json_slots" if a.repr == "text" else "text_slots"
        prof["window"][drop] = {"claim_bearing_ctx": {"value": "未测", "by": f"window_repr --repr {'json' if drop=='json_slots' else 'text'}"}}
    prof["run_status"] = status
    prof["run_budget_usd"] = a.budget
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(prof, ensure_ascii=False, indent=1))
    print(f"档案写到 {out}")


if __name__ == "__main__":
    main()
