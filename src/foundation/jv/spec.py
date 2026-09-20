"""推测提升与循环向量化（§4 pass 1「提升」的扩展；README §3.1）。

目的（P5 状态收费、题免费；P3 批无偏）：同一份材料上的题应一次问完，互不依赖的判断应同层并发。
惰性执行下 `cut` / `match` 是刷新点，把本该同层的判断隔成多层。这里在刷新点做两件事，都**只推测 judge**
（judge 无世界效应、题几乎免费，推错不需回滚；红队 06 A1 删掉的是跨分支推测 do / gen）：

1. **推测提升**：从触发刷新的语句起，沿直线段向前（含 if / match 分支体）找后面的 `jv.judge` 站点，
   在当前帧局部变量的快照上求值它的状态与题——只允许纯表达式（名字、常量、容器与推导式、纯内置函数、
   `jv.state / lit / transform / gen / test / select / measure / calib`），就绪的登记进本层；
   真站点到达时按（状态哈希, 题）命中已推测的效应，不再发调用、不再多一层。
2. **循环向量化**：触发点在宿主 `for` 循环体内、循环体无 loop-carried 依赖（后面写的名字不是前面读的名字）、
   无 do / ask / return / break 时，对可迭代对象的其余元素逐个绑定循环变量、在沙盒里重放体内前置语句
   （守卫 `if …: continue`、赋值、gen / transform），把各自的 judge 登记进本层。

do / ask 永远不推测：它们触世界或触人。推测求值只许零成本零副作用：gen 花钱且一登记就发，transform 跑宿主代码，
所以含 gen / transform 的站点（或它们的前置赋值）只在**无条件直线可达**时提前执行（反正一定会执行；真站点到达时命中效应账本）；
站点在分支体或嵌套循环体内、能不能走到还不知道 → 不推测，`stats["spec"]["skipped"]` 记 "gen-in-branch"。
被向量化的循环其余轮次算无条件（前提已核：无 break / return，守卫 `if …: continue` 在沙盒里按值求）。
求值失败、名字被中间语句改写、状态含未解析期物，都只是「不推测」，从不改变程序语义。
"""

from __future__ import annotations

import ast
import builtins
import inspect
import os
import textwrap
from dataclasses import dataclass, field
from typing import Any

from .ir import Exit, Q, State, _is

_SAFE_BUILTINS = {n: getattr(builtins, n) for n in (
    "len", "list", "sorted", "str", "int", "float", "min", "max", "range", "enumerate", "zip", "dict", "set",
    "tuple", "abs", "sum", "any", "all", "reversed", "bool", "repr", "round", "frozenset")}
_PURE_METHODS = {"content", "text", "get", "items", "keys", "values", "split", "strip", "lower", "upper",
                 "startswith", "endswith", "join", "format", "replace", "count", "index", "hash", "tokens",
                 "as_mat", "kind", "splitlines", "isdigit", "encode", "decode"}
_MUTATORS = {"add", "append", "extend", "update", "pop", "remove", "insert", "clear", "discard", "setdefault",
             "sort", "reverse", "popitem"}
_JV_PURE = {"state", "lit", "mat", "transform", "gen", "test", "select", "measure", "calib", "anchors", "fitref"}
_JV_COSTLY = {"gen", "transform"}          # 非零成本：只在无条件可达时提前执行
_RE_ITERABLE = (list, tuple, range, dict, set, frozenset, str)


class _Unsafe(Exception):
    pass


def _pure_isinstance(obj, cls):
    """沙盒里的 isinstance：对出口不经 __instancecheck__（不消费、不动 _last_matched）。"""
    if isinstance(cls, tuple):
        return any(_pure_isinstance(obj, c) for c in cls)
    if isinstance(cls, type) and issubclass(cls, Exit):
        return _is(obj, cls)
    return isinstance(obj, cls)


_BUILTINS = dict(_SAFE_BUILTINS, isinstance=_pure_isinstance)


# ---------------------------------------------------------------- 表达式：纯性检查与求值
def _check_expr(e: ast.AST) -> None:
    for n in ast.walk(e):
        if isinstance(n, (ast.Lambda, ast.Await, ast.Yield, ast.YieldFrom, ast.NamedExpr)):
            raise _Unsafe(type(n).__name__)
        if isinstance(n, ast.Call):
            f = n.func
            if isinstance(f, ast.Name):
                if f.id not in _BUILTINS:
                    raise _Unsafe(f"call {f.id}")
            elif isinstance(f, ast.Attribute):
                if isinstance(f.value, ast.Name) and f.value.id == "jv":
                    if f.attr not in _JV_PURE:
                        raise _Unsafe(f"jv.{f.attr}")
                elif f.attr not in _PURE_METHODS:
                    raise _Unsafe(f"method {f.attr}")
            else:
                raise _Unsafe("call")
        if isinstance(n, ast.Attribute) and n.attr.startswith("_"):
            raise _Unsafe("private attr")


