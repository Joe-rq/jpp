"""建造第 4 步：21 条程序（examples/twentyone.py）逐条跑通 + §6.3 六种易错模式的静态拦截率。全部 FakeClient，$0。

拦截率测试的变体由程序化改写生成（不手写）：对每条程序的 AST 注入一种模式，能就地改写就就地改（更像真实误写），
就地改不了才在函数末尾追加一段规范样式的误写（保底）。两类分开计数，因为就地改写才是检查器真正的考题。
`python -m foundation.tests.test_twentyone` 打印拦截率表与融合率表（STATS.md 的数据来源）。
"""

from __future__ import annotations

import ast
import copy
import inspect
import json
import linecache
import os
import re
import sys
import textwrap

import pytest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

import foundation.jv as jv  # noqa: E402
from foundation.jv.checker import check  # noqa: E402
from foundation.jv.examples import twentyone as T  # noqa: E402

IDS = [p.__name__ for p in T.PROGRAMS]


# ================================================================ 1. 逐条跑通
@pytest.mark.parametrize("prog", T.PROGRAMS, ids=IDS)
def test_program_runs_under_fake_client(prog):
    rows = T.run_all(programs=[prog])
    assert rows[0]["状态"] == "ok", rows[0]
    assert rows[0]["层数"] >= 1


@pytest.mark.parametrize("prog", T.PROGRAMS, ids=IDS)
def test_program_static_check_clean(prog):
    rep = check(prog.__jv_fn__)
    assert rep.errors == [], rep.errors


def test_twentyone_count():
    assert len(T.PROGRAMS) == 21
    assert len({p.__name__ for p in T.PROGRAMS}) == 21


# ================================================================ 2. §6.3 六种模式的程序化注入
EXIT_KINDS = ("Act", "Ignore", "Unsure", "Pick", "At")
PATTERNS = {
    1: ("judge 后直接 match", ("J-01",)),
    2: ("e == jv.Act", ("W-cmp-type",)),
    3: ("r1 > r2", ("J-01",)),
    4: ("循环里常量序号", ("J-13", "W-seq-const")),
    5: ("calib 传字符串", ("J-03",)),
    6: ("循环内串行依赖", ("W-serial",)),
}


def _is_jv(node, name=None):
    if isinstance(node, ast.Call):
        node = node.func
    return isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id == "jv" \
        and (name is None or node.attr == name)


def _kw(call, name):
    for k in call.keywords:
        if k.arg == name:
            return k
    return None


def _fn_tree(prog) -> ast.Module:
    src = textwrap.dedent(inspect.getsource(prog.__jv_fn__))
    tree = ast.parse(src)
    fdef = tree.body[0]
    fdef.decorator_list = []
    return tree


def _walk_with_loops(node, depth=0):
    """(node, 循环深度) 的先序遍历。"""
    yield node, depth
    d = depth + (1 if isinstance(node, (ast.For, ast.While, ast.ListComp, ast.SetComp, ast.GeneratorExp, ast.DictComp)) else 0)
    for ch in ast.iter_child_nodes(node):
        yield from _walk_with_loops(ch, d)


def _replace(tree, old, new):
    class R(ast.NodeTransformer):
        def visit(self, n):
            if n is old:
                return new
            return self.generic_visit(n)
    return R().visit(tree)


def _first(tree, pred):
    for n in ast.walk(tree):
        if pred(n):
            return n
    return None


def _append(fdef, snippet: str):
    fdef.body.extend(ast.parse(textwrap.dedent(snippet)).body)


def _first_judge_src(tree) -> str:
    j = _first(tree, lambda n: isinstance(n, ast.Call) and _is_jv(n, "judge"))
    return ast.unparse(j) if j is not None else 'jv.judge(jv.state(on=jv.lit("x")), jv.test("q", calib=jv.calib("k")))'


def _first_action_src(tree) -> str:
    d = _first(tree, lambda n: isinstance(n, ast.Call) and _is_jv(n, "do") and n.args)
    return ast.unparse(d.args[0]) if d is not None else "run"


