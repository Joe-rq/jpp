"""从已有实验结果（experiments/*.json、raw/*/summary*.json）汇总出模型档案。
字段能从已有结果填的填；填不了的标「未测」并写出该由哪项测。用法：
  .venv/bin/python -m foundation.profile.build_from_raw [--model jev-1.13.0]
"""
from __future__ import annotations
import glob, json, math, re, statistics, sys
from collections import Counter
from datetime import date
from pathlib import Path

EXP = Path(__file__).resolve().parents[1] / "experiments"
RAW = EXP / "raw"
UNTESTED = "未测"


def _load(p: Path):
    try:
        return json.loads(p.read_text())
    except Exception:
        return None


def wilson(k: int, n: int, z: float = 1.96) -> list[float] | None:
    if not n:
        return None
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [round(c - h, 3), round(c + h, 3)]


def untested(by: str):
    return {"value": UNTESTED, "by": by}


def window(e5, e5n, e10rows, ejson=None, ejson_hi=None):
    w = {"representation_note": "窗口常数按 state 表示分列：text_slots = [JVR] 文字槽标记；json_slots = JSON 具名槽（E-JSON/E-JSON-hi）。"}
    text = {}
    if ejson and ejson_hi:
        t, th = ejson["summary"]["text"], ejson_hi["summary"]["text"]
        text["claim_bearing_ctx"] = {
            "flip_frac_by_ctx_tokens": {"~100": t["d4"]["flip_frac"], "~1000": th["d16"]["flip_frac"], "~1800": th["d32"]["flip_frac"]},
            "mean_abs_delta_by_ctx_tokens": {"~100": t["d4"]["mean_abs_delta"], "~1000": th["d16"]["mean_abs_delta"], "~1800": th["d32"]["mean_abs_delta"]},
            "onset_dose_docs": t.get("onset_dose"), "n_per_cell": 28,
            "bound": {"kind": "upper", "token": 1000, "reason": "≈1,000 token 带主张语境下翻转 60.7%，读数被语境接管；E5 的 500 处 median 0.11 是 lower"},
            "usable_lower": 500, "source": "E-JSON + E-JSON-hi + E5"}
    w["text_slots"] = text if text else {"claim_bearing_ctx": untested("window_repr --repr text")}
    js = {}
    if ejson and ejson_hi:
        j, jh = ejson["summary"]["json"], ejson_hi["summary"]["json"]
        js["claim_bearing_ctx"] = {
            "flip_frac_by_ctx_tokens": {"~100": j["d4"]["flip_frac"], "~1000": jh["d16"]["flip_frac"], "~1800": jh["d32"]["flip_frac"]},
            "mean_abs_delta_by_ctx_tokens": {"~100": j["d4"]["mean_abs_delta"], "~1000": jh["d16"]["mean_abs_delta"], "~1800": jh["d32"]["mean_abs_delta"]},
            "onset_dose_docs": j.get("onset_dose"), "n_per_cell": 28,
            "bound": {"kind": "lower", "token": 1800, "reason": "≈1,800 token 带主张语境翻转 3.6%，未触发"},
            "upper": untested("window_repr --repr json（3k / 5k / 8k 剂量，尚未包装）"),
            "source": "E-JSON + E-JSON-hi"}
    w["json_slots"] = js if js else {"claim_bearing_ctx": untested("window_repr --repr json")}
    if e5:
        w["noul_claim_bearing"] = {
            "unit": "filler token (nominal) → median/max |drift| of noul reading",
            "median_drift": e5["median_abs_drift_by_tok"], "max_drift": e5["max_abs_drift_by_tok"],
            "flips_beyond_noise": e5["flips_beyond_noise_by_tok"],
            "n_survivors": e5["n_survivors"], "reps": 3,
            "representation": "text",
            "bound": {"kind": "lower", "token": 500, "reason": "500 处 median 0.11 已 ≥ 2δ_noul；更小的剂量未测（文字渲染下的常数，见 text_slots）"}}
    else:
        w["noul_claim_bearing"] = untested("window")
    if e5n:
        w["noul_neutral"] = {"representation": "text", "median_drift": e5n["median_drift"], "max_drift": e5n["max_drift"],
                             "n": len(e5n["detail"]), "reps": 3,
                             "bound": {"kind": "lower", "token": 8000, "reason": "8000 中性填充 median 0.017，未见带偏"}}
    else:
        w["noul_neutral"] = untested("window")
    # choice / score 窗口：E10 组1/组2 的最大状态 token（一致率仍可接受处）
    w["choice"] = {"notes_100tok_candidates": {"max_state_tokens_ok": 1760, "consistency": 1.00, "K": 16,
                                               "bound": "lower", "n_runs": 30},
                   "code_300tok_candidates": {"max_state_tokens_ok": 1528, "consistency": 0.83, "K": 8,
                                              "bound": "lower", "n_runs": 30},
                   "source": "E10 组 1"} if e10rows else untested("choice_k")
    w["score"] = {"max_state_tokens_ok": 821, "anchors": 10, "rerun_consistency": 0.98, "bound": "lower",
                  "source": "E10 组 2"} if e10rows else untested("score_anchor")
    return w


