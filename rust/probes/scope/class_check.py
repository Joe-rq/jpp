"""步 20b 探针对账（固定观察，0 费用）：类键借线前后。预测见 `地基/过程记录/工程-步20b.md`。

用法：python3 class_check.py <步 20b 之前的 jpp 二进制>
A. 无类记录：三探针（各自夹具与校准目录）与 20d-1 的三个范围复跑，新旧二进制的报告与账本逐字节比较。
B. 借线：R 题式的 440 行标注加 class = "form-topic"、question = 填好的题面，导成类记录（calib-class/，
   只含类记录）；去掉夹具里的 form-topic 手写线复跑三程序，数 W-class-line、W-calib-scope，
   value 与 20d-1 用题式线时（report-*.json）比较。
C. 题式记录与类记录并存：类层命中数。

步 20f（B75）起：来源 = 题式，同一题式的不同填法算一个来源。B 组的 440 行只有一个题式（21 个填法），
导入停在「待核：类记录来源不足（1 个来源，需 ≥ 2）」，三程序的借线出口变回冷；B 组的数字是步 20b
临时口径下的历史结果（`class_result.json`），见 `地基/过程记录/工程-步20f.md`。要复现借线，改用两个题式
（S4D + S4R，同一批材料各自有标注）造类记录。
"""
import hashlib
import json
import pathlib
import shutil
import subprocess
import sys

R = pathlib.Path(__file__).resolve().parents[2]  # rust-jpp
HERE = pathlib.Path(__file__).parent
NEW = R / "target" / "debug" / "jpp"
OLD = pathlib.Path(sys.argv[1])
WORK = HERE / "class-work"
shutil.rmtree(WORK, ignore_errors=True)
WORK.mkdir()


def run(binary, src, fixture, calib, tag):
    rep, led = WORK / f"{tag}.json", WORK / f"{tag}.ledger.jsonl"
    cmd = [str(binary), "run", str(src), "--fixtures", str(fixture), "--output", str(rep), "--ledger-out", str(led)]
    if calib:
        cmd += ["--calib", str(calib)]
    cwd = src.parent if "examples" not in str(src) else R
    p = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd)
    body = rep.read_bytes() if rep.exists() else b""
    ledger = led.read_bytes() if led.exists() else b""
    r = json.loads(body) if body else {}
    w = r.get("trace", {}).get("warnings", [])
    return {
        "exit": p.returncode,
        "report": hashlib.sha256(body).hexdigest()[:16],
        "ledger": hashlib.sha256(ledger).hexdigest()[:16],
        "value": r.get("value"),
        "class_line": sum(x.startswith("W-class-line") for x in w),
        "form_line": sum(x.startswith("W-form-line") for x in w),
        "calib_scope": sum(x.startswith("W-calib-scope") for x in w),
        "stderr": p.stderr.strip()[-200:],
    }


# ---------------- A：无类记录，新旧逐字节
cases_a = [
    ("winnow", R / "probes/winnow/winnow.jpp", R / "probes/winnow/fixture.json", None),
    ("folio", R / "probes/folio/folio.jpp", R / "probes/folio/fixture.json", None),
    ("entity-align", R / "probes/entity-align/align.jpp", R / "probes/entity-align/fixture.json", None),
    ("scope-folio", R / "probes/folio/folio.jpp", HERE / "fixture-folio.json", HERE / "calib"),
    ("scope-winnow", R / "probes/winnow/winnow.jpp", HERE / "fixture-winnow.json", HERE / "calib"),
    ("scope-topic", R / "examples/topic-relevance.jpp", HERE / "fixture-topic-relevance.json", HERE / "calib"),
]
out = {"A": {}, "B": {}, "C": {}}
for name, src, fx, cal in cases_a:
    old = run(OLD, src, fx, cal, f"A-{name}-old")
    new = run(NEW, src, fx, cal, f"A-{name}-new")
    out["A"][name] = {
        "report_same": old["report"] == new["report"],
        "ledger_same": old["ledger"] == new["ledger"],
        "class_line_new": new["class_line"],
        "exit": (old["exit"], new["exit"]),
    }