def _safe(e: ast.AST) -> bool:
    try:
        _check_expr(e)
        return True
    except _Unsafe:
        return False


_CODE_CACHE: dict = {}


def _eval(e: ast.AST, ns: dict, g: dict, filename: str):
    """在「全局 + 局部快照」合并的命名空间里求值（推导式才看得见局部名）。文件名与行号保留：
    沙盒里执行的 jv.gen / jv.transform 的站点（site_of）与真站点相同，账本键才能命中。"""
    key = id(e)
    code = _CODE_CACHE.get(key)
    if code is None or code[0] is not e:
        expr = ast.Expression(body=e)
        ast.fix_missing_locations(expr)
        code = (e, compile(expr, filename, "eval"))
        _CODE_CACHE[key] = code
    env = dict(g)
    env.update(ns)
    env["__builtins__"] = _BUILTINS
    return eval(code[1], env)


def _names(node: ast.AST) -> set[str]:
    return {n.id for n in ast.walk(node) if isinstance(n, ast.Name)}


def _stores(node: ast.AST) -> set[str]:
    """语句里写到的名字：赋值目标、for 目标、with 目标、match 捕获、下标赋值与可变方法调用的对象。"""
    out: set[str] = set()
    for n in ast.walk(node):
        if isinstance(n, ast.Name) and isinstance(n.ctx, (ast.Store, ast.Del)):
            out.add(n.id)
        elif isinstance(n, ast.MatchAs) and n.name:
            out.add(n.name)
        elif isinstance(n, ast.MatchStar) and n.name:
            out.add(n.name)
        elif isinstance(n, ast.MatchMapping) and n.rest:
            out.add(n.rest)
        elif isinstance(n, ast.Subscript) and isinstance(n.ctx, (ast.Store, ast.Del)):
            out |= _names(n.value)
        elif isinstance(n, ast.Attribute) and isinstance(n.ctx, (ast.Store, ast.Del)):
            out |= _names(n.value)
        elif isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and n.func.attr in _MUTATORS:
            out |= _names(n.func.value)
        elif isinstance(n, ast.AugAssign):
            out |= _names(n.target)
    return out


def _is_jv_call(node: ast.AST, name: str) -> bool:
    return (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name) and node.func.value.id == "jv" and node.func.attr == name)


def _has_jv_call(node: ast.AST, names: set[str]) -> bool:
    return any(isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and isinstance(n.func.value, ast.Name)
               and n.func.value.id == "jv" and n.func.attr in names for n in ast.walk(node))