def delta(e1):
    if not e1:
        return {k: untested("delta") for k in ("noul", "choice_prob_chosen", "choice_confidence", "score")}
    out = {}
    imm = e1["immediate_1_2_3"]["by_type"]
    post = e1.get("post_gap_4_5", {}).get("by_type", {})
    for k in ("noul", "choice_prob_chosen", "choice_confidence", "score"):
        a, b = imm.get(k, {}), post.get(k, {})
        out[k] = {"immediate": {"n": a.get("n"), "p95": a.get("p95"), "p99": a.get("p99"), "max": a.get("max")},
                  "after_gap": {"n": b.get("n"), "p95": b.get("p95"), "p99": b.get("p99"), "max": b.get("max")}}
    return out


def flip_rate(e1, e5):
    out = {}
    if e1:
        k, n = map(int, e1["immediate_1_2_3"]["choice_label_mismatch"].split("/"))
        out["choice_argmax_rerun"] = {"rate": round(k / n, 4), "n": n, "ci95": wilson(k, n), "source": "E1"}
    else:
        out["choice_argmax_rerun"] = untested("delta")
    if e5:
        out["noul_outlet_under_filler"] = {"flips": e5["flips_beyond_noise_by_tok"], "source": "E5",
                                           "note": "过线翻转按迟滞 δ 判，三档剂量均 0"}
    return out


def batch(e2, e10_g3):
    out = {}
    if e2:
        out["noul"] = {"alone_vs_batched_mean_abs_diff": {c: v["mean"] for c, v in e2["alone_mean_vs_condition_mean"].items()},
                       "max": {c: v["max"] for c, v in e2["alone_mean_vs_condition_mean"].items()},
                       "n_per_condition": 30, "reps": 3,
                       "choice_label_mismatch": e2["choice_label_mismatch_within_condition"], "source": "E2/E3"}
    else:
        out["noul"] = untested("batch")
    out["score"] = ({"cross_batch_mode_consistency": 0.90, "with_peers_vs_alone": 0.80,
                     "within_config_rerun": 0.97, "n_objects": 20, "batches": 4, "reps": 3,
                     "ci95_cross_batch": wilson(18, 20), "source": "E10 组 3",
                     "rule": "状态只放锚与对象不放同伴时才可跨材料偏序排序"}
                    if e10_g3 else untested("score_anchor"))
    return out


def cost(e4, e8ok):
    out = {"price_usd_per_input_token": 0.042 / 1e6, "output_billed": False}
    if e4:
        rows = e4["raw_rows"]
        by = {}
        for r in rows:
            by.setdefault(r["n"], []).append(r["input_tokens"])
        toks = {str(n): statistics.median(v) for n, v in sorted(by.items())}
        out["tokens_by_n_questions_300tok_state"] = toks
        if "1" in toks and "200" in toks:
            out["tokens_per_question"] = round((toks["200"] - toks["1"]) / 199, 1)
        out["latency_median_by_n"] = {k: v["median"] for k, v in e4["by_n"].items()}
        out["ratio_50_over_1_time"] = e4.get("ratio_50_over_1")
    else:
        out["tokens_per_question"] = untested("cost")
    if e8ok:
        out["regression"] = {"intercept_tokens": 271, "state_char_coef": 1.00, "question_char_coef": 0.88,
                             "n_calls": 1088, "source": "E8 回归"}
    else:
        out["regression"] = untested("concurrency")
    return out


def concurrency(e8q1):
    if not e8q1:
        return {"upper_bound": untested("concurrency")}
    h = e8q1["http"]
    return {"lower_bound_ok": 32, "throughput_calls_per_s": {"c8": 9, "c32": 35},
            "latency_s": {"p50": h["p50"], "p95": h["p95"], "max": h["max"]},
            "n_http": h["n"], "n429": h["n429"], "n529": h["n529"],
            "upper_bound": {"value": UNTESTED, "by": "concurrency（≥64；E10 在并发 16 + 大状态时见 SSL EOF，上限与状态大小相关）"},
            "source": "E8"}