def mutate(prog, pattern: int) -> tuple[ast.Module, str]:
    """返回 (改写后的树, 'inplace' | 'append')。"""
    tree = _fn_tree(prog)
    fdef = tree.body[0]
    if pattern == 1:
        cut = _first(tree, lambda n: isinstance(n, ast.Call) and _is_jv(n, "cut") and n.args and (
            _is_jv(n.args[0], "judge") or (isinstance(n.args[0], ast.Subscript) and _is_jv(n.args[0].value, "judge"))))
        if cut is not None:
            inner = cut.args[0]
            judge = inner.value if isinstance(inner, ast.Subscript) else inner
            _replace(tree, cut, judge)
            return tree, "inplace"
        _append(fdef, f"_r = {_first_judge_src(tree)}\nmatch _r:\n    case jv.Act():\n        pass\n")
        return tree, "append"
    if pattern == 2:
        isi = _first(tree, lambda n: isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "isinstance"
                     and len(n.args) == 2 and _is_jv(n.args[1]) and n.args[1].attr in EXIT_KINDS)
        if isi is not None:
            _replace(tree, isi, ast.Compare(left=isi.args[0], ops=[ast.Eq()], comparators=[isi.args[1]]))
            return tree, "inplace"
        asg = _first(tree, lambda n: isinstance(n, ast.Assign) and isinstance(n.targets[0], ast.Name) and _is_jv(n.value, "cut"))
        if asg is not None:
            _append(fdef, f"if {asg.targets[0].id} == jv.Act:\n    pass\n")
            return tree, "append"
        _append(fdef, f"_e = jv.cut({_first_judge_src(tree)})\nif _e == jv.Act:\n    pass\n")
        return tree, "append"
    if pattern == 3:
        asg = _first(tree, lambda n: isinstance(n, ast.Assign) and isinstance(n.targets[0], ast.Name) and _is_jv(n.value, "judge"))
        if asg is not None:
            _append(fdef, f"if {asg.targets[0].id}[0] > {asg.targets[0].id}[0]:\n    pass\n")
            return tree, "inplace"                         # 用程序自己的读数变量比较
        _append(fdef, f"_r = {_first_judge_src(tree)}\nif _r[0] > _r[0]:\n    pass\n")
        return tree, "append"
    if pattern == 4:
        for n, depth in _walk_with_loops(fdef):
            if depth > 0 and isinstance(n, ast.Call) and (_is_jv(n, "do") or _is_jv(n, "gen")):
                k = _kw(n, "iter_seq" if _is_jv(n, "do") else "retry_seq")
                if k is not None and not isinstance(k.value, ast.Constant):
                    k.value = ast.Constant(0)
                    return tree, "inplace"
        d = _first(tree, lambda n: isinstance(n, ast.Call) and _is_jv(n, "do"))
        if d is not None:
            d2 = copy.deepcopy(d)
            k = _kw(d2, "iter_seq")
            if k is None:
                d2.keywords.append(ast.keyword(arg="iter_seq", value=ast.Constant(0)))
            else:
                k.value = ast.Constant(0)
            _append(fdef, f"for _k in range(2):\n    {ast.unparse(d2)}\n")
        else:
            _append(fdef, "for _k in range(2):\n    jv.gen('x', ctx=[], n=1, retry_seq=0)\n")
        return tree, "append"
    if pattern == 5:
        q = _first(tree, lambda n: isinstance(n, ast.Call) and _is_jv(n) and n.func.attr in ("test", "select", "measure")
                   and _kw(n, "calib") is not None and _is_jv(_kw(n, "calib").value, "calib"))
        if q is not None:
            inner = _kw(q, "calib").value
            _kw(q, "calib").value = inner.args[0] if inner.args and isinstance(inner.args[0], ast.Constant) else ast.Constant("k")
            return tree, "inplace"
        _append(fdef, 'jv.cut(jv.judge(jv.state(on=jv.lit("x")), jv.test("q", calib=jv.calib("k")))[0], calib="k")\n')
        return tree, "append"
    if pattern == 6:
        act = _first_action_src(tree)
        loop = _first(fdef, lambda n: isinstance(n, ast.For) and isinstance(n.target, ast.Name))
        if loop is not None:
            stmt = ast.parse(f"_x = jv.do({act}, _x, iter_seq={loop.target.id})").body[0]
            loop.body.insert(0, stmt)
            return tree, "inplace"
        _append(fdef, f"_x = jv.lit('x')\nfor _c in range(2):\n    _x = jv.do({act}, _x, iter_seq=_c)\n")
        return tree, "append"
    raise ValueError(pattern)


