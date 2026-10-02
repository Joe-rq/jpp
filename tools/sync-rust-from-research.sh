#!/usr/bin/env bash
# 把研究树 `地基/rust-jpp` 某个提交的状态同步到本仓库的 `rust/`。
# Sync the research tree `地基/rust-jpp` at a given commit into this repo's `rust/`.
#
# 用法 / usage:
#   tools/sync-rust-from-research.sh [研究仓库路径] [提交]
#   默认 ~/个人项目/jev 与其 HEAD；只取已提交内容（git archive），不带工作区未提交的改动。
#
# 目录一一对应：研究树 X ↔ 本仓 rust/X。研究树删掉的文件这里也删（rsync --delete），
# 下面三类例外：
#   1. 只在公开侧的文件（KEEP）：保留，不被删除。
#   2. 不公开的文件（SKIP）：不复制。过程记录、黑板、附注不在 rust-jpp 目录里，本来就不会带上；
#      这里列的是 rust-jpp 里面的协作记录、Nature 人工抽检的逐条标注，以及引用研究区私有路径的
#      发行说明草稿（发行时整理进 docs/progress.md），以及依赖研究机远端编译机的 cargoq、合入列车 train.sh
#      和包着 cargoq 的 test-companions-on（公开仓直接用 cargo；开伴随题跑全量设 JPP_TEST_COMPANIONS=on）。
#   3. 可移植改写（PORT）：测试里指向研究工作区 `foundation/` 的路径改到本仓的 `src/foundation/`，
#      读未公开运行目录的测试改读仓库内夹具。改写找不到原文时报错退出，提醒人工核对。
# 跑完后在 rust/ 下执行 `cargo test --locked --workspace`，并用 `git status` 看清改动再提交。
set -euo pipefail

REPO="${1:-$HOME/个人项目/jev}"
REV="${2:-HEAD}"
HERE="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$HERE/rust"

# 2026-09-25（研究树步 14a）：jpp-core 与 jpp-cli 合并改名为 jpp（lib + bin）。
# 只在公开侧的两个文件随之从 crates/jpp-core/tests/ 手动搬到 crates/jpp/tests/（一次性，
# 已在当次同步提交里做完）；这里的 KEEP 路径改指向新位置。
KEEP=(
  /.gitignore
  /target/
  /crates/jpp/tests/fixtures/
  /crates/jpp/tests/known_defects.rs
  /scripts/ci_public.sh
  /scripts/doc_snippets.py
  /examples/purpose-only.jpp
  /examples/purpose-only.args
  /examples/purpose-only/
  /PUBLIC-SNAPSHOT.md
)
SKIP=(
  /COORDINATION.md
  /probes/scope/语义R-带材料.jsonl
  /发行说明-待发布.md
  /scripts/cargoq
  /scripts/cargoq-stale-repro
  /scripts/train.sh
  /scripts/test-companions-on
)

commit="$(git -C "$REPO" rev-parse --verify "$REV^{commit}")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
git -C "$REPO" archive "$commit" 地基/rust-jpp | tar -x -C "$tmp"
src="$tmp/地基/rust-jpp"

# KEEP 用 protect（P）：不删、但研究树若有同名文件仍会覆盖；SKIP 用 exclude：不复制。
args=(-a --delete)
for p in "${KEEP[@]}"; do args+=(--filter="P $p"); done
for p in "${SKIP[@]}"; do args+=(--exclude="$p"); done
rsync "${args[@]}" "$src/" "$DEST/"

python3 - "$DEST" <<'PY'
import pathlib, sys

root = pathlib.Path(sys.argv[1])

def rewrite(rel, old, new, count=None, optional=False):
    p = root / rel
    s = p.read_text(encoding="utf-8")
    n = s.count(old)
    if optional and n == 0:
        return
    if n == 0 or (count is not None and n != count):
        sys.exit(f"PORT 改写找不到原文或次数不符（{rel}：{n} 处）：{old[:60]!r}")
    p.write_text(s.replace(old, new), encoding="utf-8")
    print(f"PORT {rel}：{n} 处")

prof_old = '"../../../foundation/profile/profiles/'
prof_new = '"../../../src/foundation/profile/profiles/'
# 2026-09-25：扫描面从 crates/*/tests/*.rs 放宽到每个 crate 的 src/ 与 tests/ 全树（rglob），
# 因为这条路径也会出现在 src 内嵌单元测试里（crates/jpp-effects/src/profile.rs 曾漏改，报
# 「读不到档案」，cargo test 才发现——见 docs/progress.md 2026-09-25 条目）。
for sub in ("src", "tests"):
    for f in sorted((root / "crates").glob(f"*/{sub}/**/*.rs")):
        if prof_old in f.read_text(encoding="utf-8"):
            rewrite(f.relative_to(root), prof_old, prof_new)