def e10_g1_rows():
    rows = []
    for f in glob.glob(str(RAW / "e10" / "g1_*.json")):
        d = _load(Path(f))
        if not d:
            continue
        name = Path(f).stem  # g1_<dom>_K<k>_s<set>_sh<shuffle>_r<rep>
        parts = name.split("_")
        try:
            dom, K, st = parts[1], int(parts[2][1:]), parts[3]
        except Exception:
            continue
        ans = d.get("answers", {})
        q = next(iter(ans.values()), {}) if isinstance(ans, dict) else {}
        probs = q.get("probabilities") or q.get("probs") or {}
        letter = q.get("choice") or (max(probs, key=probs.get) if probs else None)
        # 置换后字母不可比：从状态文本里把字母映回候选正文（"## 候选 A\n正文"）
        mapping = {}
        st_text = d.get("state") if isinstance(d.get("state"), str) else json.dumps(d.get("state"), ensure_ascii=False)
        for m in re.finditer(r"^#+ 候选 ([A-P])[^\n]*\n(.*?)(?=^#+ 候选 [A-P]|\Z)", st_text, re.S | re.M):
            mapping[m.group(1)] = m.group(2).strip()[:300]
        chosen = mapping.get(letter, letter)
        rows.append({"dom": dom, "K": K, "set": st, "chosen": chosen, "pmax": max(probs.values()) if probs else None,
                     "tokens": d.get("input_tokens")})
    return rows


def k_limit(rows):
    if not rows:
        return {"by_candidate_tokens": untested("choice_k")}
    return {"by_candidate_tokens": {
        "<=120": {"K_max": 16, "consistency": 1.00, "source": "E10 笔记段"},
        "120-250": {"K_max": UNTESTED, "by": "choice_k（补测一格）"},
        ">=300": {"K_max": 4, "reason": "E10 K=8 一致率 0.83 但 E9b/E9c/E9d 8 选项首位偏置 25–39%；默认改 K 道 noul 一层",
                  "source": "E10 + E9b/E9c/E9d"}},
        "hard_max_options": 255, "source_hard_max": "官方文档 D1"}


def position_bias(rows, e9, e9c, e9d):
    out = {}
    if rows:
        by = {}
        for r in rows:
            by.setdefault((r["dom"], r["K"], r["set"]), Counter())[r["chosen"]] += 1
        agg = {}
        for (d, k, st), c in by.items():
            agg.setdefault(f"{d}_K{k}", []).append(max(c.values()) / sum(c.values()))
        out["choice_argmax_consistency_E10"] = {k: {"mode_share_mean": round(statistics.mean(v), 2), "n_sets": len(v),
                                                    "runs_per_set": 15} for k, v in sorted(agg.items())}
        out["note_E10"] = "16 集里 14 集重跑一致率 1.00；不一致全来自换顺序，7 次翻转里 6 次新众数落在第 0 位"
    if e9:
        out["choice_first_pos_share_E9_8way"] = {"first": 0.23, "second": 0.18, "uniform": 0.125}
        out["noul_position_effect_E9"] = {"reading_range_mean": 0.07, "p95": 0.24}
    if e9c:
        out["choice_pos_dist_E9c_8way"] = e9c.get("choice_pos_dist")
        out["choice_first_pos_share_E9c"] = e9c.get("choice_first_pos_share")
    if e9d:
        out["choice_first_pos_share_E9d"] = e9d["summary"].get("choice_first_pos_share")
    if not out:
        out = {"choice": untested("choice_k")}
    out["uniform_reference"] = 0.125
    return out


def anchors(e10ok):
    if not e10ok:
        return {"convergence": untested("score_anchor")}
    return {"expected_level_drift_vs_0": {"3": 0.26, "5": 0.28, "10": 0.25},
            "adjacent_config_drift": {"3->5": 0.15, "5->10": 0.07},
            "argmax_consistency": {"0v5": 0.75, "3v5": 0.75, "5v10": 0.90},
            "within_config_rerun": {"0": 0.98, "3": 0.97, "5": 1.00, "10": 0.98},
            "convergence_point": 5, "default_per_level": 1, "n_objects": 20, "reps": 5, "source": "E10 组 2"}


def calibration(ecal0):
    if not ecal0:
        return {"ece_by_source": untested("E-CAL（需 300 条人工标注）")}
    return {"ece_by_source": {k: {"n": v["n"], "ece": round(v["ece"], 3), "brier": round(v["brier"], 3),
                                  "auc": round(v["auc"], 3)} for k, v in ecal0.items()},
            "rule": "校准是字面化程度的函数：错误字面可见时 ECE ≈ 0.07 可当概率；不可见时 ECE ≈ 0.23 只当序数",
            "zh_reliability_curve": untested("E-CAL 正式版（e_cal_labels.csv 300 条待 Nature 标注）"),
            "source": "E-CAL-0"}