def _judge_calls_in_expr(node: ast.AST) -> list[ast.Call]:
    """表达式（不含嵌套语句块）里的 jv.judge 调用；不进 lambda / 推导式 / 嵌套 def。"""
    out: list[ast.Call] = []

    def walk(n: ast.AST):
        if isinstance(n, (ast.Lambda, ast.ListComp, ast.SetComp, ast.DictComp, ast.GeneratorExp,
                          ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            return
        if _is_jv_call(n, "judge"):
            out.append(n)
        for c in ast.iter_child_nodes(n):
            if isinstance(c, ast.stmt):
                continue                                  # 嵌套语句块由 _walk_block 走
            walk(c)

    walk(node)
    return out


def _stmt_exprs(s: ast.stmt) -> list[ast.AST]:
    """一条语句「本层」的表达式（不含其分支体）。"""
    if isinstance(s, ast.If):
        return [s.test]
    if isinstance(s, ast.Match):
        return [s.subject]
    if isinstance(s, (ast.Assign, ast.Expr)):
        return [s.value]
    if isinstance(s, (ast.AnnAssign, ast.AugAssign)) and s.value is not None:
        return [s.value]
    if isinstance(s, ast.Return) and s.value is not None:
        return [s.value]
    if isinstance(s, (ast.For, ast.While)):
        return [s.iter] if isinstance(s, ast.For) else [s.test]
    return []


# ---------------------------------------------------------------- 程序的语句索引
@dataclass
class _StmtInfo:
    node: ast.stmt
    block: list                     # 所在语句列表
    index: int
    loops: list                     # 外层 for 链（内层在后）


@dataclass
class SpecPlan:
    fn: Any
    tree: ast.AST
    filename: str
    stmts: list = field(default_factory=list)
    loop_ok: dict = field(default_factory=dict)          # id(For) → (bool, reason)

    @classmethod
    def for_function(cls, fn) -> "SpecPlan | None":
        cached = getattr(fn, "__jv_spec__", None)
        if cached is not None:
            return cached if cached is not False else None
        try:
            lines, start = inspect.getsourcelines(fn)
            tree = ast.parse(textwrap.dedent("".join(lines)))
            ast.increment_lineno(tree, start - 1)
        except (OSError, TypeError, SyntaxError):
            try:
                fn.__jv_spec__ = False
            except (AttributeError, TypeError):
                pass
            return None
        fdef = next((n for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))), None)
        plan = cls(fn=fn, tree=tree, filename=fn.__code__.co_filename)
        if fdef is not None:
            plan._index(fdef.body, [])
        try:
            fn.__jv_spec__ = plan
        except (AttributeError, TypeError):
            pass
        return plan

    def _index(self, block: list, loops: list):
        for i, s in enumerate(block):
            self.stmts.append(_StmtInfo(s, block, i, list(loops)))
            if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef, ast.Lambda)):
                continue
            inner = loops + [s] if isinstance(s, ast.For) else loops
            for attr in ("body", "orelse", "finalbody", "handlers", "cases"):
                sub = getattr(s, attr, None)
                if not sub:
                    continue
                if all(isinstance(x, ast.stmt) for x in sub):
                    self._index(sub, inner)
                else:                                     # match 的 cases / try 的 handlers：各自有 body
                    for c in sub:
                        b = getattr(c, "body", None)
                        if b:
                            self._index(b, inner)

    def locate(self, lineno: int) -> "_StmtInfo | None":
        """含该行的最内层语句。"""
        best = None
        for info in self.stmts:
            s = info.node
            if s.lineno <= lineno <= (getattr(s, "end_lineno", None) or s.lineno):
                if best is None or (s.lineno, -(s.end_lineno or s.lineno)) >= (best.node.lineno, -(best.node.end_lineno or best.node.lineno)):
                    best = info
        return best

    def site(self, node: ast.AST) -> str:
        return f"{os.path.basename(self.filename)}:{node.lineno}"


# ---------------------------------------------------------------- 走语句块
@dataclass
class _Ctx:
    rt: Any
    plan: SpecPlan
    g: dict
    from_site: str
    kind: str                        # "lift" | "vectorize"
    registered: int = 0
    aborted: int = 0


def _bind(target: ast.AST, val, ns: dict) -> bool:
    if isinstance(target, ast.Name):
        ns[target.id] = val
        return True
    if isinstance(target, (ast.Tuple, ast.List)):
        try:
            vals = list(val)
        except TypeError:
            return False
        if len(vals) != len(target.elts) or any(isinstance(t, ast.Starred) for t in target.elts):
            return False
        return all(_bind(t, v, ns) for t, v in zip(target.elts, vals))
    return False


def _site_states_qs(call: ast.Call, ns: dict, ctx: _Ctx):
    if len(call.args) < 2 or call.keywords:
        return None
    for a in call.args:
        _check_expr(a)
    st = _eval(call.args[0], ns, ctx.g, ctx.plan.filename)
    qs = [_eval(a, ns, ctx.g, ctx.plan.filename) for a in call.args[1:]]
    states = list(st) if isinstance(st, (list, tuple)) else [st]
    if not states or not all(isinstance(s, State) for s in states) or not all(isinstance(q, Q) for q in qs):
        return None
    return (st if isinstance(st, (list, tuple)) else states[0]), qs


def _try_sites(s: ast.stmt, ns: dict, tainted: set, ctx: _Ctx, uncond: bool) -> dict:
    """求值并登记一条语句本层表达式里的 judge 站点。返回 {id(call): 效应}。
    `uncond`：该语句相对触发点无条件直线可达；否则含 gen / transform 的站点不推测（零成本零副作用原则）。"""
    out: dict = {}
    for e in _stmt_exprs(s):
        for call in _judge_calls_in_expr(e):
            if _names(call) & tainted:
                continue
            if not uncond and _has_jv_call(call, _JV_COSTLY):
                ctx.rt._note_spec("gen-in-branch")
                continue
            try:
                got = _site_states_qs(call, ns, ctx)
            except Exception:
                ctx.aborted += 1
                continue
            if got is None:
                continue
            st, qs = got
            eff = ctx.rt._register_spec(st, qs, ctx.plan.site(call), ctx.from_site, ctx.kind)
            if eff is not None:
                out[id(call)] = eff
                ctx.registered += 1
    return out