rewrite("crates/jpp/tests/wiring.rs",
        '"../foundation/profile/profiles/', '"../src/foundation/profile/profiles/')
rewrite("crates/jpp/tests/calib_load.rs",
        '''fn 真records() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../foundation/runs/jv/e-cal/calib")
}

/// **内核第一次读得到那三条记录。**
#[test]
fn 读得进e_cal那三条真记录() {
    let store = CalibStore::load(&真records()).expect("三条真记录该读得进来");''',
        '''fn 记录夹具() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/calib_legacy")
}

/// 旧记录格式的可携带回归：保留字段形状，不依赖未发布的研究运行目录。
#[test]
fn 读得进e_cal旧格式记录夹具() {
    let store = CalibStore::load(&记录夹具()).expect("仓库内三条旧格式夹具该读得进来");''', count=1)

# 2026-09-26（步 20a-2b 新增测试自带的一处漏改）：b116_questions_out.rs 的 V7 案例指向研究工作区的
# 评估/2026-09-24-V7固定序/（公开仓库里不存在，cargo test 首次同步即报 No such file or directory）。
# 改读仓库内夹具 tests/fixtures/refund-do.jpp（内容与研究树该文件逐字节相同，已随本次同步一并加入）。
rewrite("crates/jpp/tests/b116_questions_out.rs",
        '''let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../评估/2026-09-24-V7固定序/refund-do.jpp");''',
        '''// 公开仓库副本：原路径指向研究工作区私有目录（评估/2026-09-24-V7固定序/），
    // 不在本仓库里；tools/sync-rust-from-research.sh 把这一行改写成仓库内夹具。
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/refund-do.jpp");''',
        count=1, optional=True)

# 2026-10-01：ablation/plan.rs 的「第二版_题库三条是非题式」读研究区 题库/第二批/C01/calib110（公开仓库没有），
# 缺目录时打印「跳过」换下一条（与 e_alloc.rs 等读未公开数据的测试同一做法）；其余两条读 bank/entries/，照常断言。
rewrite("crates/jpp/tests/ablation/plan.rs",
        '''    for (name, d, want, want_sym) in dirs {
        let s = CalibStore::load(&d).expect("装载");''',
        '''    for (name, d, want, want_sym) in dirs {
        if !d.exists() {
            // 公开仓库没有研究区的 题库/第二批/ 校准目录：跳过这一条（tools/sync-rust-from-research.sh 改写）
            eprintln!("跳过 {name}：校准目录 {} 不在本仓库", d.display());
            continue;
        }
        let s = CalibStore::load(&d).expect("装载");''',
        count=1, optional=True)

# 2026-10-01：公开仓不放内部人名与过程性文字，也不绑研究机的工具链。
# (1) rust/.cargo/config.toml：研究机的 rustc-wrapper = "sccache" 与占盘注释不带过来，公开版只留并发与目录体积两项配置。
(root / ".cargo").mkdir(exist_ok=True)
(root / ".cargo" / "config.toml").write_text(
    "# 限制编译与测试并发，防止多轨并行时整机卡死；单轨或 CI 要更快，用环境变量 CARGO_BUILD_JOBS / RUST_TEST_THREADS 覆盖。\n"
    "# Cap build/test parallelism; override with CARGO_BUILD_JOBS / RUST_TEST_THREADS.\n"
    "[build]\n"
    "jobs = 3\n"
    "\n"
    "# 关增量编译、调试信息只留行号表，让 target/ 小一半；改一行后的重编会慢一些。\n"
    "# Smaller target/: no incremental builds, line tables only; rebuilds after a one-line edit are slower.\n"
    "incremental = false\n"
    "\n[profile.dev]\ndebug = \"line-tables-only\"\n\n[env]\nRUST_TEST_THREADS = \"4\"\n",
    encoding="utf-8")
print("PORT .cargo/config.toml：整份改写")

