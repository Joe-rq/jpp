#!/usr/bin/env bash
# 把研究工作区（地基）的内核源码与研究文档同步到本发布仓库。
# 用法：tools/sync-from-workspace.sh [工作区路径]   默认 ~/个人项目/jev/地基
# 只复制，不改内容；不含私人对话、密钥、原始模型记录、运行输出。
# 唯一会改内容的地方是「公开标记」：工作区敏感行的上一行写一条注释，本脚本复制后替换或删除它。
#   tools/sync-from-workspace.sh --self-test    只跑标记过滤的自测，不碰任何文件
#   tools/sync-from-workspace.sh --filter-only  只对库里已有的 research/ 再跑一遍过滤
set -euo pipefail

# 公开标记过滤。作用于给定目录下所有 .md：
#   <!-- 公开替换：X -->  删掉这行注释，并把紧随其后的一行整行换成 X
#   <!-- 不公开 -->       删掉这行注释和紧随其后的一行
# 两种标记都要独占一行才生效，所以正文里用反引号引用它们不会被误伤。
redact_markers() {
  python3 - "$1" <<'PY'
import pathlib, re, sys

REPLACE = re.compile(r"^[ \t]*<!--[ \t]*公开替换[：:][ \t]?(.*?)[ \t]*-->[ \t]*$")
DROP = re.compile(r"^[ \t]*<!--[ \t]*不公开[ \t]*-->[ \t]*$")


def filter_lines(lines):
    out, i, hits = [], 0, 0
    while i < len(lines):
        m = REPLACE.match(lines[i])
        if m:
            out.append(m.group(1))          # 注释行没了，下一行被换成 X
            i += 2 if i + 1 < len(lines) else 1
            hits += 1
            continue
        if DROP.match(lines[i]):
            i += 2 if i + 1 < len(lines) else 1
            hits += 1
            continue
        out.append(lines[i])
        i += 1
    return out, hits


root, total = pathlib.Path(sys.argv[1]), 0
for f in sorted(root.rglob("*.md")):
    new, hits = filter_lines(f.read_text(encoding="utf-8").split("\n"))
    if hits:
        f.write_text("\n".join(new), encoding="utf-8")
        print(f"公开标记过滤 {f.relative_to(root)}：{hits} 处")
        total += hits
print(f"公开标记过滤合计 {total} 处")
PY
}

# --self-test：用 fixture 证明替换、删除、行内引用不误伤三项都对；不碰工作区和本库
if [ "${1:-}" = "--self-test" ]; then
  T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
  cat > "$T/fixture.md" <<'FIX'
保留的第一行
<!-- 公开替换：- 换上来的公开版本 -->
- 敏感原文，不该出现在公开副本里
<!-- 不公开 -->
- 整条删掉，不该出现在公开副本里
正文里提到 `<!-- 公开替换：X -->` 和 `<!-- 不公开 -->` 时不该被误伤
保留的最后一行
FIX
  cat > "$T/expected" <<'EXP'
保留的第一行
- 换上来的公开版本
正文里提到 `<!-- 公开替换：X -->` 和 `<!-- 不公开 -->` 时不该被误伤
保留的最后一行
EXP
  redact_markers "$T" > /dev/null
  if diff -u "$T/expected" "$T/fixture.md"; then
    echo "--self-test PASS：替换、删除、行内引用不误伤"; exit 0
  else
    echo "--self-test FAIL" >&2; exit 1
  fi
fi

# --filter-only：只对库里已有的 research/ 再跑一遍过滤（手工复制过文件之后用）
if [ "${1:-}" = "--filter-only" ]; then
  redact_markers "$(cd "$(dirname "$0")/.." && pwd)/research"
  exit 0
fi
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

# 4. 公开标记过滤：把工作区标好的敏感行替换掉或删掉（见文件头 redact_markers）。
#    公开副本不要手改——手改的东西下次同步会被上面的 cp/rsync 静默盖回去。
redact_markers "$R"

echo "synced from $WS at $(date +%F)"
( cd "$WS/foundation" && cat jv/*.py | shasum -a 256 | cut -c1-12 | sed 's/^/jv 全包指纹 /' )
