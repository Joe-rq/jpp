#!/usr/bin/env bash
# 公开仓库的 CI 检查入口（只在公开侧，见 PUBLIC-SNAPSHOT.md）。
# 取研究树 scripts/ci.sh 里在本仓库能跑、不读研究区私有文件的各项。文档片段与示例实跑不在这里：它由 workflow 的
# docs-and-examples 作业阻断地跑（scripts/doc_snippets.py，JPP_CI_MODE=fail），装机冒烟由 install-smoke 作业跑。
# JPP_CI_MODE=report（默认）：每项只报告、与基线比较，不失败；JPP_CI_MODE=fail：超基线或命令失败即非零退出。
# JPP_CI_GATE（默认 report）：挡位，只管两项——fmt（基线 0）、clippy（告警数不超基线）。
#   report 只报告；fail 时这两项任一不过，脚本即非零退出（CI 变红），不论 JPP_CI_MODE。其余各项仍跟 JPP_CI_MODE。
#   workflow 里的 JPP_CI_GATE 是总开关：fmt 在研究树清零、同步到本仓后，把它从 report 改成 fail。
#
# 与 ci.sh 的差别：
#   - cargo test 由 workflow 的阻断步单独跑，这里不重复。
#   - 不跑 trace（读研究区现行的 12）与 kill_switch（读 20-架构方案）。
#   - gen_profiles --check 读 ../foundation/profile/profiles；本仓的来源在 src/foundation，运行时临时建软链接。
#   - equiv_pairs 只跑文件都在本仓的等价写法对；refund 一对读研究区的评估目录，跳过并打印出来。
#   - 不跑 scripts/doc_snippets.py：文档片段与全部示例由 docs-and-examples 作业阻断地跑。
set -u
cd "$(dirname "$0")/.."
MODE="${JPP_CI_MODE:-report}"
GATE="${JPP_CI_GATE:-report}"
export JPP_CI_MODE="$MODE"
status=0
rows=()
# 单项运行：$1 是这一项的模式（报告 / 失败），$2 是名称，其余是命令。
run_step() {
  local m="$1" name="$2"; shift 2
  echo "== $name"
  local out rc
  out="$(JPP_CI_MODE="$m" "$@" 2>&1)"; rc=$?
  printf '%s\n' "$out"
  local first
  first="$(printf '%s\n' "$out" | grep -m1 -E '^\[|^合计' || true)"
  if [ $rc -eq 0 ]; then echo "   通过"; rows+=("| $name | 通过 | ${first//|//} |")
  else echo "   未通过"; rows+=("| $name | 未通过 | ${first//|//} |"); [ "$m" = fail ] && status=1; fi
}
step() { local name="$1"; shift; run_step "$MODE" "$name" "$@"; }
gate() { local name="$1"; shift; run_step "$GATE" "${name}（挡位 ${GATE}）" "$@"; }

gate "cargo fmt --check（计需重排文件数）" python3 scripts/fmt_clippy.py fmt
# Linux 上 clippy 比研究机（macOS）多报几条与平台有关的告警（例如只在 macOS 才构造的枚举变体「从不构造」）：
# Linux 用 scripts/baselines/clippy-linux.json（只在公开侧，同步时保留）当基线，其余平台用 clippy.json。基线只许减少。
if [ "$(uname -s)" = Linux ] && [ -f scripts/baselines/clippy-linux.json ]; then
  cp scripts/baselines/clippy-linux.json scripts/baselines/clippy.json
fi
gate "cargo clippy（计告警数）" python3 scripts/fmt_clippy.py clippy
# 计数只认「路径:行:列: warning|error: 」这一种短格式；工具链换版本后格式若变，计数会失真。
# 这里把缓存里的原始输出前几行打出来，便于核对计数口径（第二次运行只读缓存，不重新编译）。
echo "   clippy 原始输出前 5 行（核对计数口径）："
cargo clippy --workspace --all-targets --offline --message-format=short 2>&1 | grep -m5 -E "warning|error" | sed 's/^/     /'
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

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### 研究树 CI 检查（公开子集，模式 ${MODE}，挡位 ${GATE}）/ Research-tree CI checks (public subset, mode ${MODE}, gate ${GATE})"
    echo
    echo "| 检查 | 结果 | 首行读数 |"
    echo "|---|---|---|"
    printf '%s\n' "${rows[@]}"
    echo
  } >> "$GITHUB_STEP_SUMMARY"
fi
exit $status
