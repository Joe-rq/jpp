"""J++x 运行时值。记录 = dict，列表 = list，文字/数/布尔 = Python 原生，unit = None。"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .core import Exit, Question, Reading, canon, h


class JxError(Exception):
    """运行时错误，带源码位置。"""

    def __init__(self, msg: str, node=None):
        self.msg = msg
        self.where = node.where if node is not None and hasattr(node, "where") else ""
        super().__init__(f"{self.where}: {msg}" if self.where else msg)


class Stop(Exception):
    def __init__(self, value):
        self.value = value


@dataclass(frozen=True)
class Mat:
    content: Any
    origin: str | None = None
    taint: str = "trusted"
    addr: str | None = None
    tier: str = "t0"


@dataclass(frozen=True)
class State:
    on: Any
    ctx: Any = None
    ref: Any = None
    over: Any = None
    owners: tuple = ()

    def wire(self) -> dict:
        w = self.__dict__.get("_wire")
        if w is None:
            w = {"on": render(self.on)}
            for k in ("ctx", "ref", "over"):
                v = getattr(self, k)
                if v is not None:
                    w[k] = render(v)
            object.__setattr__(self, "_wire", w)      # 状态不可变：渲染与哈希只算一次
        return w

    @property
    def sid(self) -> str:
        s = self.__dict__.get("_sid")
        if s is None:
            s = h(self.wire())
            object.__setattr__(self, "_sid", s)
        return s


def render(v):
    """材料渲染成判断器看得见的 JSON：Mat 取 content，其余递归。"""
    if isinstance(v, Mat):
        return render(v.content)
    if isinstance(v, dict):
        return {k: render(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [render(x) for x in v]
    if isinstance(v, (Exit, Reading, Question, Pend, Fail, Handle)):
        return to_py(v)
    return v


def origins(v, acc=None):
    acc = [] if acc is None else acc
    if isinstance(v, Mat):
        if v.origin and v.origin not in acc:
            acc.append(v.origin)
        origins(v.content, acc)
    elif isinstance(v, dict):
        for x in v.values():
            origins(x, acc)
    elif isinstance(v, (list, tuple)):
        for x in v:
            origins(x, acc)
    return acc


@dataclass(eq=False)
class Closure:
    fn: Any                  # ast.FnLit
    env: Any
    name: str = ""

    def __repr__(self):
        return f"<fn {self.name or '匿名'}@{self.fn.where}>"


@dataclass(eq=False)
class Builtin:
    name: str
    impl: Any                # async (ip, cx, args, node) -> value


@dataclass(eq=False)
class Pend:
    """未决值：结果依赖一个还没有的值（settled 读到未定、或缺席）。运算传播，不当假、不当空。"""
    cause: str
    needed: Any = None
    on: Any = None

    def __repr__(self):
        return f"<未决 {self.cause}>"


class Fam(str):
    """单元族名作为值（peek_family(edge, a)、members(world) 的第一个参数）。"""


@dataclass(frozen=True)
class Fail:
    msg: str


@dataclass(frozen=True)
class Handle:
    """程序单元或单元实例的引用（spawn 的返回值）。"""
    name: str
    key: tuple


@dataclass(eq=False)
class LazyReading:
    """惰性读数：judge 只登记，被检视时才等待。"""
    req: Any                 # sched.Req
    q: Question = None
    state: State = None
    companion: Any = None    # 同批伴随题（缺哪类信息）的 LazyReading

    @property
    def done(self):
        return self.req.done


@dataclass(eq=False)
class LazyExit:
    lr: LazyReading
    line: Any = None
    node: Any = None
    result: Exit | None = None          # 过默认链后的出口
    raw: Exit | None = None             # 未过默认链的出口
    emitted: bool = False


@dataclass(eq=False)
class Lazy:
    """惰性的 gen / do 结果。"""
    fut: Any
    kind: str = "gen"


def to_py(v, depth=0):
    """转成纯 Python（给宿主、事件总线、哈希）。"""
    if depth > 60:
        return "<深>"
    if v is None or isinstance(v, (bool, int, float, str)):
        return v
    if isinstance(v, dict):
        return {str(k): to_py(x, depth + 1) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [to_py(x, depth + 1) for x in v]
    if isinstance(v, Mat):
        o = {"mat": True, "content": to_py(v.content, depth + 1), "origin": v.origin}
        return o
    if isinstance(v, State):
        return {"state": True, **to_py(v.wire(), depth + 1), "owners": list(v.owners)}
    if isinstance(v, Exit):
        r = v.reading
        q = r.q if r else None
        o = {"exit": True, "kind": v.kind, "value": v.value, "cause": v.cause or None, "grade": v.grade,
             "p": round(v.conf, 4) if v.conf else (round(r.top(), 4) if (r and r.q.op == "test") else 0.0),
             "conf": round(v.conf, 4), "q": q.text if q else None, "key": q.key if q else None}
        if v.lean is not None:
            o["lean"] = v.lean
        if v.needed is not None:
            o["needed"] = to_py(v.needed)
        if v.ask_to:
            o["ask_to"] = list(v.ask_to)
        if v.waiting:
            o["waiting"] = True
        return o
    if isinstance(v, Reading):
        o = {"reading": True, "q": v.q.text, "key": v.q.key, "by": v.by}
        if v.q.op == "test":
            o["p"] = v.p
        else:
            o["dist"] = [list(x) for x in v.dist] if v.q.op == "select" else list(v.dist)
        return o
    if isinstance(v, Question):
        return {"question": True, "op": v.op, "q": v.text, "key": v.key}
    if isinstance(v, Pend):
        return {"pending": v.cause, "needed": to_py(v.needed, depth + 1)}
    if isinstance(v, Fail):
        return {"fail": v.msg}
    if isinstance(v, Handle):
        return {"unit": v.name, "key": to_py(list(v.key), depth + 1)}
    if isinstance(v, (Closure, Builtin)):
        return f"<fn {getattr(v, 'name', '')}>"
    if isinstance(v, (LazyExit, LazyReading, Lazy)):
        return {"lazy": True}
    return str(v)


def vhash(v) -> str:
    return h(to_py(v))


def key_of(args) -> tuple:
    """单元键：实参转成可哈希的规范形。"""
    out = []
    for a in args:
        if isinstance(a, (str, int, float, bool)) or a is None:
            out.append(a)
        else:
            out.append(canon(to_py(a)))
    return tuple(out)


def type_name(v) -> str:
    if v is None:
        return "unit"
    return {bool: "布尔", int: "数", float: "数", str: "文字", dict: "记录", list: "列表"}.get(type(v), type(v).__name__)
