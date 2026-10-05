"""J++x 语法分析：记号流 → AST。

文法：现行 .jpp 表达式（FRONTEND.md）+ Fable-A 的 cell / resident / peek / settled / put / claim / spawn / ev。
报错带文件、行、列与源码行。
"""
from __future__ import annotations

import os

from . import ast as A
from .lexer import JxSyntaxError, Tok, lex

REDUCERS = {"single", "union", "by_key", "claim", "override"}
SOURCE_KINDS = {"event", "timer", "change", "settled"}

# 二元运算优先级（左结合）
PREC = {"??": 1, "||": 2, "&&": 3, "==": 4, "!=": 4, "<": 5, ">": 5, "<=": 5, ">=": 5, "<-": 5,
        "+": 6, "-": 6, "*": 7, "/": 7, "%": 7}


class Parser:
    def __init__(self, src: str, file: str = "<src>"):
        self.src = src
        self.file = file
        self.toks = lex(src, file)
        self.i = 0

    # ------------------------------------------------------------ 工具
    @property
    def t(self) -> Tok:
        return self.toks[self.i]

    def peek(self, k=1) -> Tok:
        return self.toks[min(self.i + k, len(self.toks) - 1)]

    def err(self, msg, tok: Tok | None = None):
        tok = tok or self.t
        raise JxSyntaxError(msg, tok.line, tok.col, self.file, self.src)

    def at(self, kind, val=None) -> bool:
        t = self.t
        return t.kind == kind and (val is None or t.val == val)

    def at_sym(self, s) -> bool:
        return self.t.kind == "sym" and self.t.val == s

    def at_kw(self, s) -> bool:
        return self.t.kind == "kw" and self.t.val == s

    def eat_sym(self, s) -> Tok:
        if not self.at_sym(s):
            self.err(f"这里要 `{s}`，看到的是 {self._desc(self.t)}")
        t = self.t
        self.i += 1
        return t

    def eat_kw(self, s) -> Tok:
        if not self.at_kw(s):
            self.err(f"这里要 `{s}`，看到的是 {self._desc(self.t)}")
        t = self.t
        self.i += 1
        return t

    def maybe_sym(self, s) -> bool:
        if self.at_sym(s):
            self.i += 1
            return True
        return False

    def ident(self, allow_kw=False) -> str:
        t = self.t
        if t.kind == "ident" or (allow_kw and t.kind == "kw"):
            self.i += 1
            return t.val
        self.err(f"这里要一个名字，看到的是 {self._desc(t)}")

    @staticmethod
    def _desc(t: Tok) -> str:
        if t.kind == "eof":
            return "文件结尾"
        return f"`{t.val}`"

    def mk(self, cls, tok: Tok, **kw):
        n = cls(**kw)
        n.line, n.col, n.file = tok.line, tok.col, self.file
        return n

    # ------------------------------------------------------------ 程序
    def program(self) -> A.Program:
        start = self.t
        prog = self.mk(A.Program, start)
        prog.src = self.src
        while self.at_kw("import"):
            t = self.t
            self.i += 1
            if not self.at("str"):
                self.err("import 后面要一个字符串路径")
            path = self.t.val
            self.i += 1
            self.eat_sym(";")
            prog.imports.append(self.mk(A.Import, t, path=path))
        if self.at_kw("budget"):
            self.i += 1
            if not self.at_sym("{"):
                self.err("budget 后面要一个记录，如 budget {calls: 10, cost: 0.01};")
            prog.budget = self.record()
            self.eat_sym(";")
        while not self.at("eof"):
            if self.at_kw("import"):
                self.err("import 只能写在文件开头")
            if self.at_kw("budget"):
                self.err("budget 只能写在文件开头（import 之后）")
            st, is_final = self.statement(top=True)
            if is_final:
                prog.final = st
                if not self.at("eof"):
                    self.err("程序的结果表达式后面不能再有语句（漏了分号？）")
                break
            prog.stmts.append(st)
        return prog

    def statement(self, top=False):
        """返回 (节点, 是否是块的结果表达式)。"""
        t = self.t
        if self.at_kw("let"):
            self.i += 1
            name = self.ident()
            if self.maybe_sym(":"):
                self.skip_type()
            self.eat_sym("=")
            e = self.expr()
            self.eat_sym(";")
            return self.mk(A.Let, t, name=name, e=e), False
        if self.at_kw("fn") and self.peek().kind == "ident":
            self.i += 1
            name = self.ident()
            f = self.fn_tail(t, name)
            self.maybe_sym(";")
            return self.mk(A.FnDecl, t, name=name, fn=f), False
        if self.at_kw("cell"):
            if not top:
                self.err("cell 只能在顶层声明")
            return self.cell_decl(), False
        if self.at_kw("resident"):
            if not top:
                self.err("resident 只能在顶层声明")
            return self.resident_decl(), False
        e = self.expr()
        if self.maybe_sym(";"):
            return self.mk(A.ExprStmt, t, e=e), False
        if self.at_sym("}") or self.at("eof"):
            return e, True
        # 以块结尾的表达式（if/块）后面可以不写分号
        if isinstance(e, (A.If, A.Block)):
            return self.mk(A.ExprStmt, t, e=e), False
        self.err(f"表达式后面缺 `;`（看到 {self._desc(self.t)}）")

    def cell_decl(self) -> A.CellDecl:
        t = self.eat_kw("cell")
        name = self.ident()
        keys = []
        if self.maybe_sym("["):
            keys.append(self.ident())
            while self.maybe_sym(","):
                keys.append(self.ident())
            self.eat_sym("]")
        red, arg = "single", None
        if self.at_kw("reducer"):
            self.i += 1
            rt = self.t
            red = self.ident(allow_kw=True)
            if red not in REDUCERS:
                self.err(f"不认识的归约器 `{red}`；可选 single / union / by_key(字段) / claim / override", rt)
            if red == "by_key":
                self.eat_sym("(")
                arg = self.ident(allow_kw=True)
                self.eat_sym(")")
        self.eat_sym(";")
        return self.mk(A.CellDecl, t, name=name, keys=keys, reducer=red, reducer_arg=arg)

    def resident_decl(self) -> A.ResidentDecl:
        t = self.eat_kw("resident")
        name = self.ident()
        params = []
        if self.maybe_sym("("):
            if not self.at_sym(")"):
                params.append(self.param())
                while self.maybe_sym(","):
                    if self.at_sym(")"):
                        break
                    params.append(self.param())
            self.eat_sym(")")
        sources = []
        if self.at_kw("on"):
            self.i += 1
            self.eat_sym("[")
            if not self.at_sym("]"):
                sources.append(self.source())
                while self.maybe_sym(","):
                    if self.at_sym("]"):
                        break
                    sources.append(self.source())
            self.eat_sym("]")
        budget = deadline = rank = None
        at_rest = False
        while True:
            if self.at_kw("budget"):
                self.i += 1
                if not self.at_sym("{"):
                    self.err("resident 的 budget 子句要一个记录，如 budget {calls: 96}")
                budget = self.record()
            elif self.at("ident", "deadline"):
                self.i += 1
                deadline = self.unary()
            elif self.at("ident", "rank"):
                self.i += 1
                rank = self.unary()
            elif self.at("ident", "at_rest"):
                self.i += 1
                at_rest = True
            else:
                break
        if not self.at_sym("{"):
            self.err("resident 的程序体要用 { } 包起来")
        body = self.block()
        return self.mk(A.ResidentDecl, t, name=name, params=params, sources=sources, body=body,
                       budget=budget, deadline=deadline, rank=rank, at_rest=at_rest)

    def source(self) -> A.Source:
        t = self.t
        kind = self.ident(allow_kw=True)
        if kind not in SOURCE_KINDS:
            self.err(f"不认识的事件来源 `{kind}`；可选 event(名) / timer(秒) / change(单元) / settled(单元)", t)
        self.eat_sym("(")
        if kind == "event":
            arg = self.ident(allow_kw=True)
        elif kind == "timer":
            if not self.at("num"):
                self.err("timer 要一个秒数")
            arg = self.t.val
            self.i += 1
        else:
            arg = self.cellref()
        self.eat_sym(")")
        return self.mk(A.Source, t, kind=kind, arg=arg)

    def param(self) -> str:
        name = self.ident()
        if self.maybe_sym(":"):
            self.skip_type()
        return name

    def skip_type(self):
        """类型标注只解析不使用（与现行 .jpp 一致，检查由公共检查器做）。"""
        t = self.t
        if t.kind == "ident" and t.val in ("Fn", "Fn1"):
            self.i += 1
            self.eat_sym("(")
            if not self.at_sym(")"):
                self.skip_type()
                while self.maybe_sym(","):
                    self.skip_type()
            self.eat_sym(")")
            if self.at_sym("-") and self.peek().kind == "sym" and self.peek().val == "!":
                self.i += 2
                self.skip_effects()
            if self.at_sym("->"):
                self.i += 1
                self.skip_type()
            return
        if t.kind in ("ident", "kw"):
            self.i += 1
            if self.maybe_sym("<"):
                self.skip_type()
                while self.maybe_sym(","):
                    self.skip_type()
                self.eat_sym(">")
            return
        if self.maybe_sym("["):
            self.skip_type()
            self.eat_sym("]")
            return
        if self.maybe_sym("{"):
            depth = 1
            while depth and not self.at("eof"):
                if self.at_sym("{"):
                    depth += 1
                elif self.at_sym("}"):
                    depth -= 1
                self.i += 1
            return
        self.err("类型标注写错了")

    def skip_effects(self):
        self.eat_sym("{")
        while not self.at_sym("}"):
            self.i += 1
            if self.at("eof"):
                self.err("效应标注没有结束")
        self.eat_sym("}")

    # ------------------------------------------------------------ 块、函数
    def block(self) -> A.Block:
        t = self.eat_sym("{")
        b = self.mk(A.Block, t)
        while not self.at_sym("}"):
            if self.at("eof"):
                self.err("块没有结束（缺 `}`）", t)
            st, is_final = self.statement()
            if is_final:
                b.final = st
                break
            b.stmts.append(st)
        self.eat_sym("}")
        return b

    def fn_tail(self, t: Tok, name="") -> A.FnLit:
        self.eat_sym("(")
        params = []
        if not self.at_sym(")"):
            params.append(self.param())
            while self.maybe_sym(","):
                if self.at_sym(")"):
                    break
                params.append(self.param())
        self.eat_sym(")")
        if self.at_sym("->"):
            self.i += 1
            self.skip_type()
        if self.at_sym("!") and self.peek().kind == "sym" and self.peek().val == "{":
            self.i += 1
            self.skip_effects()
        if not self.at_sym("{"):
            self.err("函数体要用 { } 包起来")
        body = self.block()
        return self.mk(A.FnLit, t, params=params, body=body, name=name)

    # ------------------------------------------------------------ 表达式
    def expr(self, minp=0) -> A.Node:
        left = self.unary()
        while True:
            t = self.t
            if t.kind != "sym" or t.val not in PREC:
                break
            p = PREC[t.val]
            if p < minp:
                break
            self.i += 1
            if t.val == "<-":
                # `a<-1` 是 a < -1
                right = self.expr(p + 1)
                neg = self.mk(A.Unary, t, op="-", e=right)
                left = self.mk(A.Binary, t, op="<", l=left, r=neg)
                continue
            right = self.expr(p + 1)
            left = self.mk(A.Binary, t, op=t.val, l=left, r=right)
        return left

    def unary(self) -> A.Node:
        t = self.t
        if self.at_sym("!") or self.at_sym("-"):
            self.i += 1
            e = self.unary()
            return self.mk(A.Unary, t, op=t.val, e=e)
        if self.at_kw("peek"):
            self.i += 1
            return self.mk(A.Peek, t, ref=self.cellref())
        if self.at_kw("snap"):
            self.i += 1
            return self.mk(A.Peek, t, ref=self.cellref(), track=False)
        if self.at_kw("settled"):
            self.i += 1
            return self.mk(A.Settled, t, ref=self.cellref())
        if self.at_kw("put") or self.at_kw("claim"):
            self.i += 1
            ref = self.cellref()
            if not self.at_sym("<-"):
                self.err(f"{t.val} 的写法是 `{t.val} 单元[键] <- 值`")
            self.i += 1
            e = self.expr()
            return self.mk(A.Put if t.val == "put" else A.Claim, t, ref=ref, e=e)
        if self.at_kw("spawn"):
            self.i += 1
            name = self.ident()
            self.eat_sym("(")
            args = self.args_until(")")
            node = self.mk(A.Spawn, t, name=name, args=args)
            while True:
                if self.at_kw("budget"):
                    self.i += 1
                    node.budget = self.postfix(self.primary())
                elif self.at("ident", "deadline"):
                    self.i += 1
                    node.deadline = self.unary()
                elif self.at("ident", "rank"):
                    self.i += 1
                    node.rank = self.unary()
                else:
                    break
            return node
        return self.postfix(self.primary())

    def cellref(self) -> A.CellRef:
        t = self.t
        name = self.ident()
        keys = []
        if self.maybe_sym("["):
            keys = self.args_until("]")
        return self.mk(A.CellRef, t, name=name, keys=keys)

    def args_until(self, close) -> list:
        args = []
        if self.maybe_sym(close):
            return args
        args.append(self.expr())
        while self.maybe_sym(","):
            if self.at_sym(close):
                break
            args.append(self.expr())
        self.eat_sym(close)
        return args

    def postfix(self, e) -> A.Node:
        while True:
            t = self.t
            if self.at_sym("("):
                self.i += 1
                e = self.mk(A.Call, t, fn=e, args=self.args_until(")"))
            elif self.at_sym("."):
                self.i += 1
                if self.at("num"):     # r.0 风格不支持
                    self.err("字段名不能是数字；下标用 r[0]")
                e = self.mk(A.Field, t, obj=e, name=self.ident(allow_kw=True))
            elif self.at_sym("["):
                self.i += 1
                idx = self.expr()
                self.eat_sym("]")
                e = self.mk(A.Index, t, obj=e, idx=idx)
            else:
                return e

    def primary(self) -> A.Node:
        t = self.t
        if t.kind == "num":
            self.i += 1
            return self.mk(A.Num, t, v=t.val)
        if t.kind == "str":
            self.i += 1
            return self.mk(A.Str, t, v=t.val)
        if t.kind == "ident":
            self.i += 1
            return self.mk(A.Var, t, name=t.val)
        if t.kind == "kw":
            if t.val in ("true", "false"):
                self.i += 1
                return self.mk(A.Bool, t, v=(t.val == "true"))
            if t.val == "unit":
                self.i += 1
                return self.mk(A.Unit, t)
            if t.val == "ev":
                self.i += 1
                return self.mk(A.Ev, t)
            if t.val == "fn":
                self.i += 1
                return self.fn_tail(t)
            if t.val == "if":
                return self.if_expr()
            self.err(f"`{t.val}` 不能出现在这里")
        if self.at_sym("("):
            self.i += 1
            if self.maybe_sym(")"):
                return self.mk(A.Unit, t)
            e = self.expr()
            if self.at_sym(","):
                # (a, b) 复合对象 = 列表
                items = [e]
                while self.maybe_sym(","):
                    if self.at_sym(")"):
                        break
                    items.append(self.expr())
                self.eat_sym(")")
                return self.mk(A.ListLit, t, items=items)
            self.eat_sym(")")
            return e
        if self.at_sym("["):
            self.i += 1
            return self.mk(A.ListLit, t, items=self.args_until("]"))
        if self.at_sym("{"):
            nx, nx2 = self.peek(1), self.peek(2)
            if nx.kind == "sym" and nx.val == "}":
                self.i += 2
                return self.mk(A.RecordLit, t, items=[])
            if nx.kind in ("ident", "kw", "str") and nx2.kind == "sym" and nx2.val == ":":
                return self.record()
            return self.block()
        self.err(f"这里要一个表达式，看到的是 {self._desc(t)}")

    def record(self) -> A.RecordLit:
        t = self.eat_sym("{")
        items = []
        seen = set()
        while not self.at_sym("}"):
            kt = self.t
            if kt.kind == "str":
                key = kt.val
                self.i += 1
            else:
                key = self.ident(allow_kw=True)
            if key in seen:
                self.err(f"记录里字段 `{key}` 重复", kt)
            seen.add(key)
            self.eat_sym(":")
            items.append((key, self.expr()))
            if not self.maybe_sym(","):
                break
        self.eat_sym("}")
        return self.mk(A.RecordLit, t, items=items)

    def if_expr(self) -> A.If:
        t = self.eat_kw("if")
        cond = self.expr()
        if not self.at_sym("{"):
            self.err("if 的分支要用 { } 包起来")
        then = self.block()
        els = None
        if self.at_kw("else"):
            self.i += 1
            if self.at_kw("if"):
                els = self.if_expr()
            elif self.at_sym("{"):
                els = self.block()
            else:
                els = self.expr()
        return self.mk(A.If, t, cond=cond, then=then, els=els)


