#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
被压掉的行 A-E 规则推分类脚本。用于 `research/19-what-the-rewrites-cut.zh-CN.md`。

跟 `busywork_classify.py` 的关系：复用它的 v2（块级归属 + 判断调用上下文邻近）
逐行分类逻辑，把 12 类杂活重新归并成 A/B/C 三桶，再对没有命中 12 类杂活正则的
"业务本身"残余行，另外跑一组"单纯语法差异"（D）的正则；没被 D 命中的残余行
算"保留"（假定映射到 J++ 判断语句本身的语义，比如 criteria/instructions 文本
来源、状态字段定义），不计入"被压掉"。

这是**规则推的**，不是人工核对的——只对 84 项目里未被人工逐块核对的那些项目跑
（人工核对的 15 个项目见 `research/data/2026-09-27-what-got-cut/manual-*.md`，规则
和人工结果的差异见同目录下的对照说明）。

分桶映射（对应研究 19 的 A-E 定义）：
  A 杂活   = 12类杂活正则里 2/3/4/5/6/7/12（门槛、拿不准、重试、缓存、费用、并发、校准）
  B 组合方式 = 1/10/11（调用循环与合批、结果回流、多判断合成）
  C 数据整形 = 8/9（题面拼装、材料裁剪）
  D 语法差异 = 残余行里命中"语法样板"正则的（class/import/类型注解/装饰器/日志/docstring等）
  E 功能缺失 = 本脚本不判定行号，只从 `research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv` 的
              equivalence_category 字段读"是否部分等价"，逐项目标记，不给出精确行数
              （精确行数只有人工读代码、对照 gap_titles 才能给，规则法做不到）
  保留     = 既不是杂活也不是语法样板的残余行，假定映射到判断语句本身的语义文本，
              未被压掉

已知局限（如实写，不假装是真值）：
  - D 的正则是语法层面的样板检测，不理解语义，会漏掉一些其实是纯语法差异、但写法
    特殊的行（比如把类型注解写在注释里的老式 Python），也可能把恰好含"class"字样
    的业务字符串误判（概率低，未见到）。
  - A/B/C 直接沿用 busywork_classify.py 的已知局限（花括号块尾朴素括号计数、
    JEV_CONTEXT 是关键词代理不是语义理解）。
  - "保留"不等于"这些行在 J++ 里一字不改"，只是假定它们对应判断语句的语义部分，
    是本脚本能做到的最好近似，不是精确的逐行对应关系（逐行对应只有人工核对能做，
    见 15 个人工项目）。

用法：
  python3 research/scripts/cut_lines_classify.py                    # 全量（含已人工核对的15个，标记 source=rule 供对照）
  python3 research/scripts/cut_lines_classify.py --exclude-manual   # 跳过15个人工核对过的项目，只出规则推的62个

公开发布说明：本脚本按私有工作区目录约定读取内部改写语料库与改写实测数据表，
未随本次公开——在公开仓库里直接运行会因找不到目录而报错。已经算好的数据表见
research/data/2026-09-27-what-got-cut/。发布本脚本是为了方法透明，不是为了在
公开仓库里独立可跑。
"""
import argparse
import csv
import re
import statistics
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
EVAL_DIR = SCRIPT_DIR.parent
JEV_ROOT = EVAL_DIR.parent.parent
REWRITE_DIR = JEV_ROOT / "internal_workspace" / "internal_rewrite_corpus" / "rewrites"  # private-workspace directory names generalized for publication; this path does not exist in the public repo
OUT_DIR = EVAL_DIR / "2026-09-27-压掉的六成"
DATA_CSV = EVAL_DIR / "2026-09-26-改写实测数据.csv"

sys.path.insert(0, str(SCRIPT_DIR))
import busywork_classify as bw  # noqa: E402

# 12 类杂活 -> A/B/C 桶
BUCKET_OF_CAT = {
    "2_门槛": "A", "3_拿不准的处理": "A", "4_重试与失败": "A", "5_缓存": "A",
    "6_费用与调用计数": "A", "7_并发限制": "A", "12_校准与标注": "A",
    "1_调用循环与合批": "B", "10_结果回流": "B", "11_多判断合成": "B",
    "8_题面拼装": "C", "9_材料裁剪": "C",
}

# 人工核对过的 15 个项目编号（见 research/data/2026-09-27-what-got-cut/manual-*.md）
MANUAL_IDS = {
    "B5-11b", "B1-07", "B5-06b", "Q-04", "B5-02",       # 低倍率组
    "B6-03", "Q-07", "B3-08", "B6-06", "B3-09",          # 中倍率组
    "B4-06", "B5-10", "B4-12", "B4-01", "B4-09",         # 高倍率组
}

EXCLUDE_NO_SOURCE = {"B1-11", "B4-02", "B6-11", "B7-01"}  # 无许可证，original/ 没有代码副本
EXCLUDE_OTHER = {"B2-05", "B3-07", "Q-05"}  # 记录缺失 / 不可比

D_PATTERNS_PY = re.compile(
    r"^\s*(class\s+\w+|import\s|from\s+\S+\s+import|@dataclass|@\w+(\(.*\))?\s*$|"
    r"def\s+__init__\s*\(|def\s+__repr__\s*\(|def\s+__post_init__\s*\(|"
    r'"""|\'\'\'|logging\.(getLogger|basicConfig)|logger\s*=\s*logging|'
    r"parser\s*=\s*argparse|parser\.add_argument|argparse\.ArgumentParser|"
    r"if\s+__name__\s*==\s*[\"']__main__[\"']|@staticmethod|@classmethod|@property|"
    r"@abstractmethod|from\s+abc\s+import|from\s+dataclasses\s+import|"
    r"from\s+typing\s+import|from\s+enum\s+import|class\s+\w+\(Enum\)|"
    r"^\s*\w+\s*:\s*(int|str|float|bool|bytes|Optional\[|List\[|Dict\[|Any|None)\b)",
    re.IGNORECASE,
)
D_PATTERNS_BRACE = re.compile(
    r"^\s*(import\s|export\s+(class|interface|type|default)|interface\s+\w+\s*\{?|"
    r"type\s+\w+\s*=|class\s+\w+|constructor\s*\(|@\w+\(|"
    r"^\s*\w+\s*:\s*(string|number|boolean|void|any|unknown)\b|"
    r"package\s+\w+|func\s+\(\w+\s+\*?\w+\)\s+\w+\(|"
    r"use\s+std::|impl\s+\w+|pub\s+struct|pub\s+fn|"
    r"console\.(log|error|warn|debug)\(|println!\()",
    re.IGNORECASE,
)