def build(tree: ast.Module, name: str):
    """把改写后的树变回可 inspect.getsource 的函数（源码登记进 linecache）。"""
    ast.fix_missing_locations(tree)
    src = ast.unparse(tree) + "\n"
    filename = f"<mutant:{name}>"
    linecache.cache[filename] = (len(src), None, src.splitlines(True), filename)
    ns = dict(vars(T))
    exec(compile(src, filename, "exec"), ns)
    return ns[tree.body[0].name]


_LINE = re.compile(r"（行 \d+）")


def _norm(msgs):
    return {_LINE.sub("", m) for m in msgs}


def interception(prog, pattern: int) -> dict:
    base = _norm(check(prog.__jv_fn__).errors + check(prog.__jv_fn__).warnings)
    tree, how = mutate(prog, pattern)
    fn = build(tree, f"{prog.__name__}_p{pattern}")
    rep = check(fn)
    new = [m for m in rep.errors + rep.warnings if _LINE.sub("", m) not in base]
    want = PATTERNS[pattern][1]
    hit = [m for m in new if any(w in m for w in want)]
    return {"程序": prog.__name__, "模式": pattern, "方式": how, "拦住": bool(hit), "带修法": any("修法" in m for m in hit),
            "报文": hit[:1], "其它新报文": [m for m in new if m not in hit][:2]}


RESULTS: list[dict] = []


@pytest.mark.parametrize("pattern", sorted(PATTERNS), ids=[f"p{k}" for k in sorted(PATTERNS)])
@pytest.mark.parametrize("prog", T.PROGRAMS, ids=IDS)
def test_j17_interception(prog, pattern):
    r = interception(prog, pattern)
    RESULTS.append(r)
    if r["方式"] == "append":                 # 保底变体是规范 §6.3 原样写法，必须拦住且带修法
        assert r["拦住"] and r["带修法"], r
    # 就地改写只记录不断言：拦不住的按模式归类写进 STATS.md


def summary_table() -> list[dict]:
    rows = []
    for k in sorted(PATTERNS):
        rs = [interception(p, k) for p in T.PROGRAMS]
        rows.append({"模式": k, "名": PATTERNS[k][0],
                     "就地": sum(r["方式"] == "inplace" for r in rs),
                     "就地拦住": sum(r["方式"] == "inplace" and r["拦住"] for r in rs),
                     "保底": sum(r["方式"] == "append" for r in rs),
                     "保底拦住": sum(r["方式"] == "append" and r["拦住"] for r in rs),
                     "带修法": sum(r["带修法"] for r in rs),
                     "漏": [r["程序"] for r in rs if not r["拦住"]]})
    return rows


def line_counts() -> list[tuple[str, int]]:
    out = []
    for p in T.PROGRAMS:
        src = textwrap.dedent(inspect.getsource(p.__jv_fn__)).splitlines()
        body = [ln for ln in src[1:] if ln.strip() and not ln.strip().startswith('"""')]
        out.append((p.__name__, len(body)))
    return out


if __name__ == "__main__":
    print("== 拦截率")
    for r in summary_table():
        print(json.dumps(r, ensure_ascii=False))
    print("== 漏网明细")
    for k in sorted(PATTERNS):
        for p in T.PROGRAMS:
            r = interception(p, k)
            if not r["拦住"]:
                print(json.dumps(r, ensure_ascii=False))
    print("== 行数")
    lc = line_counts()
    for n, c in lc:
        print(n, c)
    print("合计", sum(c for _, c in lc))
    print("== 融合率")
    for r in T.run_all():
        print(json.dumps({k: r[k] for k in ("程序", "状态", "层数", "每层题数", "每层调用", "调用", "题")}, ensure_ascii=False))
