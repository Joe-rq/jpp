#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
重复杂活分类脚本。用于 `research/18-repetition-in-real-jev-projects.zh-CN.md`。

做什么：读 内部改写语料库（未随本次公开）里每个项目下的 `measure.toml` 里登记的判断核心（[[core]]
条目），把核心代码逐行按 12 类「杂活」正则匹配，算每类出现与否、每类行数、
杂活总行数占核心行数的比例。

两个范围（口径），预注册时已说明为什么要两个：
  - scope=core   ：measure.toml [[core]] 登记的精确行号范围（改写实测报告用的
                   「判断核心」定义）。这个范围按定义排除了命令行解析、网络请求、
                   日志、重试——所以 4/6/7 类在这个口径下会被系统性低估，1/5 类
                   也可能被砍掉一部分（重试、并发限制、调用循环往往写在核心行号
                   范围紧邻的胶水代码里，而不是阈值判断那几行本身）。
  - scope=file   ：[[core]] 条目指向的文件，取整份文件（不截断行号范围）。用来
                   补 core 口径的系统性遗漏，代价是会把明显不相关的代码也算进来
                   （用「业务本身」残余类兜底，不计入杂活占比的分子）。

行的判定：跳过空行与整行只有注释的行（Python `#`、C 系 `//` 见 COMMENT_RE）；
块注释（`/* ... */`、三引号文档字符串）不特殊处理，读成普通行——如果被误判成
杂活，属于本脚本的已知噪声，不是行为造假；已用独立子代理盲标注 10 个项目核对
误差（见同目录报告 §三）。

每行只归一类（排他，用于占比），归类顺序见 PRIORITY；同一行可能命中多个正则，
按 PRIORITY 顺序取第一个命中的类。多标签「出现与否」统计另算，不受排他顺序影响
（一行同时命中类 A 和类 B，两类的「出现」都算命中，即使排他计数只算给 A）。

用法：
  python3 research/scripts/busywork_classify.py            # 全量 84 项目，两种口径
  python3 research/scripts/busywork_classify.py --ids B1-01,B2-02   # 只跑指定项目（供人工抽查/校准子代理用）

公开发布说明：本脚本按私有工作区目录约定读取内部改写语料库（84 个真实开源项目的
原始代码与判断核心行号登记，见 measure.toml），该语料库未随本次公开——在公开仓库
里直接运行会因找不到目录而报错。已经算好的数据表见
research/data/2026-09-27-repetition/（本脚本原始输出目录名与此不同，属私有工作区
约定，发布时数据已重新归档到上述路径）。发布本脚本是为了方法透明，不是为了在
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
JEV_ROOT = EVAL_DIR.parent.parent  # 内部改写语料库根目录（未随本次公开发布，脚本在公开仓库里运行会因找不到该目录而报错）
REWRITE_DIR = JEV_ROOT / "internal_workspace" / "internal_rewrite_corpus" / "rewrites"  # private-workspace directory names generalized for publication; this path does not exist in the public repo
OUT_DIR = EVAL_DIR / "2026-09-27-重复杂活"

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
RESIDUAL = "0_业务本身"

# 排他归类的优先顺序（同一行命中多类时，取列表里靠前的类计入占比分子）。
# 越靠前越"结构特征明显、误判率低"；阈值/题面拼装这类通用性强、容易和别的
# 类混在同一行的排在后面，避免把明显的缓存/重试/并发行为吞并成"门槛"。
PRIORITY = [
    "5_缓存",
    "4_重试与失败",
    "7_并发限制",
    "1_调用循环与合批",
    "6_费用与调用计数",
    "12_校准与标注",
    "10_结果回流",
    "9_材料裁剪",
    "3_拿不准的处理",
    "2_门槛",
    "8_题面拼装",
    "11_多判断合成",
]

COMMENT_RE = re.compile(r"^\s*(#|//|/\*|\*|\"\"\"|''')")