PYTHON_EXTS = bw.PYTHON_EXTS
BRACE_EXTS = bw.BRACE_EXTS


def classify_project_for_cut(project_dir: Path):
    toml_path = project_dir / "measure.toml"
    core_entries, _host_entries = bw.parse_measure_toml(toml_path)
    segments, missing = bw.load_segments(project_dir, core_entries, "core")

    bucket_lines = {"A": 0, "B": 0, "C": 0, "D": 0}
    kept_lines = 0
    total = 0

    for ext, seg_lines in segments:
        excl, _presence, seg_total, _detail = bw.classify_segment_v2(seg_lines, ext)
        total += seg_total
        for cat, n in excl.items():
            if cat == bw.RESIDUAL:
                continue
            bucket = BUCKET_OF_CAT.get(cat)
            if bucket:
                bucket_lines[bucket] += n
        # 残余行（既没命中12类杂活正则）：单独重跑一遍，标出哪些是 D 语法样板
        residual_count = excl[bw.RESIDUAL]
        if residual_count:
            d_count = _count_d_in_residual(seg_lines, ext, excl_detail=_detail)
            bucket_lines["D"] += d_count
            kept_lines += residual_count - d_count

    return {
        "total": total,
        "A": bucket_lines["A"], "B": bucket_lines["B"], "C": bucket_lines["C"],
        "D": bucket_lines["D"], "kept": kept_lines,
        "missing_files": missing,
    }


def _count_d_in_residual(seg_lines, ext, excl_detail):
    """对没被12类杂活正则命中的行（残余行），跑 D 语法样板正则。
    需要重新知道"哪些行是残余"——用 classify_segment_v2 相同的逻辑重算一次 claimed，
    这里简化：直接对整段重新过一遍 12 类正则 + 触发块展开，取"未被任何 12 类命中"的
    行集合，再对这个集合跑 D 正则。"""
    excl2, _presence2, _total2, detail2 = bw.classify_segment_v2(seg_lines, ext)
    claimed_lines = set()
    for ln, _text, chosen, _matched in detail2:
        if chosen is not None:
            claimed_lines.add(ln)
    d_re = D_PATTERNS_PY if ext in PYTHON_EXTS else D_PATTERNS_BRACE
    d_count = 0
    for i, raw in enumerate(seg_lines, start=1):
        stripped = raw.strip()
        if not stripped or bw.COMMENT_RE.match(stripped):
            continue
        if i in claimed_lines:
            continue
        if d_re.search(raw):
            d_count += 1
    return d_count


