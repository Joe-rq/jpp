#!/usr/bin/env bash
# 公开仓库的 CI 检查入口（只在公开侧，见 PUBLIC-SNAPSHOT.md）。
# 取研究树 scripts/ci.sh 里在本仓库能跑、不读研究区私有文件的各项，再加文档片段与示例实跑。
# JPP_CI_MODE=report（默认）：每项只报告、与基线比较，不失败；JPP_CI_MODE=fail：超基线或命令失败即非零退出。
#
# 与 ci.sh 的差别：
#   - cargo test 由 workflow 的阻断步单独跑，这里不重复。
#   - 不跑 trace（读研究区现行的 12）与 kill_switch（读 20-架构方案）。
#   - gen_profiles --check 读 ../foundation/profile/profiles；本仓的来源在 src/foundation，运行时临时建软链接。
#   - equiv_pairs 只跑文件都在本仓的等价写法对；refund 一对读研究区的评估目录，跳过并打印出来。
#   - 另跑 scripts/doc_snippets.py：GUIDE、README 等文档里的代码片段与全部示例。
set -u
cd "$(dirname "$0")/.."
MODE="${JPP_CI_MODE:-report}"
export JPP_CI_MODE="$MODE"
status=0
rows=()
step() {
  local name="$1"; shift
  echo "== $name"
  local out rc
  out="$("$@" 2>&1)"; rc=$?
  printf '%s\n' "$out"
  local first
  first="$(printf '%s\n' "$out" | grep -m1 -E '^\[|^合计' || true)"
  if [ $rc -eq 0 ]; then echo "   通过"; rows+=("| $name | 通过 | ${first//|//} |")
  else echo "   未通过"; rows+=("| $name | 未通过 | ${first//|//} |"); [ "$MODE" = fail ] && status=1; fi
}

step "cargo fmt --check（计需重排文件数）" python3 scripts/fmt_clippy.py fmt
step "cargo clippy（计告警数）" python3 scripts/fmt_clippy.py clippy
for s in deps lines grep_effect_names grep_constants grep_paths grep_fill grep_rules_checker grep_rt_codes; do
  step "$s" python3 "scripts/$s.py"
done

gen_profiles_check() {
  local link=../foundation made=0
  if [ ! -e "$link" ]; then ln -s src/foundation "$link"; made=1; fi
  python3 scripts/gen_profiles.py --check; local rc=$?
  [ $made = 1 ] && rm "$link"
  return $rc
}
step "gen_profiles --check（发行画像与 src/foundation 的来源一致，B73）" gen_profiles_check

equiv_pairs_public() {
  python3 - <<'PY'
import json, sys
sys.path.insert(0, "scripts")
import equiv_pairs as e
for p in json.loads(e.MANIFEST.read_text(encoding="utf-8"))["pairs"]:
    files = [p[k] for k in ("slow", "fast", "source", "fixtures") if k in p]
    missing = [f for f in files if not (e.ROOT / f).exists()]
    if missing:
        print(f"[equiv_pairs_{p['name']}] 跳过：文件不在本仓（{', '.join(missing)}）")
        continue
    e.KINDS[p["kind"]](p)
PY
}
step "equiv_pairs（等价写法对调用比，只报告）" equiv_pairs_public
step "doc_snippets（文档片段与示例实跑）" python3 scripts/doc_snippets.py

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### 研究树 CI 检查（公开子集，模式 ${MODE}）/ Research-tree CI checks (public subset, mode ${MODE})"
    echo
    echo "| 检查 | 结果 | 首行读数 |"
    echo "|---|---|---|"
    printf '%s\n' "${rows[@]}"
    echo
  } >> "$GITHUB_STEP_SUMMARY"
fi
exit $status
