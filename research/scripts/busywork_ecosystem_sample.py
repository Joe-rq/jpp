#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
生态交叉验证抽样脚本。用于跟 84 个精选改写项目的「重复杂活」出现率做交叉验证。

背景：`busywork_classify.py` 只跑 `内部改写语料库（未随本次公开）/` 下 84 个精选真实 JEV 项目。
本脚本从未经筛选的生态快照 `实测/jev-ecosystem-2026-09-23/sources/`（4041 个仓库，
17GB 纯文本源码快照，只读不动）里，找出真的在代码里调用了 JEV/TypeSafe 接口的
仓库，抽 100 个，只统计 12 类杂活「出现与否」（不算行占比），跟 84 项目的出现率
对比。

方法（详见任务说明，不重复展开）：
  1. 相关性判定：用 ripgrep 在每个仓库的源码文件里找 SDK 调用证据（见
     RELEVANCE_PATTERN），而不是只看 candidates.json 的发现证据或 README 提及。
     搜索排除 node_modules/vendor/dist/build/.venv/venv/site-packages/.git/
     __pycache__/.next/_next/target/bin/obj 等第三方或构建产物目录。
  2. 排除：
     - 官方 org（typesafe-ai、TypeSafeAI、typesafe-sdk-csharp，大小写不敏感）。
     - 名字里同时含 "sdk" 和 "jev"/"typesafe" 的仓库（第三方 SDK 客户端库本身，
       不是业务逻辑；例如 typesafe-sdk-go、jev-php-sdk）。这条是名字级启发式，
       不逐仓库人工确认，可能有漏网或误伤，报告里会说明。
     - 仓库总文件数（排除第三方/构建目录后）少于 DEMO_MIN_FILES 的，按“教程/
       模板/demo-only”处理排除。这也是粗启发式，不是逐仓库人工确认。
     - 与 84 项目重名去重：读 `research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv` 的 repo 列
       （org/name），同名仓库跳过。
  3. random.seed(42) 固定种子，从剩余候选池按仓库名排序后 random.sample 抽 100
     个（不足 100 就抽全部）。
  4. 对每个抽中仓库，只读它匹配到调用证据的那些文件（不读全仓库），用与
     busywork_classify.py 完全一致的 12 组正则逐行匹配（跳过空行和整行注释），
     只记「出现与否」（0/1），不排他归类、不算占比。

已知局限（如实报告，不假装消除）：
  - 相关性判定基于关键词，可能漏掉用非常规写法调用 JEV 的仓库，也可能把「集成
    了 TypeSafe 作为众多 provider 之一的通用框架」（如某些仓库把 JEV 适配层当成
    一个 adapter crate/module）算进来——这类仓库的匹配文件本质上更像 SDK 适配层
    而非应用业务逻辑，可能抬高 4/5/6/7 类（重试、缓存、并发限制、调用计数）这些
    「基础设施味」更重的类别出现率。
  - demo-only 与「测试专用仓库」排除用的是文件数量与名字启发式，不是逐仓库人工
    读过，允许有误伤或漏网。
  - 用 `rg`（ripgrep）做全量扫描；如本机没有 rg，需要先安装或把 RG_BIN 改成
    等价的 `grep -r` 调用（脚本未提供自动降级，避免静默改变行为）。

用法：
  python3 research/scripts/busywork_ecosystem_sample.py