def load_equivalence(project_id_map):
    """从改写实测数据 CSV 读 equivalence_category，判断该项目是否"部分等价"（E 标记）。"""
    out = {}
    if not DATA_CSV.exists():
        return out
    with open(DATA_CSV, encoding="utf-8") as f:
        for row in csv.DictReader(f):
            pid = row.get("id", "")
            eq = row.get("equivalence_category", "")
            gaps = row.get("gap_titles", "")
            out[pid] = {"equivalence_category": eq, "gap_titles": gaps}
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exclude-manual", action="store_true",
                     help="跳过15个人工核对过的项目，只出规则推的62个（推荐：与人工结果分开报告，避免重复计入）")
    ap.add_argument("--ids", default="", help="逗号分隔的编号子集")
    args = ap.parse_args()

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    projects = sorted([d for d in REWRITE_DIR.iterdir() if d.is_dir()])

    equiv = load_equivalence(None)

    rows = []
    skipped = []
    for pd in projects:
        parts = pd.name.split("-", 2)
        pid = parts[0] + "-" + parts[1] if len(parts) >= 2 else pd.name
        if args.ids:
            wanted = set(args.ids.split(","))
            if pid not in wanted:
                continue
        if pid in EXCLUDE_NO_SOURCE or pid in EXCLUDE_OTHER:
            skipped.append((pid, "无源码副本或不可比"))
            continue
        if args.exclude_manual and pid in MANUAL_IDS:
            continue
        toml_path = pd / "measure.toml"
        if not toml_path.exists():
            skipped.append((pid, "无 measure.toml"))
            continue
        result = classify_project_for_cut(pd)
        if result["total"] == 0:
            skipped.append((pid, "核心行数为0或源文件缺失: " + ";".join(result["missing_files"])))
            continue
        eq_info = equiv.get(pid, {})
        eq_cat = eq_info.get("equivalence_category", "")
        is_partial = bool(eq_cat) and "全部" not in eq_cat
        row = {
            "id": pid, "dir": pd.name,
            "source": "rule",
            "total_core_lines": result["total"],
            "A_杂活": result["A"], "B_组合方式": result["B"], "C_数据整形": result["C"],
            "D_语法差异": result["D"], "kept_保留": result["kept"],
            "cut_total": result["A"] + result["B"] + result["C"] + result["D"],
            "equivalence_category": eq_cat,
            "has_gap_E_flag": 1 if is_partial else 0,
            "gap_titles": eq_info.get("gap_titles", ""),
            "missing_files": ";".join(result["missing_files"]),
        }
        rows.append(row)

    out_path = OUT_DIR / ("规则推-62项目.csv" if args.exclude_manual else "规则推-全量.csv")
    fieldnames = ["id", "dir", "source", "total_core_lines", "A_杂活", "B_组合方式", "C_数据整形",
                  "D_语法差异", "kept_保留", "cut_total", "equivalence_category", "has_gap_E_flag",
                  "gap_titles", "missing_files"]
    with open(out_path, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        for r in rows:
            w.writerow(r)

    # 汇总：每个项目算"被压掉行数中，A/B/C/D 各占多少比例"，再取中位数（不是行数合计，
    # 避免大项目的行数吞掉小项目的信号；合计版本另算一遍，两个都报，跟研究18同样的谨慎）
    print(f"共处理 {len(rows)} 个项目（跳过 {len(skipped)} 个：{skipped}）\n")

    per_project_pct = {"A": [], "B": [], "C": [], "D": []}
    total_cut_sum = 0
    total_bucket_sum = {"A": 0, "B": 0, "C": 0, "D": 0}
    for r in rows:
        cut = r["cut_total"]
        if cut <= 0:
            continue
        for k, label in (("A", "A_杂活"), ("B", "B_组合方式"), ("C", "C_数据整形"), ("D", "D_语法差异")):
            per_project_pct[k].append(r[label] / cut)
            total_bucket_sum[k] += r[label]
        total_cut_sum += cut

    print("逐项目占比中位数（每个项目先算 A/B/C/D 各占该项目被压掉行数的比例，再取中位数）：")
    for k in ("A", "B", "C", "D"):
        vals = per_project_pct[k]
        if vals:
            print(f"  {k}: 中位数 {statistics.median(vals)*100:.1f}%  均值 {statistics.mean(vals)*100:.1f}%")

    print("\n合计占比（全部项目被压掉的行数直接相加后算比例，大项目权重更高）：")
    for k in ("A", "B", "C", "D"):
        pct = total_bucket_sum[k] / total_cut_sum * 100 if total_cut_sum else 0
        print(f"  {k}: {total_bucket_sum[k]} / {total_cut_sum} = {pct:.1f}%")

    n_gap = sum(r["has_gap_E_flag"] for r in rows)
    print(f"\nE（部分等价/有已知缺口的项目数）：{n_gap}/{len(rows)}（{n_gap/len(rows)*100:.1f}%）——"
          f"这是项目数占比，不是行数占比，规则法给不出精确的 E 行数")

    total_core = sum(r["total_core_lines"] for r in rows)
    total_kept = sum(r["kept_保留"] for r in rows)
    print(f"\n核心总行数 {total_core}，保留（未被压掉）{total_kept}，"
          f"被压掉 {total_cut_sum}（{total_cut_sum/total_core*100:.1f}% of core）")

    print(f"\n输出：{out_path}")


if __name__ == "__main__":
    main()