def language(e3):
    if not e3:
        return untested("E3 重跑")
    zh = [v["zh"]["gap_avg3"] for v in e3.values() if isinstance(v, dict) and "zh" in v]
    en = [v["en"]["gap_avg3"] for v in e3.values() if isinstance(v, dict) and "en" in v]
    return {"probe_gap_zh_mean": round(statistics.mean(zh), 3), "probe_gap_en_mean": round(statistics.mean(en), 3),
            "n_questions": len(zh), "rule": "默认中文题面；个别题英文缝更大，按题选", "source": "E3"}


def build(model: str = "jev-1.13.0") -> dict:
    e1 = _load(EXP / "e1_final_report.json")
    e2 = _load(EXP / "e2_results.json")
    e3 = _load(EXP / "e3_results.json")
    e4 = _load(EXP / "e4_v2_results.json")
    e5 = _load(EXP / "e5_results.json")
    e5n = _load(EXP / "e5_neutral_result.json")
    e8q1 = _load(RAW / "e8" / "summary_q1_c32.json")
    e9 = _load(RAW / "e9" / "summary.json")
    e9c = _load(RAW / "e9c" / "summary.json")
    e9d = _load(RAW / "e9d" / "summary.json")
    ecal0 = _load(RAW / "e_cal0_summary.json")
    ejson = _load(RAW / "e-json" / "summary.json")
    ejson_hi = _load(RAW / "e-json-hi" / "summary.json")
    rows = e10_g1_rows()
    e10ok = (RAW / "e10" / "run_stats.json").exists()
    prof = {
        "model_version": model, "date": str(date.today()),
        "sources": ["E1", "E2", "E3", "E4v2", "E5", "E5-neutral", "E8", "E9", "E9b", "E9c", "E9d", "E10", "E-CAL-0", "E-JSON", "E-JSON-hi"],
        "window": window(e5, e5n, rows, ejson, ejson_hi),
        "delta": delta(e1),
        "flip_rate": flip_rate(e1, e5),
        "batch_invariance": batch(e2, e10ok),
        "cost": cost(e4, bool(e8q1)),
        "concurrency": concurrency(e8q1),
        "k_limit": k_limit(rows),
        "position_bias": position_bias(rows, e9, e9c, e9d),
        "anchors": anchors(e10ok),
        "calibration": calibration(ecal0),
        "language": language(e3),
        "state_representation_default": "json_slots",
        # jv 运行时引用的两项（建造第 3 步加）：保守线非实测、置换同调用串扰未测
        "lines": {"safety_default": {"hi": 0.75, "lo": 0.25, "n": 0, "bound": "none",
                                     "status": "待 E-CAL 定（非实测常数）",
                                     "source": "沿用 v0 core/params.DEFAULTS.safety_default_hi/lo（施工单 v0.2 常数）；jv 冷校准的临时出口用它，非档案实测",
                                     "by": "E-CAL（e_cal_labels.csv 300 条标注后按域扫线）"}},
        "modalities_accepted": ["text"],
        "question_types": ["noul", "choice", "score"],
        "notes": "值为「未测」的字段带 by= 指向该由 run.py 的哪一项测；上界/下界见各 bound 字段。",
    }
    prof["batch_invariance"]["choice_same_call_perm_crosstalk"] = untested("E-PERM-SAME-CALL（预注册草案见 experiments/EXPERIMENTS.md）")
    # 统计已填 / 未测
    def walk(o, acc):
        if isinstance(o, dict):
            if o.get("value") == UNTESTED:
                acc["untested"] += 1
                return
            for v in o.values():
                if v == UNTESTED:
                    acc["untested"] += 1
                else:
                    walk(v, acc)
        elif isinstance(o, list):
            for v in o:
                walk(v, acc)
        else:
            acc["filled"] += 1
    acc = {"filled": 0, "untested": 0}
    walk({k: v for k, v in prof.items() if k not in ("model_version", "date", "sources", "notes")}, acc)
    prof["field_stats"] = acc
    return prof


if __name__ == "__main__":
    model = sys.argv[sys.argv.index("--model") + 1] if "--model" in sys.argv else "jev-1.13.0"
    p = build(model)
    out = Path(__file__).resolve().parent / "profiles" / f"{model}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(p, ensure_ascii=False, indent=1))
    print(f"写到 {out}；已填 {p['field_stats']['filled']} 叶字段，未测 {p['field_stats']['untested']} 项")