公开发布说明：本脚本读取的生态快照（4041 个公开仓库的本地镜像）与候选池排除规则
所需的本地文件未随本次公开——在公开仓库里直接运行会因找不到目录而报错。已经算好
的数据表见 research/data/2026-09-27-repetition/生态抽样100.csv。发布本脚本是为了
方法透明，不是为了在公开仓库里独立可跑。
"""
import csv
import random
import re
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
EVAL_DIR = SCRIPT_DIR.parent
JEV_ROOT = EVAL_DIR.parent.parent  # 内部改写语料库根目录（未随本次公开发布，脚本在公开仓库里运行会因找不到该目录而报错）
SOURCES_DIR = JEV_ROOT / "实测" / "jev-ecosystem-2026-09-23" / "sources"
DEDUP_CSV = EVAL_DIR / "2026-09-26-改写实测数据.csv"
OUT_DIR = EVAL_DIR / "2026-09-27-重复杂活"
OUT_CSV = OUT_DIR / "生态抽样100.csv"

RG_BIN = "rg"
SEED = 42
SAMPLE_SIZE = 100
DEMO_MIN_FILES = 5  # 仓库总文件数（排除第三方/构建目录）低于此视为 demo-only

# 顶层前缀不是 GitHub 仓库快照的目录（npm/pypi 包快照等），不参与抽样
NON_REPO_PREFIXES = ("npm__", "pypi", "hf__", "official-docs")

# 第三方/构建产物/虚拟环境目录，grep 与计数都要排除
EXCLUDE_DIR_GLOB = (
    "!{node_modules,vendor,dist,build,.venv,venv,site-packages,.git,"
    "__pycache__,.next,_next,target,bin,obj}/**"
)
SOURCE_EXT_GLOB = (
    "*.{py,js,jsx,ts,tsx,mjs,cjs,go,rs,rb,java,kt,swift,dart,cs,php,"
    "c,cc,cpp,h,hpp,scala,ex,exs,lua}"
)

# 判定「仓库源码里真的调用了 JEV/TypeSafe」的证据：官方 Python/JS SDK 的
# import 与类名、NoulAnswer 等 SDK 特有类型、以及 jev 作为模块名的 import/require。
# 不用裸的 "typesafe" 或 "judge(" 这类通用词——"typesafe" 在 TypeScript 社区里
# 是极常见的形容词（type-safe），会产生大量误报；judge( 在仲裁/体育/评审类项目
# 里也很常见，跟 JEV 无关。
RELEVANCE_PATTERN = re.compile(
    r"typesafe_sdk|@typesafe-ai/|TypeSafeClient|AsyncTypeSafeClient|NoulAnswer|"
    r"systemone\.judge|\bimport\s+jev\b|\bfrom\s+jev\b|jev_client|JevClient|"
    r"""from\s+["']jev["']|require\(["']jev["']\)""",
    re.IGNORECASE,
)

# 官方组织（不区分大小写比对）
OFFICIAL_ORGS = {"typesafe-ai", "typesafesdk-csharp", "typesafe-sdk-csharp"}

# 第三方 SDK 客户端库本身（名字里同时含 sdk 和 jev/typesafe）
SDK_NAME_RE = re.compile(r"sdk", re.IGNORECASE)
JEV_OR_TYPESAFE_RE = re.compile(r"jev|typesafe", re.IGNORECASE)

COMMENT_RE = re.compile(r"^\s*(#|//|/\*|\*|\"\"\"|''')")

# 84 项目基线用的 measure.toml [[core]] 范围明确排除测试文件（改写实测的「判断
# 核心」定义如此）。为了让本样本可比，分类时默认排除测试文件；测试文件仍会出现
# 在 matched_files 列里（那是 grep 的原始命中，不改），只是不参与 12 类正则计数。
# 若某仓库匹配到的文件全部是测试文件（没有非测试的调用点文件），退回用测试文件
# 分类并在 CSV 里标记 classified_test_fallback=1，避免这几个仓库的调用证据被
# 白白丢弃——这是已知的口径妥协，不是隐藏处理。
TEST_FILE_RE = re.compile(
    r"(^|/)(tests?|__tests__|spec)(/|$)|\.test\.|_test\.|\.spec\.", re.IGNORECASE
)

CATEGORIES = [
    "1_调用循环与合批",
    "2_门槛",
    "3_拿不准的处理",
    "4_重试与失败",
    "5_缓存",
    "6_费用与调用计数",
    "7_并发限制",
    "8_题面拼装",
    "9_材料裁剪",
    "10_结果回流",
    "11_多判断合成",
    "12_校准与标注",
]