# 正则按类分组；不区分大小写；语言以 Python/JS/TS 为主（84 项目里 49 Python、
# 19 TS、10 JS，覆盖 78/84；Go/Rust/Swift/Dart 共 6 项用通用词汇兜底，已知这几
# 项的语言特有写法（比如 Go 的 errgroup、Rust 的 tokio::sync::Semaphore）覆盖
# 不全，逐项人工核对见校准报告）。
PATTERNS = {
    "5_缓存": re.compile(
        r"lru_cache|functools\.cache\b|@cache\b|\bcache\s*=\s*\{|\bcache\s*=\s*dict\(|"
        r"\bcache\[|\.get\(\s*key|hashlib\.(sha256|md5)|cache_key|cachedir|"
        r"\bsqlite3\b|\bredis\b|memoize|\bMap\(\)\s*;.*cache|new Map\(\).*[Cc]ache|"
        r"localStorage|diskcache|shelve\.open|pickle\.(dump|load).*cache",
        re.IGNORECASE,
    ),
    "4_重试与失败": re.compile(
        r"\btry\s*:|\btry\s*\{|except\s+\w*Exception|except\s*:|\.catch\(|"
        r"\bretry\b|\bbackoff\b|max_retries|maxRetries|retry_count|n_retries|"
        r"time\.sleep\(|setTimeout\(|asyncio\.sleep\(|\bfallback\b|fail_open|"
        r"circuit.?breaker|\bexponential\b.*backoff|except\s+\(",
        re.IGNORECASE,
    ),
    "7_并发限制": re.compile(
        r"Semaphore\(|asyncio\.Semaphore|threading\.Semaphore|rate_?limit|"
        r"RateLimiter|p-limit|pLimit|\bthrottle\b|concurrency_limit|max_concurrent|"
        r"\bmutex\b|\.acquire\(\)|token_?bucket",
        re.IGNORECASE,
    ),
    "1_调用循环与合批": re.compile(
        r"asyncio\.gather|Promise\.all|Promise\.allSettled|ThreadPoolExecutor|"
        r"ProcessPoolExecutor|multiprocessing\.Pool|concurrent\.futures|"
        r"asyncio\.as_completed|\bbatch\b.*append|\bbuffer\b.*append|"
        r"for\s+[\w\s,]+\s+in\s+.*:\s*$|for\s*\(.*of\s+.*\)\s*\{|\.map\(async|"
        r"chunk(ed|s)?\(|\bqueue\.append",
        re.IGNORECASE,
    ),
    "6_费用与调用计数": re.compile(
        r"call_count|num_calls|n_calls|total_cost|token_count|token_usage|"
        r"\bbudget\b|cost_so_far|spend(_so_far)?|usage\.total|calls_made|"
        r"request_count|api_calls",
        re.IGNORECASE,
    ),
    "12_校准与标注": re.compile(
        r"\bcalibrat\w*|threshold_search|grid_search|sklearn|roc_auc|\bAUC\b|"
        r"labeled_data|ground_truth|gold_label|hand_labeled|"
        r"precision_recall|f1_score|"
        r"(?<!__future__ import )\bannotations?\b(?!\s*$)",
        re.IGNORECASE,
    ),
    "10_结果回流": re.compile(
        r"recent_signals|previous_(result|judgment|answer|confidence)|"
        r"last_(result|judgment|answer|confidence)|history\.append|"
        r"prior_(result|judgment)|\.append\(result\)|\.append\(judgment\)|"
        r"context\s*\+=|memory\.append|conversation_history|"
        r"\.choices\[|\.answers\[|second_\w*question|next_\w*question|"
        r"round_2|previous_choice|first\.\w+\[",
        re.IGNORECASE,
    ),
    "9_材料裁剪": re.compile(
        r"\[:\s*\d+\s*\]|\btruncate\(|textwrap\.(shorten|wrap)|top_k\b|\[:k\]|"
        r"\.slice\(0,|\bmax_len(gth)?\b|\bchunk_size\b|\.substring\(0,|"
        r"split_into_chunks|\.head\(\d+\)",
        re.IGNORECASE,
    ),
    "3_拿不准的处理": re.compile(
        r"\bunsure\b|\buncertain\b|\bambiguous\b|needs_review|needs_human|"
        r"\bunknown\b.*status|status\s*=\s*[\"']pending[\"']|\bpending_review\b|"
        r"else\s*:\s*$.*#.*(unsure|uncertain)|manual_review|"
        r"abs\([^)]*-\s*0\.5\)|abs\(0\.5\s*-[^)]*\)|gate(d|_threshold)\b",
        re.IGNORECASE,
    ),
    "2_门槛": re.compile(
        r"confidence\s*[<>]=?\s*0\.\d+|probability\s*[<>]=?\s*0\.\d+|"
        r"score\s*[<>]=?\s*0\.\d+|\bp\s*[<>]=?\s*0\.\d+|"
        r"THRESHOLD|_THRESHOLD|min_confidence|MIN_CONFIDENCE|"
        r"0\.\d+\s*[<>]=?\s*(confidence|probability|score)\b",
        re.IGNORECASE,
    ),
    "8_题面拼装": re.compile(
        r"f[\"']{1,3}.*\{.*(question|prompt|candidate)|"
        r"\.format\(.*question|question\s*=\s*[\"'].*\+|"
        r"prompt\s*=\s*[\"'].*\+|`.*\$\{.*\}.*`|"
        r"\bjson\.dumps\(candidates|join\(candidates\)|"
        r"\+\s*[\"']\\n[\"']\s*\+|template\.(render|format)",
        re.IGNORECASE,
    ),
    "11_多判断合成": re.compile(
        r"\bweighted_(sum|average)\b|\*\s*weight\b|\bvote\b|\bvoting\b|"
        r"Counter\(.*answers|\bcascad\w*|\bensemble\b|majority|"
        r"sum\(.*scores?\)|np\.average\(|statistics\.mean\(|"
        r"\bweighted\b|\bcomposite\b|expect(ed)?_score|expected_value|"
        r"risk_score\s*[+\-*]|combine_score|aggregate_score",
        re.IGNORECASE,
    ),
}