# ---------------- B：导类记录
ROOT = R.parents[1] / "实测" / "校准题式-2026-09-23"
fill_of = {}
for x in json.load(open(ROOT / "items4.json")):
    if x["t"] == "S4R":
        on, fill = x["state"]["on"], x["fill"]
        fill_of[hashlib.sha256((on + "\x1f" + fill).encode()).hexdigest()[:12]] = fill
rows = []
missing = 0
for line in open(HERE / "语义R-带材料.jsonl"):
    r = json.loads(line)
    f = fill_of.get(r["item"])
    if f is None:
        missing += 1
        continue
    tmpl = r["form"]["template"]
    r["question"] = tmpl.replace("{concept}", f)
    r["class"] = "form-topic"
    rows.append(r)
labels = WORK / "class-labels.jsonl"
with open(labels, "w") as fh:
    for r in rows:
        fh.write(json.dumps(r, ensure_ascii=False) + "\n")
CLASS = HERE / "calib-class"
shutil.rmtree(CLASS, ignore_errors=True)
p = subprocess.run([str(NEW), "calib-import", str(labels), "--calib-out", str(CLASS)], capture_output=True, text=True)
out["B"]["import_stdout_tail"] = p.stdout.strip()[-600:]
out["B"]["import_exit"] = p.returncode
out["B"]["rows"] = len(rows)
out["B"]["rows_missing_fill"] = missing
out["B"]["questions"] = len({r["question"] for r in rows})
recs = {f.name: json.load(open(f)) for f in CLASS.glob("*.json")}
out["B"]["records"] = {k: {"key": v["key"], "status": v["status"], "hi": v["hi"], "lo": v["lo"], "n": v["n"]} for k, v in recs.items()}
form_rec = json.load(open(next((HERE / "calib").glob("*.json"))))
out["B"]["form_record"] = {"status": form_rec["status"], "hi": form_rec["hi"], "lo": form_rec["lo"], "n": form_rec["n"]}

for name, src, fx in [
    ("folio", R / "probes/folio/folio.jpp", HERE / "fixture-folio.json"),
    ("winnow", R / "probes/winnow/winnow.jpp", HERE / "fixture-winnow.json"),
    ("topic-relevance", R / "examples/topic-relevance.jpp", HERE / "fixture-topic-relevance.json"),
]:
    res = run(NEW, src, fx, CLASS, f"B-{name}")
    prev = json.load(open(HERE / f"report-{name}.json"))
    out["B"][name] = {k: res[k] for k in ("exit", "class_line", "form_line", "calib_scope")}
    out["B"][name]["value_same_as_form_line_run"] = res["value"] == prev.get("value")

# ---------------- C：题式记录与类记录并存
BOTH = WORK / "calib-both"
BOTH.mkdir()
for d in (HERE / "calib", CLASS):
    for f in d.glob("*.json"):
        shutil.copy(f, BOTH / f.name)
for name, src, fx in [
    ("folio", R / "probes/folio/folio.jpp", HERE / "fixture-folio.json"),
    ("winnow", R / "probes/winnow/winnow.jpp", HERE / "fixture-winnow.json"),
    ("topic-relevance", R / "examples/topic-relevance.jpp", HERE / "fixture-topic-relevance.json"),
]:
    res = run(NEW, src, fx, BOTH, f"C-{name}")
    out["C"][name] = {k: res[k] for k in ("exit", "class_line", "form_line", "calib_scope")}

json.dump(out, open(HERE / "class_result.json", "w"), ensure_ascii=False, indent=1)
shutil.rmtree(WORK, ignore_errors=True)
print(json.dumps(out, ensure_ascii=False, indent=1))