# 照抄任务里给出的正则，不重新设计。注意：这份正则是
# `research/scripts/busywork_classify.py` 里 PATTERNS 的子集——原脚本的
# 5_缓存 还有 `\bMap\(\)\s*;.*cache|new Map\(\).*[Cc]ache` 分支，4_重试与失败
# 还有 `\bexponential\b.*backoff|except\s+\(` 分支，3_拿不准的处理还有
# `else\s*:\s*$.*#.*(unsure|uncertain)` 分支，本脚本都没有。这会让本样本在
# 3/4/5 类上比用原脚本正则跑出来的结果略偏低，是已知的口径差异，不是行为差异。
PATTERNS = {
    "5_缓存": r"lru_cache|functools\.cache\b|@cache\b|\bcache\s*=\s*\{|\bcache\s*=\s*dict\(|\bcache\[|\.get\(\s*key|hashlib\.(sha256|md5)|cache_key|cachedir|\bsqlite3\b|\bredis\b|memoize|localStorage|diskcache|shelve\.open|pickle\.(dump|load).*cache",
    "4_重试与失败": r"\btry\s*:|\btry\s*\{|except\s+\w*Exception|except\s*:|\.catch\(|\bretry\b|\bbackoff\b|max_retries|maxRetries|retry_count|n_retries|time\.sleep\(|setTimeout\(|asyncio\.sleep\(|\bfallback\b|fail_open|circuit.?breaker",
    "7_并发限制": r"Semaphore\(|asyncio\.Semaphore|threading\.Semaphore|rate_?limit|RateLimiter|p-limit|pLimit|\bthrottle\b|concurrency_limit|max_concurrent|\bmutex\b|\.acquire\(\)|token_?bucket",
    "1_调用循环与合批": r"asyncio\.gather|Promise\.all|Promise\.allSettled|ThreadPoolExecutor|ProcessPoolExecutor|multiprocessing\.Pool|concurrent\.futures|asyncio\.as_completed|\bbatch\b.*append|\bbuffer\b.*append|for\s+\w+\s+in\s+.*:\s*$|for\s*\(.*of\s+.*\)\s*\{|\.map\(async|chunk(ed|s)?\(|\bqueue\.append",
    "6_费用与调用计数": r"call_count|num_calls|n_calls|total_cost|token_count|token_usage|\bbudget\b|cost_so_far|spend(_so_far)?|usage\.total|calls_made|request_count|api_calls",
    "12_校准与标注": r"\bcalibrat\w*|threshold_search|grid_search|sklearn|roc_auc|\bAUC\b|labeled_data|annotation|ground_truth|gold_label|hand_labeled|precision_recall|f1_score",
    "10_结果回流": r"recent_signals|previous_(result|judgment|answer|confidence)|last_(result|judgment|answer|confidence)|history\.append|prior_(result|judgment)|\.append\(result\)|\.append\(judgment\)|context\s*\+=|memory\.append|conversation_history",
    "9_材料裁剪": r"\[:\s*\d+\s*\]|\btruncate\(|textwrap\.(shorten|wrap)|top_k\b|\[:k\]|\.slice\(0,|\bmax_len(gth)?\b|\bchunk_size\b|\.substring\(0,|split_into_chunks|\.head\(\d+\)",
    "3_拿不准的处理": r"\bunsure\b|\buncertain\b|\bambiguous\b|needs_review|needs_human|\bunknown\b.*status|status\s*=\s*[\"']pending[\"']|\bpending_review\b|manual_review",
    "2_门槛": r"confidence\s*[<>]=?\s*0\.\d+|probability\s*[<>]=?\s*0\.\d+|score\s*[<>]=?\s*0\.\d+|\bp\s*[<>]=?\s*0\.\d+|THRESHOLD|_THRESHOLD|min_confidence|MIN_CONFIDENCE|0\.\d+\s*[<>]=?\s*(confidence|probability|score)\b",
    "8_题面拼装": r"f[\"']{1,3}.*\{.*(question|prompt|candidate)|\.format\(.*question|question\s*=\s*[\"'].*\+|prompt\s*=\s*[\"'].*\+|`.*\$\{.*\}.*`|\bjson\.dumps\(candidates|join\(candidates\)|\+\s*[\"']\\n[\"']\s*\+|template\.(render|format)",
    "11_多判断合成": r"\bweighted_(sum|average)\b|\*\s*weight\b|\bvote\b|\bvoting\b|Counter\(.*answers|\bcascad\w*|\bensemble\b|majority|sum\(.*scores?\)|np\.average\(|statistics\.mean\(",
}
COMPILED_PATTERNS = {k: re.compile(v, re.IGNORECASE) for k, v in PATTERNS.items()}

