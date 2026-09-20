#!/usr/bin/env bash
# 把研究工作区（地基）的内核源码与研究文档同步到本发布仓库。
# 用法：tools/sync-from-workspace.sh [工作区路径]   默认 ~/个人项目/jev/地基
# 只复制，不改内容；不含私人对话、密钥、原始模型记录、运行输出。
set -euo pipefail
WS="${1:-$HOME/个人项目/jev/地基}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
RS="rsync -a --exclude __pycache__ --exclude '*.pyc' --exclude .pytest_cache --exclude .DS_Store"

# 1. 内核源码（保持 foundation.jv 兼容名）
$RS --delete "$WS/foundation/jv/"       "$REPO/src/foundation/jv/"
$RS          "$WS/foundation/core/"     "$REPO/src/foundation/core/"
$RS          "$WS/foundation/clients/"  "$REPO/src/foundation/clients/"
$RS          "$WS/foundation/profile/"  "$REPO/src/foundation/profile/"
cp "$WS/foundation/__init__.py" "$REPO/src/foundation/__init__.py" 2>/dev/null || true

# 2. 内核测试（只取自足的 jv 测试；core 旧测试依赖工作区试验台，不同步）
mkdir -p "$REPO/tests/foundation_jv"
rm -f "$REPO/tests/foundation_jv"/test_*.py
for f in "$WS"/foundation/tests/test_jv_*.py "$WS"/foundation/tests/test_twentyone.py "$WS"/foundation/tests/test_probes.py; do
  cp "$f" "$REPO/tests/foundation_jv/"
done
touch "$REPO/tests/foundation_jv/__init__.py"
# 契约测试按工作区目录结构找 README，这里改成发布仓库的路径
sed -i '' 's|os.path.dirname(__file__), "..", "jv", "README.md"|os.path.dirname(__file__), "..", "..", "src", "foundation", "jv", "README.md"|' "$REPO/tests/foundation_jv/test_jv_contract.py"

# 2b. 组合库（Codex 在工作区维护，随内核一起同步，避免发布副本与内核脱节）
python3 "$REPO/tools/sync-composition.py" "$WS"

# 3. 研究文档（整理副本；依据文本修改权在 Nature）
R="$REPO/research"
mkdir -p "$R/地基" "$R/地基/foundation/experiments" "$R/扩展/codex_composition"
for f in "$WS"/*.md; do
  b="$(basename "$f")"
  case "$b" in Nature原话全集.md|自检-当前状态.md) continue;; esac
  cp "$f" "$R/地基/$b"
done
# 附注（代理间协作消息与执行计划）不公开，由 Nature 另挑
for d in 设计 红队 研究 语言; do
  [ -d "$WS/$d" ] && $RS --delete --include '*/' --include '*.md' --exclude '*' "$WS/$d/" "$R/地基/$d/"
done
for f in EXPERIMENTS.md 前提结论.md E9f-设计.md; do
  [ -f "$WS/foundation/experiments/$f" ] && cp "$WS/foundation/experiments/$f" "$R/地基/foundation/experiments/$f"
done
cp "$WS/foundation/profile/SCHEMA.md" "$R/地基/foundation/profile-SCHEMA.md" 2>/dev/null || true
for f in README.md RESULTS.md; do cp "$WS/扩展/codex_composition/$f" "$R/扩展/codex_composition/"; done

# 3b. 公开前脱敏复核。DECISIONS.md 有条目在公开副本里做过人工脱敏——某些授权的范围是
#     「可送模型 API」，不等于「可发公开仓库」。上面的 rsync 会把工作区原文原样盖回来，
#     所以每次同步后都要人工复核这一段再 commit。已脱敏的段落在公开副本里带
#     〔公开副本脱敏：…〕标记；标记消失就说明被盖掉了，按上一版重做。不在此处列敏感词，
#     因为这个文件本身是公开的。
if ! grep -q '公开副本脱敏' "$R/地基/DECISIONS.md"; then
  echo "提醒：research/地基/DECISIONS.md 的人工脱敏标记不见了，可能已被工作区原文覆盖；commit 前请对照上一版重做脱敏" >&2
fi

echo "synced from $WS at $(date +%F)"
( cd "$WS/foundation" && cat jv/*.py | shasum -a 256 | cut -c1-12 | sed 's/^/jv 全包指纹 /' )