CATEGORY_LABEL = {c: c for c in CATEGORIES}

# --- v2：块级归属 + 上下文邻近，修复 v1 纯逐行关键词匹配的系统性低估 -------------
#
# v1 的问题（校准时发现）：一个 31 行的手写重试循环（例：B3-01 jev.py:118-148），
# 逐行关键词匹配只命中 `try`/`except` 那几行本身，中间的错误分类、退避计算、
# 截止时间判断全部落进「业务本身」，把一整段杂活算成了几行。反过来，`for x in y:`
# `try:` `.catch(` 这类关键词在任何代码里都极常见，不少命中跟 JEV 调用毫无关系
# （B1-05 的 `except (TypeError, ValueError)` 包的是参数解析，不是判断调用）。
#
# 修复两处：
#   1. 块级归属：对 1/4/7 三类（调用循环合批、重试失败、并发限制）——这三类的
#      本体是一段控制流块，不是一行——先找「块开启行」（for/try/with-semaphore
#      等），块开启行邻近（±CONTEXT_WINDOW 行内）出现判断调用的上下文标记时，
#      整个块（Python 按缩进、花括号语言按配对括号找块尾）都算这一类，不再只
#      数开启行本身。
#   2. 上下文邻近：1/4/6/7/8 类如果没有在判断调用上下文附近出现，不计入
#      （减少「任意一段解析/网络代码里的 try/except」这种假阳性）。2/3/5/9/10/
#      11/12 类词汇本身已经比较专一（`confidence`、`unsure`、cache 键、截断、
#      calibrate 等在业务代码里少见），不加这层过滤。
#
# 局限（老实写清楚，不假装这是真值）：花括号语言的块尾用朴素括号计数找，不处理
# 字符串/注释里的花括号，会有噪声；JEV_CONTEXT 本身也是关键词代理，不是语义理解；
# 用独立子代理盲标注核对过一部分（见报告正文）。
JEV_CONTEXT = re.compile(
    r"system_one|systemone|TypeSafeClient|typesafe|NoulAnswer|\bnoul\(|\bChoice\(|"
    r"\bScore\(|\bcriteria\s*=|\binstructions\s*=|\.judge\(|\bjev\b|Jev(Client|Error|Batch)?\(|"
    r"questions\s*[:=]|\bconfidence\b|\bprobabilit(y|ies)\b|\bJev[A-Z]\w*\(",
    re.IGNORECASE,
)
CONTEXT_WINDOW = 20  # 行