# 已知 84 项目在「任一范围出现」口径下的出现率，用于最终对比打印（不重新验证）。
BASELINE_84 = {
    "1_调用循环与合批": 71.6,
    "2_门槛": 65.4,
    "3_拿不准的处理": 30.9,
    "4_重试与失败": 82.7,
    "5_缓存": 33.3,
    "6_费用与调用计数": 18.5,
    "7_并发限制": 12.3,
    "8_题面拼装": 54.3,
    "9_材料裁剪": 54.3,
    "10_结果回流": 3.7,
    "11_多判断合成": 3.7,
    "12_校准与标注": 42.0,
}


def run_rg_relevance():
    """跑一次 ripgrep，返回 {repo_dir_name: [matched_file_relpath, ...]}。"""
    cmd = [
        RG_BIN, "-l", "-i",
        "-g", EXCLUDE_DIR_GLOB,
        "-g", SOURCE_EXT_GLOB,
        "-e", RELEVANCE_PATTERN.pattern,
        ".",
    ]
    proc = subprocess.run(cmd, cwd=SOURCES_DIR, capture_output=True, text=True)
    # rg 找不到匹配时返回码是 1，不是错误
    if proc.returncode not in (0, 1):
        print("rg 执行失败：", proc.stderr, file=sys.stderr)
        sys.exit(1)
    by_repo = {}
    for line in proc.stdout.splitlines():
        rel = line[2:] if line.startswith("./") else line
        parts = rel.split("/", 1)
        if len(parts) != 2:
            continue
        repo_dir, filepath = parts
        if repo_dir.startswith(NON_REPO_PREFIXES):
            continue
        by_repo.setdefault(repo_dir, []).append(filepath)
    return by_repo


EXCLUDE_DIR_NAMES = {
    "node_modules", "vendor", "dist", "build", ".venv", "venv",
    "site-packages", ".git", "__pycache__", ".next", "_next",
    "target", "bin", "obj",
}


def count_repo_files(repo_dir: Path) -> int:
    """仓库总文件数，排除第三方/构建产物目录，用于 demo-only 过滤。
    用 os.walk 剪枝，不下钻进排除目录，避免大仓库（如误入候选池的框架仓库）
    拖慢整体运行。"""
    import os
    n = 0
    for root, dirs, files in os.walk(repo_dir):
        dirs[:] = [d for d in dirs if d not in EXCLUDE_DIR_NAMES]
        n += len(files)
    return n


def dirname_to_repo(dirname: str) -> str:
    """org__name -> org/name（仅用于展示与去重比对，用第一个 '__' 切分）。"""
    if "__" in dirname:
        org, name = dirname.split("__", 1)
        return f"{org}/{name}"
    return dirname


def load_dedup_set():
    repos = set()
    if not DEDUP_CSV.exists():
        print(f"警告：找不到去重名单 {DEDUP_CSV}，跳过去重", file=sys.stderr)
        return repos
    with open(DEDUP_CSV, newline="", encoding="utf-8") as f:
        r = csv.DictReader(f)
        for row in r:
            repo = (row.get("repo") or "").strip().lower()
            if repo:
                repos.add(repo)
    return repos


def is_official_org(dirname: str) -> bool:
    org = dirname.split("__", 1)[0].lower()
    return org in OFFICIAL_ORGS