def _walk_block(stmts: list, start: int, ns: dict, tainted: set, ctx: _Ctx, in_progress: bool,
                uncond: bool = True) -> bool:
    """从 stmts[start] 向前走，登记能求值的 judge 站点。返回 False 表示遇到终止语句（return / break / …）。
    `in_progress`：stmts[start] 正在执行（触发刷新的语句），只抽站点、不绑定它的赋值。
    `uncond`：本块相对触发点无条件直线可达（分支体 / match 体为 False）：含 gen / transform 的表达式只在 True 时提前执行。"""
    for i in range(start, len(stmts)):
        s = stmts[i]
        first = in_progress and i == start
        if isinstance(s, (ast.Return, ast.Raise, ast.Break, ast.Continue)):
            return False
        if isinstance(s, (ast.Pass, ast.Assert, ast.Global, ast.Nonlocal, ast.Import, ast.ImportFrom)):
            continue
        effs = _try_sites(s, ns, tainted, ctx, uncond)
        if isinstance(s, ast.Assign):
            names = _stores(s)
            if first:
                tainted |= names
                continue
            v = s.value
            if len(s.targets) == 1 and not (names & tainted):
                if _is_jv_call(v, "judge") and id(v) in effs:
                    if not _bind(s.targets[0], effs[id(v)].readings, ns):
                        tainted |= names
                    continue
                if not uncond and _has_jv_call(v, _JV_COSTLY):
                    ctx.rt._note_spec("gen-in-branch")
                elif _safe(v):
                    try:
                        if _bind(s.targets[0], _eval(v, ns, ctx.g, ctx.plan.filename), ns):
                            continue
                    except Exception:
                        ctx.aborted += 1
            tainted |= names
            for n in names:
                ns.pop(n, None)
            continue
        if isinstance(s, (ast.AugAssign, ast.AnnAssign, ast.Delete)):
            tainted |= _stores(s)
            continue
        if isinstance(s, ast.Expr):
            m = _stores(s)                                   # x.add(...) 这类可变方法调用
            tainted |= m
            for n in m:
                ns.pop(n, None)
            continue
        if isinstance(s, ast.If):
            body_only = s.body and not s.orelse and len(s.body) == 1
            if body_only and isinstance(s.body[0], ast.Continue) and _safe(s.test) and not (_names(s.test) & tainted):
                try:
                    if _eval(s.test, ns, ctx.g, ctx.plan.filename):
                        return False                         # 守卫命中：本轮到此为止
                    continue
                except Exception:
                    ctx.aborted += 1
                    return False
            if body_only and isinstance(s.body[0], (ast.Break, ast.Return, ast.Raise)) and _safe(s.test) \
                    and not (_names(s.test) & tainted):
                try:
                    if _eval(s.test, ns, ctx.g, ctx.plan.filename):
                        return False
                    continue
                except Exception:
                    ctx.aborted += 1
                    return False
            for branch in (s.body, s.orelse):
                if branch:
                    _walk_block(branch, 0, dict(ns), set(tainted), ctx, False, uncond=False)
            w = _stores(s)
            tainted |= w
            for n in w:
                ns.pop(n, None)
            continue
        if isinstance(s, ast.Match):
            for case in s.cases:
                _walk_block(case.body, 0, dict(ns), set(tainted), ctx, False, uncond=False)
            w = _stores(s)
            tainted |= w
            for n in w:
                ns.pop(n, None)
            continue
        # for / while / with / try / def …：不进去；它写到的名字视为改写
        w = _stores(s)
        tainted |= w
        for n in w:
            ns.pop(n, None)
    return True


# ---------------------------------------------------------------- 循环向量化的静态前提
def _loop_static_ok(loop: ast.For, plan: SpecPlan) -> tuple[bool, str]:
    cached = plan.loop_ok.get(id(loop))
    if cached is not None:
        return cached
    res = _loop_static(loop)
    plan.loop_ok[id(loop)] = res
    return res