TRIGGER_1 = re.compile(
    r"^\s*for\s+[\w\s,]+\s+in\s+.+:\s*$|^\s*for\s*\(.*\)\s*\{|^\s*while\s+.+:\s*$|"
    r"asyncio\.gather\(|Promise\.all\(|Promise\.allSettled\(|ThreadPoolExecutor\(|"
    r"ProcessPoolExecutor\(|multiprocessing\.Pool\(",
    re.IGNORECASE,
)
TRIGGER_4 = re.compile(r"^\s*try\s*:\s*$|^\s*try\s*\{", re.IGNORECASE)
TRIGGER_7 = re.compile(
    r"Semaphore\(|async with .*[Ss]emaphore|with .*[Ss]emaphore\(", re.IGNORECASE
)
TRIGGERS = {"1_调用循环与合批": TRIGGER_1, "4_重试与失败": TRIGGER_4, "7_并发限制": TRIGGER_7}
CONTEXT_REQUIRED = {"1_调用循环与合批", "4_重试与失败", "6_费用与调用计数", "7_并发限制", "8_题面拼装"}

PYTHON_EXTS = {".py"}
BRACE_EXTS = {".js", ".jsx", ".ts", ".tsx", ".mjs", ".go", ".rs", ".swift", ".dart",
              ".java", ".c", ".cpp", ".cs", ".kt"}


def _indent_of(line: str) -> int:
    return len(line) - len(line.lstrip(" \t"))


def _python_block_end(lines, start_idx):
    base = _indent_of(lines[start_idx])
    end = start_idx + 1
    n = len(lines)
    while end < n:
        if lines[end].strip() == "":
            end += 1
            continue
        if _indent_of(lines[end]) > base:
            end += 1
        else:
            break
    return end


def _brace_block_end(lines, start_idx):
    depth = 0
    for ch in lines[start_idx]:
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
    if depth <= 0:
        # 开括号可能在下一两行（比如 `try` 单独一行，`{` 另起一行）
        for probe in range(start_idx + 1, min(start_idx + 3, len(lines))):
            for ch in lines[probe]:
                if ch == "{":
                    depth += 1
                elif ch == "}":
                    depth -= 1
            if depth > 0:
                start_idx = probe
                break
        if depth <= 0:
            return start_idx + 1
    end = start_idx + 1
    n = len(lines)
    while end < n and depth > 0:
        for ch in lines[end]:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
        end += 1
    return end


def _block_end(lines, start_idx, ext):
    if ext in PYTHON_EXTS:
        return _python_block_end(lines, start_idx)
    return _brace_block_end(lines, start_idx)