def is_sdk_repo_name(dirname: str) -> bool:
    """名字里同时含 sdk 和 jev/typesafe 的仓库：第三方 SDK 客户端库本身。"""
    name_part = dirname.split("__", 1)[1] if "__" in dirname else dirname
    org_part = dirname.split("__", 1)[0] if "__" in dirname else ""
    full = f"{org_part} {name_part}"
    return bool(SDK_NAME_RE.search(full) and JEV_OR_TYPESAFE_RE.search(full))


def classify_repo(repo_dir: Path, matched_files):
    """对匹配到的文件逐行跑 12 类正则，只记出现与否（0/1）。
    默认排除测试文件（见 TEST_FILE_RE 上方注释）；全部是测试文件时退回用测试
    文件分类，返回值第二项标记是否发生了这种退回。"""
    non_test = [f for f in matched_files if not TEST_FILE_RE.search(f)]
    fallback = False
    files_to_use = non_test
    if not files_to_use:
        files_to_use = matched_files
        fallback = bool(matched_files)
    presence = {c: 0 for c in CATEGORIES}
    for rel in files_to_use:
        fp = repo_dir / rel
        try:
            text = fp.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue
        for raw in text.splitlines():
            stripped = raw.strip()
            if not stripped or COMMENT_RE.match(stripped):
                continue
            for cat, pat in COMPILED_PATTERNS.items():
                if presence[cat]:
                    continue
                if pat.search(raw):
                    presence[cat] = 1
    return presence, fallback


