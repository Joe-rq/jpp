"""J++x 抽象语法树。每个节点带位置（文件、行、列）与全局序号 nid（站点键 = 定义路径 + 序号）。"""
from __future__ import annotations

import itertools
from dataclasses import dataclass, field

_ids = itertools.count(1)


@dataclass(eq=False)
class Node:
    line: int = 0
    col: int = 0
    file: str = "<src>"
    nid: int = field(default_factory=lambda: next(_ids), init=False)

    @property
    def where(self) -> str:
        return f"{self.file}:{self.line}:{self.col}"


# ---------------------------------------------------------------- 表达式

@dataclass(eq=False)
class Num(Node):
    v: float | int = 0


@dataclass(eq=False)
class Str(Node):
    v: str = ""


@dataclass(eq=False)
class Bool(Node):
    v: bool = False


@dataclass(eq=False)
class Unit(Node):
    pass


@dataclass(eq=False)
class Var(Node):
    name: str = ""


@dataclass(eq=False)
class ListLit(Node):
    items: list = field(default_factory=list)


@dataclass(eq=False)
class RecordLit(Node):
    items: list = field(default_factory=list)   # [(key, expr)]


@dataclass(eq=False)
class Block(Node):
    stmts: list = field(default_factory=list)
    final: Node | None = None


@dataclass(eq=False)
class FnLit(Node):
    params: list = field(default_factory=list)
    body: Block | None = None
    name: str = ""


@dataclass(eq=False)
class If(Node):
    cond: Node | None = None
    then: Node | None = None
    els: Node | None = None


@dataclass(eq=False)
class Call(Node):
    fn: Node | None = None
    args: list = field(default_factory=list)


@dataclass(eq=False)
class Field(Node):
    obj: Node | None = None
    name: str = ""


@dataclass(eq=False)
class Index(Node):
    obj: Node | None = None
    idx: Node | None = None


@dataclass(eq=False)
class Unary(Node):
    op: str = ""
    e: Node | None = None


@dataclass(eq=False)
class Binary(Node):
    op: str = ""
    l: Node | None = None
    r: Node | None = None


@dataclass(eq=False)
class Ev(Node):
    pass


@dataclass(eq=False)
class CellRef(Node):
    name: str = ""
    keys: list = field(default_factory=list)


@dataclass(eq=False)
class Peek(Node):
    ref: CellRef | None = None
    track: bool = True          # snap：读当前版本，但不登记依赖（读到的版本照样进账本）


@dataclass(eq=False)
class Settled(Node):
    ref: CellRef | None = None


@dataclass(eq=False)
class Put(Node):
    ref: CellRef | None = None
    e: Node | None = None


@dataclass(eq=False)
class Claim(Node):
    ref: CellRef | None = None
    e: Node | None = None


@dataclass(eq=False)
class Spawn(Node):
    name: str = ""
    args: list = field(default_factory=list)
    budget: Node | None = None
    deadline: Node | None = None
    rank: Node | None = None        # 派生排序值：被派生的实例在事件链的级联预算里按它排（预注册 16）


# ---------------------------------------------------------------- 语句与声明

@dataclass(eq=False)
class Let(Node):
    name: str = ""
    e: Node | None = None


@dataclass(eq=False)
class FnDecl(Node):
    name: str = ""
    fn: FnLit | None = None


@dataclass(eq=False)
class ExprStmt(Node):
    e: Node | None = None


@dataclass(eq=False)
class Import(Node):
    path: str = ""


@dataclass(eq=False)
class CellDecl(Node):
    name: str = ""
    keys: list = field(default_factory=list)
    reducer: str = "single"
    reducer_arg: str | None = None


@dataclass(eq=False)
class Source(Node):
    kind: str = ""          # event | timer | change | settled
    arg: object = None      # event: 名字；timer: 秒；change/settled: CellRef


@dataclass(eq=False)
class ResidentDecl(Node):
    name: str = ""
    params: list = field(default_factory=list)
    sources: list = field(default_factory=list)
    body: Block | None = None
    budget: Node | None = None       # 接入预算子句：这个程序被一条事件链触发时，从链账户派生（只收紧）
    deadline: Node | None = None     # 截止子句（秒）
    rank: Node | None = None         # 级联排序子句：值是数时这个实例参与事件链的级联预算、按它排；unit 不参与（预注册 16）
    at_rest: bool = False            # 链结束时才跑：被标脏后等引擎到链结束再按当时输入跑一次（预注册 17）


@dataclass(eq=False)
class Program(Node):
    budget: Node | None = None
    imports: list = field(default_factory=list)
    stmts: list = field(default_factory=list)     # Let / FnDecl / ExprStmt / CellDecl / ResidentDecl
    final: Node | None = None
    src: str = ""


def walk(n, fn):
    """前序遍历所有子节点。"""
    if isinstance(n, list):
        for x in n:
            walk(x, fn)
        return
    if not isinstance(n, Node):
        return
    fn(n)
    for k, v in n.__dict__.items():
        if k in ("line", "col", "file", "nid"):
            continue
        if isinstance(v, Node):
            walk(v, fn)
        elif isinstance(v, list):
            for x in v:
                if isinstance(x, Node):
                    walk(x, fn)
                elif isinstance(x, tuple):
                    for y in x:
                        if isinstance(y, Node):
                            walk(y, fn)