def classify_segment_v2(lines, ext):
    """lines: 0-indexed 原始行（未跳过空行/注释，块扩展需要看到真实结构）。
    返回 (exclusive_counts, presence, total_countable_lines, detail)。"""
    n = len(lines)
    excl = {c: 0 for c in CATEGORIES}
    excl[RESIDUAL] = 0
    presence = set()
    detail = []

    # 上下文覆盖：JEV_CONTEXT 命中行 ±CONTEXT_WINDOW 内都算「在判断调用附近」
    ctx_idx = [i for i, l in enumerate(lines) if JEV_CONTEXT.search(l)]
    covered = set()
    for i in ctx_idx:
        for j in range(max(0, i - CONTEXT_WINDOW), min(n, i + CONTEXT_WINDOW + 1)):
            covered.add(j)

    claimed = [None] * n  # 每行最终归类（None=未定）
    matched_multi = [set() for _ in range(n)]  # 每行命中的全部类（presence 用）

    # 第一步：块级归属（1/4/7），只在触发行落在上下文覆盖内时才展开整块
    order = ["4_重试与失败", "1_调用循环与合批", "7_并发限制"]  # 与 PRIORITY 大致一致
    for cat in order:
        trig = TRIGGERS[cat]
        for i in range(n):
            if claimed[i] is not None:
                continue
            if not trig.search(lines[i]):
                continue
            if i not in covered:
                continue  # 附近没有判断调用上下文，判定为通用代码，不算这类
            end = _block_end(lines, i, ext)
            for j in range(i, min(end, n)):
                if claimed[j] is None:
                    claimed[j] = cat
                matched_multi[j].add(cat)

    # 第二步：其余按原 PATTERNS 逐行匹配（含 1/4/7 的单行写法，供块识别漏掉的场景兜底；
    # 6/8 类要求上下文邻近，其余类别不设限制）
    for i in range(n):
        text = lines[i]
        for cat in CATEGORIES:
            if PATTERNS[cat].search(text):
                if cat in CONTEXT_REQUIRED and i not in covered:
                    continue
                matched_multi[i].add(cat)
                if claimed[i] is None:
                    claimed[i] = cat

    total = 0
    for i in range(n):
        stripped = lines[i].strip()
        if not stripped or COMMENT_RE.match(stripped):
            continue
        total += 1
        for cat in matched_multi[i]:
            presence.add(cat)
        if claimed[i] is None:
            excl[RESIDUAL] += 1
        else:
            excl[claimed[i]] += 1
        if matched_multi[i]:
            detail.append((i + 1, lines[i], claimed[i], sorted(matched_multi[i])))

    return excl, presence, total, detail


def parse_measure_toml(path: Path):
    """极简 TOML 解析：只认 [[core]]/[[host_shared]] 段落里的 path/ranges 字符串字段。"""
    core, host = [], []
    cur = None
    cur_table = None
    if not path.exists():
        return core, host
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if line == "[[core]]":
            if cur is not None:
                (core if cur_table == "core" else host).append(cur)
            cur = {}
            cur_table = "core"
            continue
        if line == "[[host_shared]]":
            if cur is not None:
                (core if cur_table == "core" else host).append(cur)
            cur = {}
            cur_table = "host"
            continue
        if line.startswith("[") and line not in ("[[core]]", "[[host_shared]]"):
            if cur is not None:
                (core if cur_table == "core" else host).append(cur)
            cur = None
            cur_table = None
            continue
        m = re.match(r'(\w+)\s*=\s*"(.*)"', line)
        if m and cur is not None:
            cur[m.group(1)] = m.group(2)
    if cur is not None:
        (core if cur_table == "core" else host).append(cur)
    return core, host


def expand_ranges(ranges_str: str, total_lines: int):
    if ranges_str == "all" or not ranges_str:
        return set(range(1, total_lines + 1))
    out = set()
    for part in ranges_str.split(","):
        part = part.strip()
        if "-" in part:
            a, b = part.split("-")
            out.update(range(int(a), int(b) + 1))
        elif part:
            out.add(int(part))
    return out


def resolve_path(project_dir: Path, raw_path: str) -> Path:
    p = Path(raw_path)
    if p.is_absolute():
        return p
    return project_dir / raw_path