def main():
    if not SOURCES_DIR.exists():
        print(f"找不到生态快照目录：{SOURCES_DIR}", file=sys.stderr)
        sys.exit(1)

    print(f"[1/5] 用 ripgrep 在 {SOURCES_DIR} 里找调用证据...")
    by_repo = run_rg_relevance()
    print(f"      匹配到 {len(by_repo)} 个仓库目录（含调用证据文件）")

    dedup_set = load_dedup_set()
    print(f"[2/5] 去重名单（84 项目）：{len(dedup_set)} 个仓库")

    excluded_official = []
    excluded_sdk = []
    excluded_dedup = []
    excluded_demo = []
    candidates = []

    print("[3/5] 应用排除规则...")
    for dirname in sorted(by_repo.keys()):
        repo_name = dirname_to_repo(dirname)
        if is_official_org(dirname):
            excluded_official.append(repo_name)
            continue
        if is_sdk_repo_name(dirname):
            excluded_sdk.append(repo_name)
            continue
        if repo_name.lower() in dedup_set:
            excluded_dedup.append(repo_name)
            continue
        repo_dir = SOURCES_DIR / dirname
        total_files = count_repo_files(repo_dir)
        if total_files < DEMO_MIN_FILES:
            excluded_demo.append(f"{repo_name} ({total_files} 文件)")
            continue
        candidates.append(dirname)

    print(f"      候选池：{len(candidates)} 个仓库")
    print(f"      排除：官方 org {len(excluded_official)}、第三方 SDK 客户端库 {len(excluded_sdk)}、"
          f"与 84 项目重名 {len(excluded_dedup)}、demo-only（文件数<{DEMO_MIN_FILES}）{len(excluded_demo)}")

    print(f"[4/5] random.seed({SEED}) 抽样...")
    random.seed(SEED)
    candidates_sorted = sorted(candidates)  # 固定顺序，保证可复现
    if len(candidates_sorted) <= SAMPLE_SIZE:
        sampled = candidates_sorted
        print(f"      候选池不足 {SAMPLE_SIZE}，抽取全部 {len(sampled)} 个")
    else:
        sampled = random.sample(candidates_sorted, SAMPLE_SIZE)
        sampled.sort()
        print(f"      抽取 {len(sampled)} 个")

    print("[5/5] 逐仓库分类统计...")
    rows = []
    fallback_repos = []
    file_counts = []
    test_file_total = 0
    all_file_total = 0
    for dirname in sampled:
        repo_name = dirname_to_repo(dirname)
        repo_dir = SOURCES_DIR / dirname
        matched_files = sorted(by_repo[dirname])
        file_counts.append(len(matched_files))
        all_file_total += len(matched_files)
        test_file_total += sum(1 for f in matched_files if TEST_FILE_RE.search(f))
        presence, fallback = classify_repo(repo_dir, matched_files)
        if fallback:
            fallback_repos.append(repo_name)
        row = {
            "repo": repo_name,
            "matched_files": ";".join(matched_files),
            "matched_files_count": len(matched_files),
            "classified_test_fallback": 1 if fallback else 0,
        }
        for cat in CATEGORIES:
            row[f"present_{cat}"] = presence[cat]
        rows.append(row)

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    fieldnames = ["repo", "matched_files", "matched_files_count", "classified_test_fallback"] + [
        f"present_{c}" for c in CATEGORIES
    ]
    with open(OUT_CSV, "w", newline="", encoding="utf-8") as f:
        f.write(f"# 生态抽样交叉验证；random.seed({SEED})；候选池 {len(candidates)}，"
                f"抽样 {len(sampled)}\n")
        f.write(f"# 候选池构建：sources/<org>__<repo>/ 里含 RELEVANCE_PATTERN 调用证据的仓库，"
                f"排除官方 org / 第三方 SDK 客户端库(名字含 sdk+jev|typesafe) / 与 84 项目重名 / "
                f"demo-only(文件数<{DEMO_MIN_FILES})\n")
        f.write(f"# 抽样方法：候选池按仓库名排序后 random.sample(seed={SEED})，"
                f"脚本见 research/scripts/busywork_ecosystem_sample.py\n")
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        for row in rows:
            w.writerow(row)

    print(f"\n输出：{OUT_CSV}")

    file_counts_sorted = sorted(file_counts)
    n = len(file_counts_sorted)
    print(f"\n--- 每仓库匹配文件数分布（用于判断口径是否可比：84 项目基线每项目通常"
          f"只读 1-3 份登记文件） ---")
    if n:
        print(f"  min={file_counts_sorted[0]} median={file_counts_sorted[n // 2]} "
              f"mean={sum(file_counts)/n:.1f} max={file_counts_sorted[-1]}")
    test_pct = test_file_total / all_file_total * 100 if all_file_total else 0
    print(f"  测试文件占匹配文件比例：{test_file_total}/{all_file_total} ({test_pct:.1f}%)"
          f"——分类时已排除，不计入 12 类正则匹配")
    if fallback_repos:
        print(f"  {len(fallback_repos)} 个仓库匹配到的文件全部是测试文件，退回用测试文件分类："
              f"{fallback_repos}")

    print(f"\n--- 排除明细 ---")
    print(f"官方 org（{len(excluded_official)}）：{excluded_official[:10]}{' ...' if len(excluded_official) > 10 else ''}")
    print(f"第三方 SDK 客户端库（{len(excluded_sdk)}）：{excluded_sdk[:10]}{' ...' if len(excluded_sdk) > 10 else ''}")
    print(f"与 84 项目重名（{len(excluded_dedup)}）：{excluded_dedup}")
    print(f"demo-only（{len(excluded_demo)}）：{excluded_demo[:10]}{' ...' if len(excluded_demo) > 10 else ''}")

    print(f"\n--- {len(rows)} 个仓库，12 类出现率（未排序）---")
    pct = {}
    for cat in CATEGORIES:
        present_count = sum(r[f"present_{cat}"] for r in rows)
        p = present_count / len(rows) * 100 if rows else 0
        pct[cat] = p
        print(f"  {cat}: {present_count}/{len(rows)} ({p:.1f}%)")

    print(f"\n--- 按出现率从高到低排序 ---")
    for cat, p in sorted(pct.items(), key=lambda kv: -kv[1]):
        baseline = BASELINE_84.get(cat)
        diff = p - baseline if baseline is not None else None
        diff_str = f"  (84项目 {baseline:.1f}%，差 {diff:+.1f})" if diff is not None else ""
        print(f"  {cat}: {p:.1f}%{diff_str}")


if __name__ == "__main__":
    main()
