"""静态检查（`python -m jx check`）：未定义的名字、单元与常驻程序引用、single 单元多写者、spawn 实参个数、
put 写常驻程序、claim 写非占用单元。返回诊断文字列表（带文件:行:列），「错误」开头的会让检查失败。"""
from __future__ import annotations

from . import ast as A
from .interp import Interp
from .parser import load


class _Dummy:
    flags = {}


def check_program(path) -> list[str]:
    prog, libs = load(path)
    diags: list[str] = []
    builtins = set(Interp.__dict__)
    builtins = {n[2:] for n in dir(Interp) if n.startswith("b_")}
    cells, residents, globals_ = {}, {}, set()
    progs = libs + [prog]
    for p in progs:
        for st in p.stmts:
            if isinstance(st, A.CellDecl):
                cells[st.name] = st
            elif isinstance(st, A.ResidentDecl):
                residents[st.name] = st
            elif isinstance(st, (A.Let, A.FnDecl)):
                globals_.add(st.name)

    def err(node, msg):
        diags.append(f"{node.where}: 错误：{msg}")

    def warn(node, msg):
        diags.append(f"{node.where}: 提示：{msg}")

    writers: dict[str, set] = {}

    def scope_check(n, scope: set, owner: str):
        if isinstance(n, A.Var):
            if n.name not in scope and n.name not in globals_ and n.name not in builtins \
                    and n.name not in cells and n.name not in residents:
                err(n, f"未定义的名字 `{n.name}`")
            return
        if isinstance(n, A.Block):
            s = set(scope)
            for st in n.stmts:
                if isinstance(st, A.Let):
                    scope_check(st.e, s, owner)
                    s.add(st.name)
                elif isinstance(st, A.FnDecl):
                    s.add(st.name)
                    scope_check(st.fn, s, owner)
                elif isinstance(st, A.ExprStmt):
                    scope_check(st.e, s, owner)
                else:
                    scope_check(st, s, owner)
            if n.final is not None:
                scope_check(n.final, s, owner)
            return
        if isinstance(n, A.FnLit):
            scope_check(n.body, scope | set(n.params), owner)
            return
        if isinstance(n, A.CellRef):
            if n.name not in cells and n.name not in residents and n.name not in scope:
                err(n, f"未声明的单元 `{n.name}`")
            elif n.name in cells and n.keys and len(n.keys) != len(cells[n.name].keys):
                err(n, f"单元 `{n.name}` 有 {len(cells[n.name].keys)} 个键，这里给了 {len(n.keys)} 个")
            for k in n.keys:
                scope_check(k, scope, owner)
            return
        if isinstance(n, A.Put):
            if n.ref.name in residents:
                err(n, f"`{n.ref.name}` 是常驻程序，不能 put")
            elif n.ref.name in cells and cells[n.ref.name].reducer == "claim":
                err(n, f"`{n.ref.name}` 是占用单元，用 claim 写")
            writers.setdefault(n.ref.name, set()).add(owner)
        if isinstance(n, A.Claim):
            if n.ref.name in cells and cells[n.ref.name].reducer != "claim":
                err(n, f"`{n.ref.name}` 不是占用单元（声明 reducer claim）")
        if isinstance(n, A.Spawn):
            r = residents.get(n.name)
            if r is None:
                err(n, f"spawn 的 `{n.name}` 不是常驻程序")
            elif len(r.params) != len(n.args):
                err(n, f"常驻程序 {n.name} 要 {len(r.params)} 个实参，给了 {len(n.args)} 个")
        for k, v in n.__dict__.items():
            if k in ("line", "col", "file", "nid"):
                continue
            if isinstance(v, A.Node):
                scope_check(v, scope, owner)
            elif isinstance(v, list):
                for x in v:
                    if isinstance(x, A.Node):
                        scope_check(x, scope, owner)
                    elif isinstance(x, tuple):
                        for y in x:
                            if isinstance(y, A.Node):
                                scope_check(y, scope, owner)

    for p in progs:
        for st in p.stmts:
            if isinstance(st, A.Let):
                scope_check(st.e, set(), "main")
            elif isinstance(st, A.FnDecl):
                scope_check(st.fn, set(), "main")
            elif isinstance(st, A.ExprStmt):
                scope_check(st.e, set(), "main")
            elif isinstance(st, A.ResidentDecl):
                for s in st.sources:
                    if s.kind in ("change", "settled"):
                        scope_check(s.arg, set(st.params), st.name)
                scope_check(st.body, set(st.params), st.name)
        if p.final is not None:
            scope_check(p.final, set(), "main")
    for c, ws in writers.items():
        d = cells.get(c)
        if d is not None and d.reducer == "single" and len(ws) > 1:
            warn(d, f"single 单元 `{c}` 有多个写者程序：{', '.join(sorted(ws))}（R7：多写者要声明 union/by_key/claim/override）")
    for r in residents.values():
        bound = set()
        for s in r.sources:
            if s.kind in ("change", "settled") and all(isinstance(k, A.Var) and k.name in r.params for k in s.arg.keys) \
                    and {k.name for k in s.arg.keys} == set(r.params):
                bound = set(r.params)
            if s.kind == "event":
                bound = set(r.params)
        if r.params and not bound:
            spawned = any(isinstance(x, A.Spawn) and x.name == r.name for p in progs for x in _all_nodes(p))
            if not spawned:
                warn(r, f"常驻程序 {r.name} 的参数没有被任何来源绑定，也没有地方 spawn 它，不会被实例化")
    return diags


def _all_nodes(p):
    out = []
    A.walk(p.stmts, out.append)
    return out