def classify_lines(lines):
    """lines: list[str] (1-indexed 用不到，直接顺序处理)。
    返回 (exclusive_counts dict, presence set, total_countable_lines, matched_lines_detail)"""
    excl = {c: 0 for c in CATEGORIES}
    excl[RESIDUAL] = 0
    presence = set()
    total = 0
    detail = []  # (line_no, text, exclusive_cat, all_matched_cats)
    for i, raw in enumerate(lines, start=1):
        text = raw.rstrip("\n")
        stripped = text.strip()
        if not stripped:
            continue
        if COMMENT_RE.match(stripped):
            continue
        total += 1
        matched = [cat for cat in CATEGORIES if PATTERNS[cat].search(text)]
        for cat in matched:
            presence.add(cat)
        chosen = None
        for cat in PRIORITY:
            if cat in matched:
                chosen = cat
                break
        if chosen is None:
            excl[RESIDUAL] += 1
        else:
            excl[chosen] += 1
        if matched:
            detail.append((i, text, chosen, matched))
    return excl, presence, total, detail


def load_core_lines(project_dir: Path, entries, scope: str):
    """scope='core' 用 ranges 截取；scope='file' 用整份文件。
    返回 (all_lines_used, missing_files)。v1（朴素逐行）用，忽略语言与片段边界。"""
    all_lines = []
    missing = []
    seen_files = set()
    for e in entries:
        raw_path = e.get("path")
        if not raw_path:
            continue
        fp = resolve_path(project_dir, raw_path)
        if scope == "file":
            if fp in seen_files:
                continue
            seen_files.add(fp)
        if not fp.exists():
            missing.append(str(fp))
            continue
        try:
            text = fp.read_text(encoding="utf-8", errors="replace")
        except Exception:
            missing.append(str(fp))
            continue
        file_lines = text.splitlines()
        if scope == "file":
            all_lines.extend(file_lines)
        else:
            ranges = expand_ranges(e.get("ranges", "all"), len(file_lines))
            for ln in sorted(ranges):
                if 1 <= ln <= len(file_lines):
                    all_lines.append(file_lines[ln - 1])
    return all_lines, missing


def load_segments(project_dir: Path, entries, scope: str):
    """v2（块级归属）用：按文件、按 ranges 里逗号分隔的每一段各自成一个连续片段，
    块扩展不会跨着一个不连续的行号缺口展开。返回 (segments, missing_files)，
    segments = [(ext, lines_list), ...]。"""
    segments = []
    missing = []
    seen_files = set()
    for e in entries:
        raw_path = e.get("path")
        if not raw_path:
            continue
        fp = resolve_path(project_dir, raw_path)
        if scope == "file":
            if fp in seen_files:
                continue
            seen_files.add(fp)
        if not fp.exists():
            missing.append(str(fp))
            continue
        try:
            text = fp.read_text(encoding="utf-8", errors="replace")
        except Exception:
            missing.append(str(fp))
            continue
        file_lines = text.splitlines()
        ext = fp.suffix.lower()
        if scope == "file":
            segments.append((ext, file_lines))
            continue
        ranges_str = e.get("ranges", "all")
        if ranges_str == "all" or not ranges_str:
            segments.append((ext, file_lines))
            continue
        for part in ranges_str.split(","):
            part = part.strip()
            if not part:
                continue
            if "-" in part:
                a, b = part.split("-")
                a, b = int(a), int(b)
            else:
                a = b = int(part)
            seg = file_lines[max(0, a - 1):b]
            if seg:
                segments.append((ext, seg))
    return segments, missing


def _v1_result(lines, missing):
    excl, presence, total, detail = classify_lines(lines)
    busywork_lines = sum(v for k, v in excl.items() if k != RESIDUAL)
    ratio = (busywork_lines / total) if total else None
    return {
        "total_lines": total, "busywork_lines": busywork_lines, "ratio": ratio,
        "excl": excl, "presence": presence, "missing_files": missing, "detail": detail,
    }


def _v2_result(segments, missing):
    total_all = 0
    busywork_all = 0
    excl_all = {c: 0 for c in CATEGORIES}
    excl_all[RESIDUAL] = 0
    presence_all = set()
    detail_all = []
    for ext, seg_lines in segments:
        excl, presence, total, detail = classify_segment_v2(seg_lines, ext)
        total_all += total
        busywork_all += sum(v for k, v in excl.items() if k != RESIDUAL)
        for k, v in excl.items():
            excl_all[k] += v
        presence_all |= presence
        detail_all.extend(detail)
    ratio = (busywork_all / total_all) if total_all else None
    return {
        "total_lines": total_all, "busywork_lines": busywork_all, "ratio": ratio,
        "excl": excl_all, "presence": presence_all, "missing_files": missing, "detail": detail_all,
    }


