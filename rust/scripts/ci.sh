#!/usr/bin/env bash
# J++ 持续集成入口（21 §九·1）。本地与公开仓库的 workflow 调同一个脚本。
# JPP_CI_MODE=report（默认，步 0 起）：每项只报告、记基线，不失败；
# JPP_CI_MODE=fail：超过基线或命令失败即非零退出。各脚本转失败模式的步见 21 §九·1。
set -u
cd "$(dirname "$0")/.."
MODE="${JPP_CI_MODE:-report}"
status=0
step() {
  local name="$1"; shift
  echo "== $name"
  if "$@"; then echo "   通过"; else echo "   未通过"; [ "$MODE" = fail ] && status=1; fi
}
step "cargo fmt --check（计需重排文件数）" python3 scripts/fmt_clippy.py fmt
step "cargo clippy（计告警数；失败模式即 -D warnings）" python3 scripts/fmt_clippy.py clippy
step "cargo test（含金样与重放）" cargo test --workspace --offline -q
for s in deps lines grep_effect_names grep_constants grep_paths grep_fill grep_rules_checker grep_rt_codes trace kill_switch; do
  step "$s" python3 "scripts/$s.py"
done
step "gen_profiles --check（发行画像与 foundation 来源一致，B73）" python3 scripts/gen_profiles.py --check
step "equiv_pairs（等价写法对调用比，只报告，见脚本头注）" python3 scripts/equiv_pairs.py
exit $status
