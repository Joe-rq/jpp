#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
改写实测数据提取脚本。

公开发布说明：本脚本按私有工作区的目录约定读取「内部改写语料库」——84 个真实开源
项目每个项目下的原始代码、J++ 改写程序、行数与调用数记录——该语料库本身未随本次
公开发布（对应的 GitHub 仓库均为公开开源项目，见发布报告按编号与仓库名引用）。在
公开仓库里直接运行本脚本会因为找不到语料库目录而报错；发布本脚本是为了方法透明，
已经算好的逐项数据表见 research/data/2026-09-26-rewrite-study/2026-09-26-rewrite-data.csv。

输入（私有工作区路径，未随本次公开）：
  <内部改写语料库>/<编号>-<项目名>/result.json 与 rerun.json（有 rerun.json 的以它为准，
  但按字段合并，不是整份文件二选一：见下方 pick_field 的优先级）
  <内部改写语料库上级>/选题.json（补 class / language / host_shared / reads_confidence 等选题期字段）
  <内部改写语料库上级>/解锁.json（受阻条目按施工步分类）

输出：
  2026-09-26-改写实测数据.csv   逐项一行（公开发布版本见上面的 research/data/ 路径）
  stdout                                  汇总统计（报告引用的数字全部来自这里）

字段合并原则（每个字段独立判断，不是整份文件二选一）：
  等价类别（equivalence）：rerun.equivalence 文本 > result.equivalence 文本 >
    选题.json 的 equivalence 字段 > 由 match 字段回退推断。
  行数、调用数：rerun 的对应字段 > result 的对应字段 > 选题.json 的对应字段（仅
    core_code_total / host_shared 用于兜底，且兜底时标注来源，供人工复核）。
  凡取不到、需要靠猜测才能填的字段，一律填「缺」，并列进"解析不出"清单，不猜数。