def process_project(project_dir: Path):
    pid_parts = project_dir.name.split("-", 2)
    pid = pid_parts[0] + "-" + pid_parts[1] if len(pid_parts) >= 2 else project_dir.name
    toml_path = project_dir / "measure.toml"
    core_entries, host_entries = parse_measure_toml(toml_path)
    results = {"v1": {}, "v2": {}}
    for scope in ("core", "file"):
        lines, missing = load_core_lines(project_dir, core_entries, scope)
        results["v1"][scope] = _v1_result(lines, missing)
        segments, missing2 = load_segments(project_dir, core_entries, scope)
        results["v2"][scope] = _v2_result(segments, missing2)
    # scope=host：measure.toml [[host_shared]] 登记的文件，取整份文件。这是
    # 改写流程明确定义为「与判断逻辑无关、但两边都要写」的胶水代码——按定义，
    # 命令行解析、网络请求、日志、重试大多落在这里，不落在 [[core]] 里。用来
    # 检验「杂活主要在胶水代码里，不在判断核心里」这个次假设。
    if host_entries:
        lines, missing = load_core_lines(project_dir, host_entries, "file")
        results["v1"]["host"] = _v1_result(lines, missing)
        segments, missing2 = load_segments(project_dir, host_entries, "file")
        results["v2"]["host"] = _v2_result(segments, missing2)
    else:
        results["v1"]["host"] = None
        results["v2"]["host"] = None
    return pid, project_dir.name, results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", default="", help="逗号分隔的编号子集，如 B1-01,B2-02；为空跑全量")
    ap.add_argument("--dump-detail-for", default="", help="打印指定编号在指定 scope 下命中的逐行详情，格式 编号:scope")
    args = ap.parse_args()

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    projects = sorted([d for d in REWRITE_DIR.iterdir() if d.is_dir()])
    if args.ids:
        wanted = set(args.ids.split(","))
        projects = [d for d in projects if (d.name.split("-", 2)[0] + "-" + d.name.split("-", 2)[1]) in wanted]

    rows = {"v1": {"core": [], "file": [], "host": []}, "v2": {"core": [], "file": [], "host": []}}
    skipped_no_toml = []
    all_missing = []

    for pd in projects:
        if not (pd / "measure.toml").exists():
            skipped_no_toml.append(pd.name)
            continue
        pid, dirname, results = process_project(pd)
        for version in ("v1", "v2"):
            for scope in ("core", "file", "host"):
                r = results[version][scope]
                if r is None:
                    continue
                row = {
                    "id": pid,
                    "dir": dirname,
                    "scope": scope,
                    "total_lines": r["total_lines"],
                    "busywork_lines": r["busywork_lines"],
                    "busywork_ratio": round(r["ratio"], 4) if r["ratio"] is not None else "",
                }
                for cat in CATEGORIES:
                    row[f"count_{cat}"] = r["excl"][cat]
                    row[f"present_{cat}"] = 1 if cat in r["presence"] else 0
                row["count_" + RESIDUAL] = r["excl"][RESIDUAL]
                row["missing_files"] = ";".join(r["missing_files"])
                rows[version][scope].append(row)
                if version == "v2":
                    all_missing.extend(r["missing_files"])

        if args.dump_detail_for:
            want_id, want_scope = args.dump_detail_for.split(":")
            if pid == want_id:
                print(f"\n=== 详情 {pid} scope={want_scope}（v2 块级+上下文） ===")
                for ln, text, chosen, matched in results["v2"][want_scope]["detail"]:
                    print(f"{ln:5d} [{chosen}] matched={matched}\n      {text.strip()[:120]}")

    fieldnames = ["id", "dir", "scope", "total_lines", "busywork_lines", "busywork_ratio"]
    for cat in CATEGORIES:
        fieldnames.append(f"count_{cat}")
        fieldnames.append(f"present_{cat}")
    fieldnames.append("count_" + RESIDUAL)
    fieldnames.append("missing_files")

    out_paths = {}
    for version in ("v1", "v2"):
        suffix = "" if version == "v1" else "-v2块级"
        for scope, label in (("core", "核心"), ("file", "整文件"), ("host", "宿主")):
            path = OUT_DIR / f"84项目-scope{label}{suffix}.csv"
            out_paths[(version, scope)] = path
            with open(path, "w", newline="", encoding="utf-8") as f:
                w = csv.DictWriter(f, fieldnames=fieldnames)
                w.writeheader()
                for row in rows[version][scope]:
                    w.writerow(row)

    def summarize(rowlist, label):
        ratios = [r["busywork_ratio"] for r in rowlist if r["busywork_ratio"] != ""]
        print(f"\n--- {label}：{len(rowlist)} 项目，其中 {len(ratios)} 有可数行 ---")
        if ratios:
            ratios_sorted = sorted(ratios)
            print(f"占比中位数 {statistics.median(ratios):.3f}")
            n = len(ratios_sorted)
            q1 = ratios_sorted[n // 4]
            q3 = ratios_sorted[(3 * n) // 4]
            print(f"四分位区间约 [{q1:.3f}, {q3:.3f}]")
            print(f"均值 {statistics.mean(ratios):.3f}")
        for cat in CATEGORIES:
            present_count = sum(r[f"present_{cat}"] for r in rowlist)
            pct = present_count / len(rowlist) * 100 if rowlist else 0
            print(f"  {cat}: 出现于 {present_count}/{len(rowlist)} 项目 ({pct:.1f}%)")

    def union_and_hit8(version):
        by_id = {}
        for scope in ("core", "file", "host"):
            for row in rows[version][scope]:
                d = by_id.setdefault(row["id"], set())
                for cat in CATEGORIES:
                    if row[f"present_{cat}"]:
                        d.add(cat)
        print(f"\n--- [{version}] 合并口径（core∪file∪host 任一出现即算），{len(by_id)} 个项目 ---")
        hit8 = 0
        for cat in CATEGORIES:
            present_count = sum(1 for s in by_id.values() if cat in s)
            pct = present_count / len(by_id) * 100 if by_id else 0
            flag = " <-- >=30%" if pct >= 30 else ""
            if pct >= 30:
                hit8 += 1
            print(f"  {cat}: 出现于 {present_count}/{len(by_id)} 项目 ({pct:.1f}%){flag}")
        print(f"  达到 >=30% 的类别数：{hit8}/12")

    for version, vlabel in (("v1", "v1 朴素逐行关键词（旧，仅作对照，已知系统性低估杂活）"),
                             ("v2", "v2 块级归属 + 判断调用上下文邻近（新，本报告主口径）")):
        print(f"\n========== {vlabel} ==========")
        summarize(rows[version]["core"], f"[{version}] scope=core（measure.toml 精确行号）")
        summarize(rows[version]["file"], f"[{version}] scope=file（[[core]] 指向的整份文件）")
        summarize(rows[version]["host"], f"[{version}] scope=host（[[host_shared]] 整份文件，仅 {len(rows[version]['host'])} 个项目登记过）")
        union_and_hit8(version)

    if skipped_no_toml:
        print(f"\n跳过（无 measure.toml）：{skipped_no_toml}")
    if all_missing:
        uniq_missing = sorted(set(all_missing))
        print(f"\n找不到的源文件（{len(uniq_missing)} 个，多为 original/ 未放副本或路径已失效）：")
        for m in uniq_missing[:30]:
            print(f"  {m}")

    print("\n输出文件：")
    for key, path in out_paths.items():
        print(f"  {key}: {path}")


if __name__ == "__main__":
    main()