def parse(src: str, file: str = "<src>") -> A.Program:
    return Parser(src, file).program()


def load(path: str) -> tuple[A.Program, list[A.Program]]:
    """读主程序与它 import 的库（相对路径，每个规范路径只载一次，检测环）。返回 (主程序, 库列表按依赖序)。"""
    path = os.path.abspath(path)
    libs: list[A.Program] = []
    loaded: dict[str, A.Program] = {}
    stack: list[str] = []

    def load_one(p: str, is_main: bool) -> A.Program:
        if p in stack:
            raise JxSyntaxError("import 成环：" + " → ".join(stack + [p]), 1, 1, p)
        if p in loaded:
            return loaded[p]
        if not os.path.exists(p):
            raise JxSyntaxError(f"找不到文件 {p}", 1, 1, stack[-1] if stack else p)
        src = open(p, encoding="utf-8").read()
        prog = parse(src, os.path.relpath(p))
        stack.append(p)
        for im in prog.imports:
            q = os.path.normpath(os.path.join(os.path.dirname(p), im.path))
            if not os.path.exists(q) and not q.endswith(".jpx"):
                q += ".jpx"
            try:
                load_one(q, False)
            except JxSyntaxError as e:
                if "找不到文件" in e.msg:
                    raise JxSyntaxError(e.msg, im.line, im.col, prog.file, src) from None
                raise
        stack.pop()
        if not is_main:
            if prog.budget is not None:
                raise JxSyntaxError("库文件不能声明 budget", prog.budget.line, prog.budget.col, prog.file, src)
            if prog.final is not None:
                raise JxSyntaxError("库文件不能有结果表达式（只放声明）", prog.final.line, prog.final.col, prog.file, src)
            libs.append(prog)
        loaded[p] = prog
        return prog

    main = load_one(path, True)
    return main, libs