# (2) 人名与「某某裁定/任务书」这类内部归属改成不带名字的说法，意思保留（optional：研究树改了之后自动跳过）。
#     不改 lib/*.jpp 与 examples/*.jpp 的文本：库与示例的文本进 lib_version 和站点偏移，金样会整批变红；那两处要在研究树里改并重录金样。
for rel, old, new in [
    ("crates/jpp-runtime/src/guard.rs", "（12:649 Nature 裁定：", "（12:649 的裁定："),
    ("crates/jpp/tests/guard.rs", "（`12`:649 Nature 的裁定）", "（`12`:649 的裁定）"),
    ("crates/jpp/INTERFACE.md", "`12`:649 Nature 裁定：", "`12`:649 的裁定："),
    ("crates/jpp/INTERFACE.md", "待 Nature 批准", "待批准"),
    ("crates/jpp/INTERFACE.md", "待第三轮实验与 Nature 裁定", "待第三轮实验与裁定"),
    ("crates/jpp/INTERFACE.md", "B17（Nature 确认进语言：", "B17（已确认进语言："),
    ("crates/jpp/INTERFACE.md", "需要 Nature / 总控裁定", "需要裁定"),
    ("crates/jpp/tests/sieve_declared_line.rs", "主会话 2026-09-26 晚：Nature 定翻转缺省值", "2026-09-26 晚：裁定翻转缺省值"),
    ("scripts/equiv_pairs.py", "（Nature 任务书：", "（任务书："),
    ("scripts/dashboard.py", "Nature 2026-09-24：「有了这个测量以后我们就可以根据结果不断地反馈，不断地修正……按真实的倍率或者真实的水平比较。」",
     "设计取向：有了测量，就按结果不断反馈、不断修正，按真实的倍率或真实的水平比较。"),
    ("scripts/dashboard.py", "（Nature 2026-09-24 确认）", "（2026-09-24 确认）"),
    ("scripts/dashboard.py", "判定档，Nature 2026-09-24 确认按 T1 判", "判定档，2026-09-24 确认按 T1 判"),
    ("probes/measure.toml", "Nature 2026-09-24 决定的维度", "2026-09-24 决定的维度"),
]:
    rewrite(rel, old, new, optional=True)
for f in sorted((root / "probes").glob("*/measure.toml")):
    rewrite(f.relative_to(root), "Nature 2026-09-24 确认验收 1 按 T1 判", "2026-09-24 确认验收 1 按 T1 判", optional=True)

# 2026-10-01：jpp-plan 的 ablation/fission.rs「切点_复现v8六份材料」读研究区 实测/V8-裂变-2026-09-29/材料.json（公开仓库没有），
# 缺文件时打印「跳过」后返回。
rewrite("crates/jpp-plan/tests/ablation/fission.rs",
        '''    let p = root().join("../实测/V8-裂变-2026-09-29/材料.json");
''',
        '''    let p = root().join("../实测/V8-裂变-2026-09-29/材料.json");
    if !p.exists() {
        // 公开仓库没有研究区的 实测/ 目录：跳过（tools/sync-rust-from-research.sh 改写）
        eprintln!("跳过：{} 不在本仓库", p.display());
        return;
    }
''',
        count=1, optional=True)

# 2026-10-02：撤掉 10-01 为 Linux CI 加的 golden.rs 的 action_facts 放宽比较。研究树 Z0901 已把随宿主变的环境事实挪进
# 报告的 host 块，golden.rs 比较时丢掉 host（drop_host），金样不再存这些事实；公开仓与研究树的 golden.rs 与金样相同，不再改写。

# 2026-10-02：plan_preview.py 的报错提示指向研究机的 cargoq（公开仓没有），改成直接用 cargo。
rewrite("scripts/plan_preview.py", "先 地基/rust-jpp/scripts/cargoq build -p jpp --features live", "先 cargo build -p jpp --features live", optional=True)

# 探针脚本与运行记录里的本机绝对路径改成相对路径（不被测试或金样读取；研究树改了之后这两条自动跳过）。
rewrite("probes/scope/rule_gradient.py",
        'ROOT = pathlib.Path("/Users/nature/个人项目/jev")\nRJ = ROOT / "地基/rust-jpp"\nCAL = ROOT / "实测/校准题式-2026-09-23"',
        'RJ = pathlib.Path(__file__).resolve().parents[2]  # rust-jpp\nCAL = RJ.parents[1] / "实测/校准题式-2026-09-23"',
        optional=True)
rewrite("probes/scope/rule_gradient.py", "（路径写死为本仓库）", "（路径相对本文件；实测目录在公开仓库里没有）", optional=True)
rewrite("probes/scope/result.json",
        "/Users/nature/个人项目/jev/.claude/worktrees/agent-a9b7f475a79e897cb/地基/rust-jpp/", "", optional=True)
import re as _re
for f in root.rglob("*"):
    if f.is_file() and "target" not in f.parts and f.suffix in {".py", ".json", ".jsonl", ".toml", ".rs", ".md", ".sh"}:
        if _re.search(r"/Users/[A-Za-z]", f.read_text(encoding="utf-8", errors="ignore")):
            print(f"注意：{f.relative_to(root)} 含本机绝对路径，核对后决定是否改写")

# 兜底检查：人工抽检行（source=human 且带 spot_check）不应出现在任何 jsonl 里。
import json
bad = []
for f in root.rglob("*.jsonl"):
    if "target" in f.parts:
        continue
    for i, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if isinstance(r, dict) and r.get("source") == "human" and "spot_check" in r:
            bad.append(f"{f.relative_to(root)}:{i}")
if bad:
    sys.exit("发现人工抽检逐条标注，先加进 SKIP：" + ", ".join(bad[:10]))
PY

echo "已同步研究树 $commit → rust/"
echo "Synced research-tree commit $commit into rust/"