行数按内部改写流程说明 §五 的口径（未随本次公开）：只数判断核心（core_files），宿主共享
（host_shared）两边都不计入 raw_ratio / wrap_ratio；本脚本另算一个「含宿主」的
比值 ratio_with_host_raw = (core_raw + host_raw) / (jpp_raw + host_raw)，用
COCOMO II 的口径把两边共用的胶水代码也算进去（08 号研究文档 §一）。这个比值只用
raw（非空非注释行），不与折行归一的 wrap 混算，因为宿主行数只有 raw 口径的记录。
"""
import csv
import json
import re
import statistics
import sys
from collections import Counter
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
EVAL_DIR = SCRIPT_DIR.parent
BASE = EVAL_DIR.parent / "internal_workspace"  # private-workspace directory name generalized for publication; this path does not exist in the public repo
REWRITE_DIR = BASE / "rewrites"
XUANTI_PATH = BASE / "selection.json"
JIESUO_PATH = BASE / "unlock.json"

OUT_CSV = EVAL_DIR / "2026-09-26-改写实测数据.csv"

EXT_LANG = {
    ".py": "Python", ".js": "JavaScript", ".jsx": "JavaScript", ".ts": "TypeScript",
    ".tsx": "TypeScript", ".go": "Go", ".rs": "Rust", ".swift": "Swift",
    ".java": "Java", ".rb": "Ruby", ".kt": "Kotlin", ".dart": "Dart", ".c": "C",
    ".cpp": "C++", ".cs": "C#", ".php": "PHP", ".sh": "Shell", ".mjs": "JavaScript",
}

# 跨候选比较：原项目一次请求带 N 道题、覆盖多个候选，J++ 把每个候选拆成单独的一次
# 判断（缺口 B155/B156），调用次数因此高于原项目。名单来自各项 result.json 的
# calls.note（Q-04 9:27、B5-09 5:130、B5-12 2:9、B6-07 2:8），调用合计按此分两类。
CROSS_CANDIDATE = {"Q-04", "B5-09", "B5-12", "B6-07"}

MISSING = []  # (id, field, why) 缺项清单


def note_missing(item_id, field, why=""):
    MISSING.append((item_id, field, why))


def load_json(path: Path):
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as e:
        print(f"PARSE ERROR {path}: {e}", file=sys.stderr)
        return None


def parse_id(dirname: str) -> str:
    parts = dirname.split("-", 2)
    if len(parts) < 2:
        return dirname
    return parts[0] + "-" + parts[1]


def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool)


def sum_line_list(lst):
    """list of dicts, each possibly {"lines":.., "code":.., "wrap100":.., "tokens":..}"""
    total = 0
    found = False
    for item in lst:
        if isinstance(item, dict):
            for key in ("lines", "code"):
                if is_num(item.get(key)):
                    total += item[key]
                    found = True
                    break
    return total if found else None


def sum_line_list_field(lst, field):
    total = 0
    found = False
    for item in lst:
        if isinstance(item, dict) and is_num(item.get(field)):
            total += item[field]
            found = True
    return total if found else None


def sum_line_dict(d):
    """dict like {"matcher.py":79, "cli.py_x":3, "total":82, "note":"..."} ->
    (value_to_use, computed_from_numeric_leaves_excluding_total).

    观察到的键名不统一：见过 "total"、"core_total"、"host_total"。任何键名里带
    "total" 字样的一律当聚合值处理，既优先采用、也从"逐文件相加"的候选里排除，
    否则会把聚合值当成又一个文件重复加一遍（曾把 B3-05/B3-06 等一批项目的核心
    行数算出恰好两倍，已修）。"""
    total_keys = [k for k in d.keys() if is_num(d.get(k)) and "total" in k.lower()]
    numeric_items = {k: v for k, v in d.items() if is_num(v) and "total" not in k.lower()}
    computed = sum(numeric_items.values()) if numeric_items else None
    if total_keys:
        return d[total_keys[0]], computed
    return computed, computed


def sum_host(host_field):
    """返回 (行数或 None, 说明)。None 表示这份记录里没有可用的宿主行数，需要由
    调用方回退到 选题.json。"""
    if host_field is None:
        return None, "缺"
    if isinstance(host_field, list):
        if not host_field:
            return None, "own record 空列表"
        s = sum_line_list(host_field)
        return (s, "own record(list)") if s is not None else (None, "own record 无法解析")
    if isinstance(host_field, dict):
        if not host_field:
            return None, "own record 空字典"
        val, _ = sum_line_dict(host_field)
        return (val, "own record(dict)") if val is not None else (None, "own record 无法解析")
    if is_num(host_field):
        return host_field, "own record(scalar)"
    return None, "own record 类型未知"


def extract_core(lines_obj, xuanti_item, item_id):
    if isinstance(lines_obj, dict):
        # B2-05 这类"一个项目内多个独立判断点"的结构，字段全部嵌在 combined 里，
        # 顶层没有 core_lines / core_files。
        combined = lines_obj.get("combined")
        if isinstance(combined, dict) and is_num(combined.get("core_lines_total")):
            return combined["core_lines_total"], "lines.combined.core_lines_total"
        # 有的项目在 measure.toml 里用 data= 排除字面量候选文本，排除后的数才是
        # 报告里实际拿去算 raw_ratio 的分子（如 B4-13：263 排除后是 250）。这个
        # 字段名比 core_files 的原始加总更接近"报告里用的那个数"，优先取它。
        excl = lines_obj.get("core_lines_after_data_exclusion")
        if is_num(excl):
            return excl, "lines.core_lines_after_data_exclusion"
        cl = lines_obj.get("core_lines")
        if is_num(cl):
            return cl, "rerun/result: lines.core_lines"
        cf = lines_obj.get("core_files")
        if isinstance(cf, list):
            s = sum_line_list(cf)
            if s is not None:
                return s, "rerun/result: lines.core_files(list)"
        if isinstance(cf, dict) and cf:
            val, computed = sum_line_dict(cf)
            if val is not None:
                return val, "rerun/result: lines.core_files(dict)"
    if xuanti_item and is_num(xuanti_item.get("core_code_total")):
        note_missing(item_id, "core_lines", "own record 缺，回退选题.json core_code_total")
        return xuanti_item["core_code_total"], "选题.json core_code_total"
    note_missing(item_id, "core_lines", "own record 与选题.json 均无")
    return None, "缺"


def extract_core_wrap(lines_obj):
    if isinstance(lines_obj, dict):
        cw = lines_obj.get("core_wrap100")
        if is_num(cw):
            return cw
        cf = lines_obj.get("core_files")
        if isinstance(cf, list):
            s = sum_line_list_field(cf, "wrap100")
            if s is not None:
                return s
    return None


def extract_core_tokens(lines_obj):
    if isinstance(lines_obj, dict):
        ct = lines_obj.get("core_tokens")
        if is_num(ct):
            return ct
        cf = lines_obj.get("core_files")
        if isinstance(cf, list):
            s = sum_line_list_field(cf, "tokens")
            if s is not None:
                return s
    return None


def extract_jpp(lines_obj, item_id):
    if isinstance(lines_obj, dict):
        combined = lines_obj.get("combined")
        if isinstance(combined, dict) and is_num(combined.get("jpp_lines_total")):
            return combined["jpp_lines_total"], "lines.combined.jpp_lines_total"
        for key in ("jpp_lines", "jpp_lines_raw"):
            jl = lines_obj.get(key)
            if is_num(jl):
                return jl, f"lines.{key}"
        jf = lines_obj.get("jpp")
        if isinstance(jf, dict):
            for key in ("lines", "raw"):
                if is_num(jf.get(key)):
                    return jf[key], f"lines.jpp.{key}"
        if is_num(jf):
            return jf, "lines.jpp(scalar)"
    note_missing(item_id, "jpp_lines", "own record 无法解析")
    return None, "缺"


def extract_jpp_wrap(lines_obj):
    if isinstance(lines_obj, dict):
        jw = lines_obj.get("jpp_wrap100")
        if is_num(jw):
            return jw
        jf = lines_obj.get("jpp")
        if isinstance(jf, dict) and is_num(jf.get("wrap100")):
            return jf["wrap100"]
    return None


def extract_jpp_tokens(lines_obj):
    if isinstance(lines_obj, dict):
        jt = lines_obj.get("jpp_tokens")
        if is_num(jt):
            return jt
        jf = lines_obj.get("jpp")
        if isinstance(jf, dict) and is_num(jf.get("tokens")):
            return jf["tokens"]
    return None


def extract_ratio(lines_obj, key):
    if isinstance(lines_obj, dict):
        combined = lines_obj.get("combined")
        combined_key = key.replace("_ratio", "_ratio_combined")
        if isinstance(combined, dict) and is_num(combined.get(combined_key)):
            return combined[combined_key]
        if is_num(lines_obj.get(key)):
            return lines_obj[key]
    return None


def pick_str_field(field, *sources):
    """按来源顺序取第一个非空字符串字段，返回 (值, 来源标签)"""
    for label, src in sources:
        if isinstance(src, dict):
            v = src.get(field)
            if isinstance(v, str) and v.strip():
                return v, label
    return None, None


def pick_any_field(field, *sources):
    for label, src in sources:
        if isinstance(src, dict) and field in src and src[field] is not None:
            return src[field], label
    return None, None


def classify_equivalence(rerun, result, xuanti_item, item_id):
    eq_text, eq_source = pick_str_field(
        "equivalence", ("rerun", rerun), ("result", result)
    )
    if eq_text is None and xuanti_item:
        eq = xuanti_item.get("equivalence")
        if isinstance(eq, str) and eq.strip():
            eq_text, eq_source = eq, "选题.json"

    match_val, match_source = pick_any_field("match", ("rerun", rerun), ("result", result))
    if isinstance(match_val, str):
        match_norm = match_val.strip().lower()
    else:
        match_norm = match_val

    if eq_text:
        t = eq_text.strip()
        t_lower = t.lower()
        if t.startswith("全部") or t.startswith("全等价"):
            category = "全部"
        elif t.startswith("部分") or t_lower.startswith("partial"):
            category = "部分"
        elif t.startswith("不适用") or t.startswith("不可比") or t_lower.startswith("not applicable"):
            category = "不可比"
        else:
            category = "未知(见equivalence文本)"
            note_missing(item_id, "equivalence_category", f"equivalence 文本首字未识别: {t[:20]}")
    else:
        if match_norm is True:
            category = "全部"
        elif match_norm == "partial":
            category = "部分"
        elif match_norm is False:
            category = "部分或不等价(无equivalence文本，按match=false计)"
        elif match_norm is None and isinstance(result, dict) and \
                isinstance(result.get("part_A_duelo"), dict) and result["part_A_duelo"].get("match") is True:
            # Q-05：duelo() 一致、torneo() 按设计 match=null（随机数序列不同），按部分等价计
            category = "部分"
            match_val = "partial(A=true,B=null)"
        elif match_norm is None:
            category = "不可比或未知(无equivalence文本，match为空)"
        else:
            category = "未知"
        note_missing(item_id, "equivalence_text", "own record 与选题.json 均无 equivalence 文本，按 match 字段回退")

    return category, eq_text, eq_source, match_val, match_source


def infer_language(xuanti_item, own_core_keys, lines_obj=None):
    exts = []
    if xuanti_item:
        for cf in xuanti_item.get("core_files", []):
            p = cf.get("path", "")
            suf = Path(p).suffix
            if suf:
                exts.append(suf)
    if not exts and own_core_keys:
        for k in own_core_keys:
            m = re.search(r"\.([A-Za-z0-9]+)(?:[^A-Za-z0-9]|$)", k)
            if m:
                exts.append("." + m.group(1))
    if not exts and isinstance(lines_obj, dict):
        # 新批次只在 core_source_note 里写文件名（如 "lib/resume-review.ts 11-214"）
        m = re.search(r"[\w/.-]+(\.(?:py|ts|tsx|js|jsx|mjs|go|rs|swift|java|rb|kt|dart))\b",
                      str(lines_obj.get("core_source_note", "")))
        if m:
            exts.append(m.group(1))
    if not exts:
        return "缺"
    top = Counter(exts).most_common(1)[0][0]
    return EXT_LANG.get(top, top)


def get_own_core_keys(lines_obj):
    if not isinstance(lines_obj, dict):
        return []
    cf = lines_obj.get("core_files")
    if isinstance(cf, dict):
        return [k for k in cf.keys() if "total" not in k.lower()]
    if isinstance(cf, list):
        keys = []
        for item in cf:
            if isinstance(item, dict):
                f = item.get("file", "")
                keys.append(Path(f).name)
        return keys
    return []


def gap_titles(result, rerun):
    """只取 gap 的短标题（"gap" 或 "id" 键），不取 detail/desc/why（可能含原项目源码片段）。"""
    titles = []
    for src in (rerun, result):
        if not isinstance(src, dict):
            continue
        gaps = src.get("gaps")
        if isinstance(gaps, list):
            for g in gaps:
                if isinstance(g, dict):
                    t = g.get("gap") or g.get("id") or g.get("desc", "")[:24]
                    if t:
                        titles.append(str(t))
                elif isinstance(g, str):
                    titles.append(g)
            if titles:
                break
    return titles


def main():
    xuanti = load_json(XUANTI_PATH)
    xuanti_items = {}
    if xuanti:
        for it in xuanti.get("items", []):
            xuanti_items[it["id"]] = it

    unlock = load_json(JIESUO_PATH)
    unlock_items = unlock.get("items", {}) if unlock else {}

    dirs = sorted(d.name for d in REWRITE_DIR.iterdir() if d.is_dir())

    rows = []
    for dname in dirs:
        item_id = parse_id(dname)
        ddir = REWRITE_DIR / dname
        result = load_json(ddir / "result.json")
        rerun = load_json(ddir / "rerun.json")
        xuanti_item = xuanti_items.get(item_id)

        repo, repo_src = pick_any_field("repo", ("rerun", rerun), ("result", result))
        if repo is None and xuanti_item:
            repo = xuanti_item.get("repo")
        commit, _ = pick_any_field("commit", ("rerun", rerun), ("result", result))
        if commit is None and xuanti_item:
            commit = xuanti_item.get("commit")
        license_, _ = pick_any_field("license", ("rerun", rerun), ("result", result))
        jpp_commit, _ = pick_any_field("jpp_commit", ("rerun", rerun), ("result", result))
        if jpp_commit is None:
            note_missing(item_id, "jpp_commit", "无 rerun.json，未记录改写时用的 jpp 版本号")

        lines_obj, lines_src = pick_any_field("lines", ("rerun", rerun), ("result", result))
        calls_obj, _ = pick_any_field("calls", ("rerun", rerun), ("result", result))
        # Q-05 这类"分两部分"的记录（part_A_duelo 可比、part_B_torneo 按设计 match=null），
        # 顶层没有 lines/calls/match，行数与调用数取可比的 part_A。
        part_a = result.get("part_A_duelo") if isinstance(result, dict) else None
        if lines_obj is None and isinstance(part_a, dict) and isinstance(part_a.get("lines"), dict):
            lines_obj, lines_src = part_a["lines"], "result.part_A_duelo"
        # 部分记录把调用数写在 verification 文本里（"calls: 2 vs 4"）；calls 对象里也有
        # 用 original_jev/jpp_judge 命名的（Q-03）。
        if not (isinstance(calls_obj, dict) and (calls_obj.get("original") is not None or calls_obj.get("jpp") is not None)):
            if isinstance(calls_obj, dict) and is_num(calls_obj.get("original_jev")) and is_num(calls_obj.get("jpp_judge")):
                calls_obj = {"original": calls_obj["original_jev"], "jpp": calls_obj["jpp_judge"],
                             "ratio": calls_obj.get("jev_ratio")}
            else:
                vtexts = []
                for src in (rerun, result, part_a):
                    v = src.get("verification") if isinstance(src, dict) else None
                    if isinstance(v, list):
                        vtexts += [x for x in v if isinstance(x, str)]
                for vt in vtexts:
                    m = re.match(r"\s*calls:\s*(\d+)\s*vs\s*(\d+)", vt)
                    if m:
                        o, j = int(m.group(1)), int(m.group(2))
                        calls_obj = {"original": o, "jpp": j, "ratio": round(j / o, 4) if o else None}
                        break

        category, eq_text, eq_source, match_val, match_source = classify_equivalence(
            rerun, result, xuanti_item, item_id
        )

        core_lines, core_src = extract_core(lines_obj, xuanti_item, item_id)
        # rerun.json 有时把 core_files 字段重置为空列表（B1-11 观察到的回归：
        # result.json 记 core_lines=137，同一项目的 rerun.json 记 0），选中的
        # 那份来源如果算出 0 而另一份记录或选题.json 有更大的数，判定为回归、
        # 不采信 0，改按"另一份 own record 优先，其次选题.json"重新取。
        # 另一份记录的 lines 子树（result 与 rerun 各自的 "lines" 字段，不是整份
        # 文件），用于下面几处"这份记录的字段回归成 0/空，换另一份"的兜底。
        alt_lines_obj = (result or {}).get("lines") if lines_src == "rerun" else (rerun or {}).get("lines")

        core_regressed = False
        if core_lines == 0:
            alt_core, alt_src = extract_core(alt_lines_obj, xuanti_item, item_id) if alt_lines_obj else (None, "缺")
            if alt_core:
                note_missing(
                    item_id, "core_lines_regression",
                    f"{lines_src}.lines 算出 core_lines=0，改用{('result' if lines_src=='rerun' else 'rerun')}.lines 的 {alt_core}（{alt_src}）",
                )
                core_lines, core_src = alt_core, alt_src
                core_regressed = True
        core_wrap = extract_core_wrap(lines_obj)
        core_tokens = extract_core_tokens(lines_obj)
        if core_regressed and alt_lines_obj:
            # core_lines 换了来源，wrap/tokens 也要跟着换，否则同一项目里行数和
            # 折行数、token 数会各自来自不同的（一份是回归前、一份是回归后的）记录。
            core_wrap = extract_core_wrap(alt_lines_obj) or core_wrap
            core_tokens = extract_core_tokens(alt_lines_obj) or core_tokens

        jpp_lines, jpp_src = extract_jpp(lines_obj, item_id)
        jpp_wrap = extract_jpp_wrap(lines_obj)
        jpp_tokens = extract_jpp_tokens(lines_obj)
        if not jpp_lines and alt_lines_obj:
            alt_jpp, alt_jsrc = extract_jpp(alt_lines_obj, item_id)
            if alt_jpp:
                note_missing(item_id, "jpp_lines_regression", f"改用另一份记录的 jpp_lines={alt_jpp}（{alt_jsrc}）")
                jpp_lines, jpp_src = alt_jpp, alt_jsrc
                jpp_wrap = extract_jpp_wrap(alt_lines_obj) or jpp_wrap
                jpp_tokens = extract_jpp_tokens(alt_lines_obj) or jpp_tokens

        raw_ratio_reported = extract_ratio(lines_obj, "raw_ratio")
        wrap_ratio_reported = extract_ratio(lines_obj, "wrap_ratio")
        token_ratio_reported = extract_ratio(lines_obj, "token_ratio")

        raw_ratio_recomputed = (
            round(core_lines / jpp_lines, 4) if is_num(core_lines) and is_num(jpp_lines) and jpp_lines else None
        )
        wrap_ratio_recomputed = (
            round(core_wrap / jpp_wrap, 4) if is_num(core_wrap) and is_num(jpp_wrap) and jpp_wrap else None
        )

        # 同一处回归（rerun.json 的 lines 子树整体失真）会连带把 raw_ratio/wrap_ratio
        # 报告值也带成 0；core_lines 已经在上面纠正过，这里让报告值跟着改用重算值，
        # 否则汇总统计会把这一项当成"省了 0 倍"的离群点算进去。
        if raw_ratio_reported == 0 and is_num(raw_ratio_recomputed) and raw_ratio_recomputed > 0:
            note_missing(item_id, "raw_ratio_regression", f"报告值0，随 core_lines 回归改用重算值{raw_ratio_recomputed}")
            raw_ratio_reported = raw_ratio_recomputed
        if wrap_ratio_reported == 0 and is_num(wrap_ratio_recomputed) and wrap_ratio_recomputed > 0:
            note_missing(item_id, "wrap_ratio_regression", f"报告值0，随 core_lines 回归改用重算值{wrap_ratio_recomputed}")
            wrap_ratio_reported = wrap_ratio_recomputed
        if token_ratio_reported == 0 and is_num(core_tokens) and is_num(jpp_tokens) and jpp_tokens:
            note_missing(item_id, "token_ratio_regression", "报告值0，随 core_lines 回归改用重算值")
            token_ratio_reported = round(core_tokens / jpp_tokens, 4)

        ratio_mismatch = ""
        if is_num(raw_ratio_reported) and is_num(raw_ratio_recomputed):
            if abs(raw_ratio_reported - raw_ratio_recomputed) > 0.02:
                ratio_mismatch = f"raw_ratio 报告值{raw_ratio_reported}与重算值{raw_ratio_recomputed}不符"
                note_missing(item_id, "raw_ratio_check", ratio_mismatch)

        # 宿主行数：own record 优先，取不到回退 选题.json host_shared。字段名不统一，
        # 见过 host_shared / host_shared_lines / host_shared_files 三种。
        own_host_lines, own_host_src = None, None
        for host_key in ("host_shared", "host_shared_lines", "host_shared_files"):
            own_host_field, host_field_src = pick_any_field(host_key, ("rerun", rerun), ("result", result))
            if own_host_field is None and isinstance(lines_obj, dict):
                # 新批次的记录把宿主行数嵌在 lines 里（Q-03 的 lines.host_shared、
                # B6-11/B7-02/Q-05 的 lines.host_shared_lines）
                own_host_field, host_field_src = lines_obj.get(host_key), "lines"
            if own_host_field is not None:
                own_host_lines, own_host_src = sum_host(own_host_field)
                own_host_src = f"{host_key}@{host_field_src}:{own_host_src}"
                if own_host_lines is not None:
                    break
        xuanti_host_lines = None
        if xuanti_item:
            hs = xuanti_item.get("host_shared", [])
            if isinstance(hs, list) and hs:
                s = sum_line_list_field(hs, "code")
                if s is not None:
                    xuanti_host_lines = s
        if xuanti_host_lines is not None:
            host_lines = xuanti_host_lines
            host_src = "选题.json host_shared"
            if own_host_lines is not None and own_host_lines != xuanti_host_lines:
                note_missing(
                    item_id, "host_lines_check",
                    f"own record={own_host_lines}({own_host_src}) 与 选题.json={xuanti_host_lines} 不一致，采用选题.json",
                )
        elif own_host_lines is not None:
            host_lines = own_host_lines
            host_src = own_host_src
        else:
            host_lines = 0
            host_src = "两处均未记录，按0（不代表确无宿主代码）"
            if xuanti_item is not None:
                note_missing(item_id, "host_lines", "选题.json 无 host_shared 且 own record 为空，按0")

        ratio_with_host_raw = None
        if is_num(core_lines) and is_num(jpp_lines) and is_num(host_lines):
            denom = jpp_lines + host_lines
            if denom:
                ratio_with_host_raw = round((core_lines + host_lines) / denom, 4)

        core_share = None
        if is_num(core_lines) and is_num(host_lines) and (core_lines + host_lines) > 0:
            core_share = round(core_lines / (core_lines + host_lines), 4)

        calls_original = calls_jpp = calls_ratio = None
        if isinstance(calls_obj, dict):
            calls_original = calls_obj.get("original")
            calls_jpp = calls_obj.get("jpp")
            calls_ratio = calls_obj.get("ratio")
            if calls_original is None and calls_jpp is None:
                # B2-05 这类多子部分的结构：{"memory_gate":{original,jpp,...},
                # "compaction":{...}}，没有顶层 original/jpp，逐子部分求和。
                sub_orig = sub_jpp = 0
                found_sub = False
                for v in calls_obj.values():
                    if isinstance(v, dict) and is_num(v.get("original")) and is_num(v.get("jpp")):
                        sub_orig += v["original"]
                        sub_jpp += v["jpp"]
                        found_sub = True
                if found_sub:
                    calls_original, calls_jpp = sub_orig, sub_jpp
                    calls_ratio = round(sub_jpp / sub_orig, 4) if sub_orig else None
        if calls_original is None:
            note_missing(item_id, "calls_original", "own record 无 calls.original")

        own_core_keys = get_own_core_keys(lines_obj)
        language = infer_language(xuanti_item, own_core_keys, lines_obj)
        if language == "缺":
            note_missing(item_id, "language", "选题.json 无该条目且 own record 文件名无法识别扩展名")

        klass = xuanti_item.get("class") if xuanti_item else None
        class_purpose = xuanti_item.get("class_purpose") if xuanti_item else None
        if klass is None:
            note_missing(item_id, "class", "选题.json 无该条目（如 B3-07b）")

        reads_confidence = xuanti_item.get("reads_confidence") if xuanti_item else None

        gaps = gap_titles(result, rerun)

        has_rerun = rerun is not None

        rows.append(dict(
            id=item_id, dir=dname, repo=repo, commit=commit, license=license_,
            jpp_commit=jpp_commit or "缺", has_rerun=has_rerun,
            language=language, klass=klass or "缺", class_purpose=class_purpose or "缺",
            equivalence_category=category, equivalence_source=eq_source or "缺(按match推断)",
            match_raw=match_val, match_source=match_source or "缺",
            core_lines=core_lines if core_lines is not None else "缺", core_lines_source=core_src,
            core_wrap=core_wrap if core_wrap is not None else "缺",
            core_tokens=core_tokens if core_tokens is not None else "缺",
            host_lines=host_lines, host_lines_source=host_src,
            jpp_lines=jpp_lines if jpp_lines is not None else "缺", jpp_lines_source=jpp_src,
            jpp_wrap=jpp_wrap if jpp_wrap is not None else "缺",
            jpp_tokens=jpp_tokens if jpp_tokens is not None else "缺",
            raw_ratio_reported=raw_ratio_reported if raw_ratio_reported is not None else "缺",
            raw_ratio_recomputed=raw_ratio_recomputed if raw_ratio_recomputed is not None else "缺",
            wrap_ratio_reported=wrap_ratio_reported if wrap_ratio_reported is not None else "缺",
            wrap_ratio_recomputed=wrap_ratio_recomputed if wrap_ratio_recomputed is not None else "缺",
            token_ratio_reported=token_ratio_reported if token_ratio_reported is not None else "缺",
            ratio_with_host_raw=ratio_with_host_raw if ratio_with_host_raw is not None else "缺",
            core_share_of_core_plus_host=core_share if core_share is not None else "缺",
            calls_original=calls_original if calls_original is not None else "缺",
            calls_jpp=calls_jpp if calls_jpp is not None else "缺",
            calls_ratio=calls_ratio if calls_ratio is not None else "缺",
            reads_confidence=reads_confidence or "缺",
            call_class="跨候选比较" if item_id in CROSS_CANDIDATE else "其他",
            gap_titles="; ".join(gaps)[:300] if gaps else "",
            ratio_mismatch_note=ratio_mismatch,
        ))

    # ---- 写 CSV ----
    fieldnames = list(rows[0].keys())
    with open(OUT_CSV, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        for r in rows:
            w.writerow(r)

    # ---- 汇总统计 ----
    print(f"=== 改写实测数据汇总（{len(rows)} 项，来自 {REWRITE_DIR}） ===\n")

    eq_counter = Counter(r["equivalence_category"] for r in rows)
    print("等价类别分布：")
    for k, v in eq_counter.most_common():
        print(f"  {k}: {v}")
    print()

    has_rerun_n = sum(1 for r in rows if r["has_rerun"])
    print(f"有 rerun.json（新版 jpp 4ea646a9 重跑过）：{has_rerun_n} / {len(rows)}")
    print(f"仅 result.json（未重跑，jpp 版本缺）：{len(rows) - has_rerun_n} / {len(rows)}\n")

    lang_counter = Counter(r["language"] for r in rows)
    print("语言分布：")
    for k, v in lang_counter.most_common():
        print(f"  {k}: {v}")
    print()

    class_counter = Counter(r["klass"] for r in rows if r["klass"] != "缺")
    print(f"覆盖到的选题类数（本批全部项去重后）：{len(class_counter)}")
    print()

    # 参与行数统计的项目：core_lines、jpp_lines 都是数字（排除"不可比"的 B3-07 等）
    numeric_rows = [r for r in rows if is_num(r["core_lines"]) and is_num(r["jpp_lines"])]
    excluded_ids = [r["id"] for r in rows if r not in numeric_rows]
    print(f"参与行数/倍率统计的项目数：{len(numeric_rows)}（排除 {len(excluded_ids)} 项不可比或行数缺失："
          f"{', '.join(excluded_ids) if excluded_ids else '无'}）\n")

    def qstats(name, key, rows_):
        vals = [r[key] for r in rows_ if is_num(r[key])]
        if not vals:
            print(f"{name}: 无数据")
            return
        vals_sorted = sorted(vals)
        n = len(vals_sorted)
        median = statistics.median(vals_sorted)
        try:
            q1, q3 = statistics.quantiles(vals_sorted, n=4)[0], statistics.quantiles(vals_sorted, n=4)[2]
        except Exception:
            q1 = q3 = None
        print(f"{name}: n={n} 中位数={median:.3f} Q1={q1:.3f} Q3={q3:.3f} 最小={min(vals_sorted):.3f} 最大={max(vals_sorted):.3f}")

    print("---- 倍率统计（不含宿主，wrap 口径为主）----")
    qstats("wrap_ratio（逐项，取报告值优先重算值）", "wrap_ratio_reported", numeric_rows)
    wrap_vals = []
    for r in numeric_rows:
        v = r["wrap_ratio_reported"] if is_num(r["wrap_ratio_reported"]) else r["wrap_ratio_recomputed"]
        if is_num(v):
            wrap_vals.append(v)
    if wrap_vals:
        print(f"  合并口径 n={len(wrap_vals)} 中位数={statistics.median(wrap_vals):.3f}")

    qstats("raw_ratio（逐项报告值）", "raw_ratio_reported", numeric_rows)
    qstats("token_ratio（逐项报告值，注意 <1 表示 J++ token 更多）", "token_ratio_reported", numeric_rows)

    # 汇总口径（Σcore / Σjpp）
    sum_core = sum(r["core_lines"] for r in numeric_rows)
    sum_jpp = sum(r["jpp_lines"] for r in numeric_rows)
    print(f"\n汇总口径（不含宿主，raw）：Σ核心行={sum_core}，ΣJ++行={sum_jpp}，"
          f"合计倍率={sum_core/sum_jpp:.3f}")

    sum_core_wrap = sum(r["core_wrap"] for r in numeric_rows if is_num(r["core_wrap"]))
    sum_jpp_wrap = sum(r["jpp_wrap"] for r in numeric_rows if is_num(r["jpp_wrap"]))
    n_wrap = sum(1 for r in numeric_rows if is_num(r["core_wrap"]) and is_num(r["jpp_wrap"]))
    if sum_jpp_wrap:
        print(f"汇总口径（不含宿主，wrap，n={n_wrap} 项有折行数据）：Σ核心={sum_core_wrap}，"
              f"ΣJ++={sum_jpp_wrap}，合计倍率={sum_core_wrap/sum_jpp_wrap:.3f}")

    print("\n---- 含宿主口径（COCOMO：胶水代码计入，raw）----")
    host_rows = [r for r in numeric_rows if is_num(r["ratio_with_host_raw"])]
    qstats("ratio_with_host_raw（逐项）", "ratio_with_host_raw", host_rows)
    total_host_lines = sum(r["host_lines"] for r in numeric_rows if is_num(r["host_lines"]))
    print(f"Σ宿主行={total_host_lines}")
    denom = sum_jpp + total_host_lines
    if denom:
        print(f"汇总口径（含宿主，raw）：(Σ核心{sum_core}+Σ宿主{total_host_lines}) / (ΣJ++{sum_jpp}+Σ宿主{total_host_lines}) "
              f"= {(sum_core+total_host_lines)/denom:.3f}")
    qstats("core_share_of_core_plus_host（core/(core+host) 逐项，Jones 30% 编码占比的类比）",
           "core_share_of_core_plus_host", numeric_rows)

    print("\n---- 分布区间（wrap_ratio，合并口径）----")
    buckets = [(0, 1), (1, 2), (2, 3), (3, 5), (5, 10), (10, float("inf"))]
    for lo, hi in buckets:
        c = sum(1 for v in wrap_vals if lo <= v < hi)
        label = f"[{lo},{hi})" if hi != float("inf") else f"[{lo},+)"
        print(f"  {label}: {c}")

    print("\n---- 调用数 ----")
    calls_rows = [r for r in rows if is_num(r["calls_original"]) and is_num(r["calls_jpp"])]
    sum_calls_orig = sum(r["calls_original"] for r in calls_rows)
    sum_calls_jpp = sum(r["calls_jpp"] for r in calls_rows)
    print(f"参与调用数统计的项目：{len(calls_rows)} / {len(rows)}")
    print(f"Σ原项目调用={sum_calls_orig}，ΣJ++调用={sum_calls_jpp}，比值={sum_calls_jpp/sum_calls_orig:.4f}"
          if sum_calls_orig else "无有效调用数")
    for cls in ("跨候选比较", "其他"):
        sub = [r for r in calls_rows if r["call_class"] == cls]
        so, sj = sum(r["calls_original"] for r in sub), sum(r["calls_jpp"] for r in sub)
        print(f"  {cls}：{len(sub)} 项，Σ原项目={so}，ΣJ++={sj}，比值={sj/so:.4f}" if so else f"  {cls}：无")
    off = [r for r in calls_rows if r["calls_original"] != r["calls_jpp"]]
    print("  调用数两边不等的项目：" + "; ".join(f"{r['id']}({r['calls_original']}:{r['calls_jpp']},{r['call_class']})" for r in off))
    reads_conf_n = sum(1 for r in rows if r["reads_confidence"] == "是")
    print(f"读 confidence 字段设阈值的项目数（真机上与桩不保证同构，内部改写流程说明 §六·11，未随本次公开）：{reads_conf_n} / {len(rows)}")

    print("\n---- 受阻队列（选题.json status=受阻，本批未改写）----")
    blocked_all = [it for it in xuanti_items.values() if it.get("status") == "受阻"]
    ids_with_dir_prefix = {r["id"] for r in rows}
    still_blocked = [it for it in blocked_all if it["id"] not in ids_with_dir_prefix]
    print(f"选题.json 标记受阻：{len(blocked_all)} 项；其中已实际改写（本批目录存在）：{len(blocked_all)-len(still_blocked)} 项；"
          f"仍未改写：{len(still_blocked)} 项")
    step_counter = Counter()
    needs_20j4 = []
    for it in still_blocked:
        u = unlock_items.get(it["id"])
        steps = u["steps"] if u else []
        for s in steps:
            step_counter[s] += 1
        if "20j-4" in steps:
            needs_20j4.append(it["id"])
    print(f"仍未改写项目按解锁施工步计数：{dict(step_counter)}")
    print(f"其中需要 20j-4（内部第三档施工，原按内部进度排在赛后）解锁的：{len(needs_20j4)} 项 -> {needs_20j4}")

    print(f"\n---- 解析不出 / 需人工复核的字段（共 {len(MISSING)} 条）----")
    for item_id, field, why in MISSING:
        print(f"  {item_id}\t{field}\t{why}")

    print(f"\nCSV 已写入：{OUT_CSV}")


if __name__ == "__main__":
    main()