def _loop_static(loop: ast.For) -> tuple[bool, str]:
    if loop.orelse:
        return False, "for 带 else"
    if _is_jv_call(loop.iter, "loop"):
        return False, "jv.loop 的轮次由变式决定"
    if not _safe(loop.iter):
        return False, "可迭代表达式不纯"
    for n in ast.walk(loop):
        if n is loop:
            continue
        if isinstance(n, (ast.Return, ast.Break)):
            return False, "体内有 return / break"
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.Lambda, ast.ClassDef, ast.Await, ast.Yield)):
            return False, "体内有嵌套定义"
    if _has_jv_call(loop, {"do", "ask", "loop", "answer"}):
        return False, "体内有 do / ask"
    targets = _stores(loop.target)
    # 前置段 = 体内从头到含第一个 judge 站点的语句（含它本层表达式）；前置段读的名字不能被体内任何语句改写，
    # 除了循环变量与前置段自己先写后读的临时名。
    pre: list[ast.stmt] = []
    for s in loop.body:
        pre.append(s)
        if any(_judge_calls_in_expr(e) for e in _stmt_exprs(s)):
            break
    else:
        return False, "体内没有 judge 站点"
    reads: set[str] = set()
    temps: set[str] = set()
    for s in pre:
        loads = {n.id for e in _stmt_exprs(s) for n in ast.walk(e) if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Load)}
        if isinstance(s, (ast.If, ast.Match)):
            pass                                         # 分支体的读写在下面按整体写集处理
        reads |= loads - temps
        if isinstance(s, ast.Assign):
            for n in _stores(s):
                if n not in reads:
                    temps.add(n)
    writes: set[str] = set()
    for s in loop.body:
        writes |= _stores(s)
    carried = (reads & writes) - targets - temps
    if carried:
        return False, f"loop-carried：{sorted(carried)}"
    return True, ""


def _vectorize(loop: ast.For, ns0: dict, ctx: _Ctx) -> int:
    ok, why = _loop_static_ok(loop, ctx.plan)
    if not ok:
        ctx.rt._note_spec(f"vectorize-skip:{why}")
        return 0
    try:
        items = _eval(loop.iter, ns0, ctx.g, ctx.plan.filename)
    except Exception:
        ctx.aborted += 1
        return 0
    if not isinstance(items, _RE_ITERABLE) and type(items).__name__ not in ("zip", "enumerate", "map"):
        return 0
    try:
        items = list(items)
    except TypeError:
        return 0
    targets = sorted(_stores(loop.target))
    cur = {n: ns0.get(n, _MISSING) for n in targets}
    start = 0
    for i, item in enumerate(items):                      # 当前这一圈：循环变量当前的值
        probe: dict = {}
        if not _bind(loop.target, item, probe):
            return 0
        if all(_same(probe.get(n), cur[n]) for n in targets):
            start = i + 1
            break
    n0 = ctx.registered
    for item in items[start:]:
        ns = dict(ns0)
        if not _bind(loop.target, item, ns):
            break
        _walk_block(loop.body, 0, ns, set(), ctx, False)
    return ctx.registered - n0


_MISSING = object()


def _same(a, b) -> bool:
    if a is b:
        return True
    if b is _MISSING:
        return False
    try:
        return bool(a == b) if not _is(a, Exit) else False
    except Exception:
        return False


# ---------------------------------------------------------------- 入口
def speculate(rt, frame_obj, prog_fn, do_lift: bool, do_vectorize: bool) -> dict:
    """在一次 cut 触发的刷新里推测。返回统计。"""
    plan = SpecPlan.for_function(prog_fn)
    if plan is None:
        return {}
    info = plan.locate(frame_obj.f_lineno)
    if info is None:
        return {}
    g = frame_obj.f_globals
    ns0 = dict(frame_obj.f_locals)
    from_site = plan.site(info.node)
    stats = {"lift": 0, "vectorize": 0, "aborted": 0}
    if do_lift:
        ctx = _Ctx(rt=rt, plan=plan, g=g, from_site=from_site, kind="lift")
        _walk_block(info.block, info.index, dict(ns0), set(), ctx, True)
        stats["lift"] = ctx.registered
        stats["aborted"] += ctx.aborted
    if do_vectorize and info.loops:
        ctx = _Ctx(rt=rt, plan=plan, g=g, from_site=from_site, kind="vectorize")
        _vectorize(info.loops[-1], ns0, ctx)
        stats["vectorize"] = ctx.registered
        stats["aborted"] += ctx.aborted
    return stats
