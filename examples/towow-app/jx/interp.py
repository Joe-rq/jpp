"""J++x 解释器：async 树遍历求值。

- `judge(state, q | [q…])` 只登记（惰性读数）；`cut` 给惰性出口；出口被检视（handle / if / 读字段 / 比较）时才等待。
- 一切等待只经过引擎的一个卡口 `eng.wait`，引擎据此判断「静止」并合批发出（见 engine.py）。
- `gen` / `gen_json` 一登记就发、不阻塞静止判定。
- 没写 unsure 臂的未决出口，被检视时走 J-05 默认链：问缺哪类（同批伴随题）→ 向持有方 put inbox、settled reply →
  补进这道题的 ctx 重判；被拒或截止 → 转交进 pending，needed 写明。
"""
from __future__ import annotations

import contextvars
import json
import math
from dataclasses import replace

from . import ast as A
from .core import ABSENCE_CAUSES, Exit, Question, Reading, canon, cut as core_cut, h
from .core import measure as q_measure, select as q_select, test as q_test
from .values import (Builtin, Closure, Fail, Fam, Handle, JxError, Lazy, LazyExit, LazyReading, Mat, Pend, State,
                     Stop, origins, render, to_py, type_name)

NONE_CAT = "不缺信息，事情本身两可"
TIERS = ["t0", "t1", "t2", "never"]
FILL_CAUSES = {"band", "tie", "insufficient"}

# 只做计算、不发效应的内置（用于判断 map 回调要不要并发）
EFFECT_BUILTINS = {"judge", "gen", "gen_json", "do"}   # 回调里有这些才可能新登记并等待：才并发


# 当前纤程的块栈（前瞻提升用）：[(块, 环境, 下一条语句下标)]。每个纤程开始时换一个新列表。
FRAMES: contextvars.ContextVar = contextvars.ContextVar("jx_frames", default=None)

# 前瞻提升里允许出现的内置（只构造值或只登记，不强制、不写、不读单元族）
SPEC_BUILTINS = {"state", "mat", "test", "select", "measure", "judge", "cut", "concat", "len", "str", "join",
                 "merge", "get", "has", "keys", "values", "range", "take", "slice", "not"}


# 自己处理未决值的内置；其余内置收到顶层未决参数时直接返回该未决值
PEND_AWARE = {"is_pend", "handle", "consume", "type", "unsure_source", "do", "judge", "cut", "state", "print",
              "gen", "gen_json", "concat", "has", "get", "merge", "set", "is_exit", "act", "ignored", "is_unsure"}


def find_pend(v, depth=0):
    if isinstance(v, Pend):
        return v
    if depth > 30:
        return None
    if isinstance(v, dict):
        for x in v.values():
            p = find_pend(x, depth + 1)
            if p is not None:
                return p
    elif isinstance(v, (list, tuple)):
        for x in v:
            p = find_pend(x, depth + 1)
            if p is not None:
                return p
    elif isinstance(v, Mat):
        return find_pend(v.content, depth + 1)
    return None


class SpecAbort(Exception):
    pass


class Env:
    __slots__ = ("vars", "parent")

    def __init__(self, parent=None):
        self.vars = {}
        self.parent = parent

    def get(self, name, node=None):
        e = self
        while e is not None:
            if name in e.vars:
                return e.vars[name]
            e = e.parent
        raise JxError(f"未定义的名字 `{name}`", node)

    def has(self, name):
        e = self
        while e is not None:
            if name in e.vars:
                return True
            e = e.parent
        return False

    def set(self, name, v):
        self.vars[name] = v


def is_pend(v) -> bool:
    return isinstance(v, Pend)


def need_list(xs, who, n):
    if isinstance(xs, Fail):
        raise JxError(f"{who} 得到一个失败值：{xs.msg}（动作可能失败时先用 is_fail 分支）", n)
    if xs is None:
        return []
    if isinstance(xs, dict):
        return list(xs.values())
    if not isinstance(xs, list):
        raise JxError(f"{who} 要列表，得到{type_name(xs)}", n)
    return xs


class Interp:
    def __init__(self, engine):
        self.eng = engine
        self.globals = Env()
        self.builtins = {}
        self.global_fns = {}          # 名 → FnLit（做效应分析）
        self._eff_memo = {}
        self._lift_memo = {}
        for name in dir(self):
            if name.startswith("b_"):
                bname = name[2:]
                self.builtins[bname] = Builtin(bname, getattr(self, name))
        for k, v in self.builtins.items():
            self.globals.set(k, v)

    # ============================================================ 求值
    async def ev(self, n, env: Env, cx):
        m = getattr(self, "e_" + type(n).__name__, None)
        if m is None:
            raise JxError(f"不能求值的节点 {type(n).__name__}", n)
        return await m(n, env, cx)

    async def e_Num(self, n, env, cx):
        return n.v

    async def e__Const(self, n, env, cx):
        return n.v

    async def e_Str(self, n, env, cx):
        return n.v

    async def e_Bool(self, n, env, cx):
        return n.v

    async def e_Unit(self, n, env, cx):
        return None

    async def e_Var(self, n, env, cx):
        try:
            return env.get(n.name, n)
        except JxError:
            if n.name in self.eng.fams:
                return Fam(n.name)
            raise

    async def e_Ev(self, n, env, cx):
        return cx.ev

    async def e_ListLit(self, n, env, cx):
        return [await self.ev(x, env, cx) for x in n.items]

    async def e_RecordLit(self, n, env, cx):
        return {k: await self.ev(x, env, cx) for k, x in n.items}

    async def e_Block(self, n, env, cx):
        return await self.run_block(n, Env(env), cx)

    async def run_block(self, b: A.Block, env: Env, cx):
        frames = FRAMES.get()
        if frames is None:
            frames = []
            FRAMES.set(frames)
        fr = [b, env, 0]
        frames.append(fr)
        try:
            for i, st in enumerate(b.stmts):
                fr[2] = i
                await self.exec_stmt(st, env, cx)
            fr[2] = len(b.stmts)
            if b.final is None:
                return None
            return await self.ev(b.final, env, cx)
        finally:
            frames.pop()

    # ---------------------------------------------------- 前瞻提升（批调度的一半）
    def liftable(self, b: A.Block):
        """块里哪些 let 的右边可以提前求值（只构造状态与题、只登记判断）。按块缓存。"""
        memo = self._lift_memo.get(b.nid)
        if memo is None:
            memo = [i for i, st in enumerate(b.stmts) if isinstance(st, A.Let) and self._spec_safe(st.e)]
            self._lift_memo[b.nid] = memo
        return memo

    def _spec_safe(self, e) -> bool:
        ok = [True]
        has_judge = [False]

        def v(x):
            if not ok[0]:
                return
            if isinstance(x, (A.Num, A.Str, A.Bool, A.Unit, A.Var, A.ListLit, A.RecordLit, A.Field, A.Index)):
                return
            if isinstance(x, A.Binary) and x.op in ("+", "-", "*", "==", "!="):
                return
            if isinstance(x, A.Call) and isinstance(x.fn, A.Var) and x.fn.name in SPEC_BUILTINS:
                if x.fn.name == "judge":
                    has_judge[0] = True
                return
            ok[0] = False
        A.walk(e, v)
        return ok[0]

    async def lift(self, cx):
        """要停下来等判断之前：把当前纤程各层块里后面那些不依赖未决结果的 let 先求值，
        其中的 judge 先登记，好和别的题在同一次静止时一起发出。作者不必为了合批改写法。"""
        frames = FRAMES.get()
        if not frames:
            return
        for b, env, i in list(frames):
            idxs = [j for j in self.liftable(b) if j > i]
            if not idxs:
                continue
            spec = Env(env)
            for j in idxs:
                st = b.stmts[j]
                if (id(env), st.nid) in cx.lifted:
                    continue
                try:
                    val = await self.spec_eval(st.e, spec, cx)
                except SpecAbort:
                    continue
                except JxError:
                    continue
                spec.set(st.name, val)
                cx.lifted.add((id(env), st.nid))

    async def spec_eval(self, n, env, cx):
        if isinstance(n, (A.Num, A.Str, A.Bool)):
            return n.v
        if isinstance(n, A.Unit):
            return None
        if isinstance(n, A.Var):
            if not env.has(n.name):
                raise SpecAbort()
            v = env.get(n.name)
            if isinstance(v, (Pend, LazyExit, Lazy)):
                raise SpecAbort()
            return v
        if isinstance(n, A.ListLit):
            return [await self.spec_eval(x, env, cx) for x in n.items]
        if isinstance(n, A.RecordLit):
            return {k: await self.spec_eval(x, env, cx) for k, x in n.items}
        if isinstance(n, A.Field):
            o = await self.spec_eval(n.obj, env, cx)
            if isinstance(o, dict) and n.name in o:
                return o[n.name]
            raise SpecAbort()
        if isinstance(n, A.Index):
            o = await self.spec_eval(n.obj, env, cx)
            i = await self.spec_eval(n.idx, env, cx)
            if isinstance(o, list) and isinstance(i, int) and -len(o) <= i < len(o):
                return o[i]
            if isinstance(o, dict) and i in o:
                return o[i]
            raise SpecAbort()
        if isinstance(n, A.Binary):
            l = await self.spec_eval(n.l, env, cx)
            r = await self.spec_eval(n.r, env, cx)
            if has_lazy(l) or has_lazy(r):
                raise SpecAbort()
            return await self.e_Binary(A.Binary(op=n.op, l=_Const(l), r=_Const(r)), env, cx)
        if isinstance(n, A.Call) and isinstance(n.fn, A.Var):
            f = env.get(n.fn.name) if env.has(n.fn.name) else None
            if not isinstance(f, Builtin) or f.name not in SPEC_BUILTINS:
                raise SpecAbort()
            args = [await self.spec_eval(a, env, cx) for a in n.args]
            if f.name not in ("judge", "cut", "concat", "len") and any(has_lazy(a) for a in args):
                raise SpecAbort()
            if f.name == "len" and any(isinstance(a, (LazyExit, LazyReading)) for a in args):
                raise SpecAbort()
            return await f.impl(cx, args, n)
        raise SpecAbort()

    async def exec_stmt(self, st, env, cx):
        if isinstance(st, A.Let):
            env.set(st.name, await self.ev(st.e, env, cx))
        elif isinstance(st, A.FnDecl):
            env.set(st.name, Closure(st.fn, env, st.name))
        elif isinstance(st, A.ExprStmt):
            await self.ev(st.e, env, cx)
        elif isinstance(st, (A.CellDecl, A.ResidentDecl)):
            pass
        else:
            await self.ev(st, env, cx)

    async def e_FnLit(self, n, env, cx):
        return Closure(n, env, n.name)

    async def e_If(self, n, env, cx):
        c = await self.force(await self.ev(n.cond, env, cx), cx)
        if is_pend(c):
            return c
        if isinstance(c, Exit):
            raise JxError("if 的条件是一个出口；用 act(e) / is_unsure(e) 取布尔，或用 handle 分支", n.cond)
        if not isinstance(c, bool):
            if c is None:
                c = False
            else:
                raise JxError(f"if 的条件要是布尔，得到的是{type_name(c)}", n.cond)
        if c:
            return await self.ev(n.then, env, cx)
        if n.els is None:
            return None
        return await self.ev(n.els, env, cx)

    async def e_Call(self, n, env, cx):
        f = await self.ev(n.fn, env, cx)
        args = [await self.ev(a, env, cx) for a in n.args]
        if is_pend(f):
            return f
        return await self.call(f, args, cx, n)

    async def call(self, f, args, cx, node=None):
        if isinstance(f, Closure):
            fn = f.fn
            e = Env(f.env)
            for i, p in enumerate(fn.params):
                e.set(p, args[i] if i < len(args) else None)
            if len(args) > len(fn.params):
                raise JxError(f"函数 {f.name or ''} 要 {len(fn.params)} 个参数，给了 {len(args)} 个", node)
            return await self.run_block(fn.body, e, cx)
        if isinstance(f, Builtin):
            if f.name not in PEND_AWARE:
                for x in args:
                    if isinstance(x, Pend):
                        return x     # 未决值传播（J-05 草案 5a）：结果依赖未决值，结果就是那个未决值
            return await f.impl(cx, args, node)
        raise JxError(f"不能调用{type_name(f)}", node)

    async def e_Field(self, n, env, cx):
        o = await self.ev(n.obj, env, cx)
        return await self.field(o, n.name, cx, n)

    async def field(self, o, name, cx, node=None):
        o = await self.force(o, cx)
        if is_pend(o):
            return o
        if isinstance(o, dict):
            if name in o:
                return o[name]
            raise JxError(f"记录里没有字段 `{name}`（有：{', '.join(list(o)[:12])}）", node)
        if isinstance(o, Exit):
            return exit_field(o, name, node)
        if isinstance(o, Reading):
            if name == "p":
                return o.p if o.q.op == "test" else o.top()[1] if o.q.op == "select" else o.dist[o.top()]
            if name == "dist":
                return [list(x) for x in o.dist] if o.q.op == "select" else list(o.dist)
            if name in ("q", "text"):
                return o.q.text
            if name == "by":
                return o.by
            raise JxError(f"读数没有字段 `{name}`（读数只能经 cut 离开，J-01）", node)
        if isinstance(o, Mat):
            if name in ("content", "origin", "taint", "addr"):
                return getattr(o, name)
            if isinstance(o.content, dict) and name in o.content:
                return o.content[name]
            raise JxError(f"材料没有字段 `{name}`", node)
        if isinstance(o, State):
            if name in ("on", "ctx", "ref", "over"):
                return getattr(o, name)
            if name == "owners":
                return list(o.owners)
            if name == "id":
                return o.sid
            raise JxError(f"状态没有字段 `{name}`", node)
        if isinstance(o, Question):
            return {"text": o.text, "key": o.key, "op": o.op, "lacks": list(o.lacks)}.get(name)
        if isinstance(o, Handle):
            if name == "name":
                return o.name
            if name == "key":
                return list(o.key)
        if isinstance(o, Fail):
            if name in ("msg", "message"):
                return o.msg
        raise JxError(f"{type_name(o)}没有字段 `{name}`", node)

    async def e_Index(self, n, env, cx):
        o = await self.force(await self.ev(n.obj, env, cx), cx)
        i = await self.force(await self.ev(n.idx, env, cx), cx)
        if is_pend(o):
            return o
        if is_pend(i):
            return i
        if isinstance(o, (list, str)):
            if not isinstance(i, int) or isinstance(i, bool):
                raise JxError(f"列表下标要是整数，得到{type_name(i)}", n.idx)
            if -len(o) <= i < len(o):
                return o[i]
            raise JxError(f"下标 {i} 越界（长度 {len(o)}）", n.idx)
        if isinstance(o, dict):
            if i in o:
                return o[i]
            raise JxError(f"记录里没有键 {i!r}", n.idx)
        if isinstance(o, Mat) and isinstance(o.content, (dict, list)):
            return o.content[i]
        raise JxError(f"不能对{type_name(o)}取下标", n)

    async def e_Unary(self, n, env, cx):
        v = await self.force(await self.ev(n.e, env, cx), cx)
        if is_pend(v):
            return v
        if n.op == "!":
            if not isinstance(v, bool):
                raise JxError(f"`!` 要布尔，得到{type_name(v)}", n)
            return not v
        if isinstance(v, Reading):
            raise JxError("读数不能做算术（J-01：读数只能经 cut 离开）", n)
        if not isinstance(v, (int, float)) or isinstance(v, bool):
            raise JxError(f"`-` 要数，得到{type_name(v)}", n)
        return -v

    async def e_Binary(self, n, env, cx):
        op = n.op
        if op == "??":
            # J++x 扩展：左边是 unit（或读不到的字段）时取右边
            try:
                l = await self.force(await self.ev(n.l, env, cx), cx)
            except JxError as e:
                if "没有字段" not in e.msg and "没有键" not in e.msg:
                    raise
                l = None
            if l is None:
                return await self.ev(n.r, env, cx)
            return l
        if op in ("&&", "||"):
            l = await self.force(await self.ev(n.l, env, cx), cx)
            if is_pend(l):
                return l
            if isinstance(l, Exit):
                raise JxError(f"`{op}` 的左边是出口；用 act(e) 取布尔", n.l)
            if op == "&&" and not l:
                return False
            if op == "||" and l:
                return True
            r = await self.force(await self.ev(n.r, env, cx), cx)
            if is_pend(r):
                return r
            return bool(r)
        l = await self.force(await self.ev(n.l, env, cx), cx)
        r = await self.force(await self.ev(n.r, env, cx), cx)
        if is_pend(l):
            return l
        if is_pend(r):
            return r
        if isinstance(l, Reading) or isinstance(r, Reading):
            raise JxError("读数不能做算术或比较（J-01；跨题不可加，同题排序用 order）", n)
        try:
            if op == "==":
                return eq(l, r)
            if op == "!=":
                return not eq(l, r)
            if op == "+":
                if isinstance(l, str) or isinstance(r, str):
                    return show(l) + show(r)
                if isinstance(l, list) and isinstance(r, list):
                    return l + r
                return num(l, n) + num(r, n)
            if op == "-":
                return num(l, n) - num(r, n)
            if op == "*":
                return num(l, n) * num(r, n)
            if op == "/":
                d = num(r, n)
                if d == 0:
                    raise JxError("除以 0", n)
                q = num(l, n) / d
                return q
            if op == "%":
                return num(l, n) % num(r, n)
            if op in ("<", ">", "<=", ">="):
                if isinstance(l, str) and isinstance(r, str):
                    a, b = l, r
                else:
                    a, b = num(l, n), num(r, n)
                return {"<": a < b, ">": a > b, "<=": a <= b, ">=": a >= b}[op]
        except TypeError as e:
            raise JxError(f"`{op}` 用错了类型：{e}", n)
        raise JxError(f"未知运算 {op}", n)

    # ---------------------------------------------------- 单元读写
    async def cellkey(self, ref: A.CellRef, env, cx):
        """返回 (族名, 键元组) 或 Pend。名字若绑定为 Handle（spawn 返回值），直接用它。"""
        if env.has(ref.name) and not ref.keys:
            v = env.get(ref.name)
            if isinstance(v, Handle):
                return v.name, v.key
        keys = []
        for k in ref.keys:
            v = await self.force(await self.ev(k, env, cx), cx)
            if is_pend(v):
                return v
            keys.append(v)
        return ref.name, tuple(keys)

    async def e_Peek(self, n, env, cx):
        ck = await self.cellkey(n.ref, env, cx)
        if is_pend(ck):
            return ck
        return self.eng.read_cell(cx, ck[0], ck[1], "peek" if getattr(n, "track", True) else "snap", n)

    async def e_Settled(self, n, env, cx):
        ck = await self.cellkey(n.ref, env, cx)
        if is_pend(ck):
            return ck
        return self.eng.read_cell(cx, ck[0], ck[1], "settled", n)

    async def e_Put(self, n, env, cx):
        ck = await self.cellkey(n.ref, env, cx)
        v = await self.ev(n.e, env, cx)
        if is_pend(ck):
            return ck
        self.eng.put(cx, ck[0], ck[1], v, n)
        return v

    async def e_Claim(self, n, env, cx):
        ck = await self.cellkey(n.ref, env, cx)
        if is_pend(ck):
            return ck
        v = await self.deep(await self.ev(n.e, env, cx), cx)
        return self.eng.claim(cx, ck[0], ck[1], v, n)

    async def e_Spawn(self, n, env, cx):
        args = [await self.deep(await self.ev(a, env, cx), cx) for a in n.args]
        for a in args:
            if is_pend(a):
                return a
        bud = await self.deep(await self.ev(n.budget, env, cx), cx) if n.budget is not None else None
        dl = await self.force(await self.ev(n.deadline, env, cx), cx) if n.deadline is not None else None
        return self.eng.spawn(cx, n.name, args, bud, dl, n)

    # ============================================================ 惰性与强制
    async def force(self, v, cx, chain=True):
        """把惰性值变成实值（单层）。"""
        if isinstance(v, LazyExit):
            return await self.force_exit(v, cx, chain)
        if isinstance(v, LazyReading):
            return await self.force_reading(v, cx)
        if isinstance(v, Lazy):
            return await self.eng.wait(v.fut, cx)
        return v

    async def deep(self, v, cx, depth=0):
        """递归强制（发布、写单元、交给宿主之前）。"""
        if depth > 50:
            return v
        v = await self.force(v, cx)
        if isinstance(v, dict):
            out = {}
            for k, x in v.items():
                out[k] = await self.deep(x, cx, depth + 1)
            return out
        if isinstance(v, list):
            return [await self.deep(x, cx, depth + 1) for x in v]
        if isinstance(v, Mat) and isinstance(v.content, (dict, list)):
            c = await self.deep(v.content, cx, depth + 1)
            return replace(v, content=c) if c is not v.content else v
        return v

    async def force_reading(self, lr: LazyReading, cx):
        req = lr.req
        if not req.done:
            if not self.eng.flags.get("no_batch") and cx is not None:
                await self.lift(cx)
            await self.eng.before_block(cx)
            await self.eng.wait(req, cx)
        if req.exc is not None:
            raise JxError(str(req.exc))
        if req.degraded:
            self.eng.degraded_units.add(cx.uid)
        if req.missing:
            return Pend(req.missing)
        return req.value

    async def force_exit(self, lx: LazyExit, cx, chain=True) -> Exit:
        if chain and lx.result is not None:
            return lx.result
        if not chain and lx.raw is not None:
            return lx.raw
        rd = await self.force_reading(lx.lr, cx)
        if is_pend(rd):
            ex = Exit("unsure", cause=rd.cause)
            ex = replace(ex, reading=None)
        else:
            ex = core_cut(rd, lx.line)
        lx.raw = ex
        if ex.kind == "unsure" and chain:
            ex = await self.default_chain(lx, ex, cx)
            lx.result = ex
            if ex.kind == "unsure":
                cx.add_pending(lx, ex)
        elif chain and self.near_boundary(lx, ex, cx):
            # 读数触发（J-05 草案改法 3）：无线是非题读数落在边界带内、且有地方补材料 → 先补再判；
            # 出口按补完后最后一次读数定，补不到就按多数块出口并记 near_boundary
            res = await self.default_chain(lx, replace(ex, kind="unsure", cause="band"), cx, near=True)
            if res.from_key:
                ex = res
            else:
                ex = replace(ex, needed=res.needed, ask_to=res.ask_to, waiting=res.waiting)
            lx.result = ex
            if ex.kind == "unsure":
                cx.add_pending(lx, ex)
        elif chain:
            lx.result = ex
        if not lx.emitted:
            lx.emitted = True
            self.eng.on_exit(cx, lx, ex)
        return ex

    def near_boundary(self, lx, ex, cx) -> bool:
        eng = self.eng
        if eng.flags.get("no_fill") or lx.line or ex.reading is None or ex.reading.q.op != "test":
            return False
        band = eng.prof.near_band
        if band <= 0 or abs(ex.reading.p - 0.5) >= band:
            return False
        q = lx.lr.q
        src = cx.unsure_source or {}
        if not (src.get("need") or q.lacks):
            return False
        return bool(src.get("fetch")) or (eng.has_family(src.get("inbox", "inbox")) and eng.has_family(src.get("reply", "reply")))

    # ============================================================ J-05 默认链
    async def default_chain(self, lx: LazyExit, ex: Exit, cx, near=False) -> Exit:
        q, st = lx.lr.q, lx.lr.state
        eng = self.eng

        def R(ex_, route, miss, ask):
            eng.route(cx, lx, ex_, "near_boundary" if (near and route == "return") else route, miss, ask)
        if ex.cause not in FILL_CAUSES:
            # 缺席类、占用冲突：随值转交，不补
            R(ex, "return", None, ())
            return ex
        if eng.flags.get("no_fill"):
            R(ex, "return", None, ())
            return ex
        src = cx.unsure_source or {}
        cats = list(src.get("need") or q.lacks or [])
        if not cats:
            R(ex, "return", None, ())
            return ex
        # 1 问：缺哪类（伴随题同批已登记；只有一类时不问）
        if len(cats) == 1:
            cat = cats[0]
        else:
            comp = lx.lr.companion
            if comp is None:
                comp = LazyReading(eng.sched.register(cx, st, companion_q(q, cats), "companion"), companion_q(q, cats), st)
            rd = await self.force_reading(comp, cx)
            if is_pend(rd) or rd is None:
                R(ex, "return", None, ())
                return ex
            lab, _ = rd.top()
            cat = NONE_CAT if lab == "none" else dict(rd.q.labels).get(lab, lab)
        if cat == NONE_CAT:
            ex = replace(ex, needed=None)
            R(ex, "return", NONE_CAT, ())
            return ex
        # 2 取：作者 fetch > 持有方（inbox/reply 单元）> 无
        extra = None
        fetch = src.get("fetch")
        if fetch is not None:
            got = await self.force(await self.call(fetch, [q.text, cat, st], cx), cx)
            if isinstance(got, Fail) or got is None or is_pend(got):
                ex = replace(ex, needed=cat)
                R(ex, "return", cat, ())
                return ex
            extra = {cat: render(got)}
            holders = ()
        else:
            inbox, reply = src.get("inbox", "inbox"), src.get("reply", "reply")
            if not (eng.has_family(inbox) and eng.has_family(reply)):
                ex = replace(ex, needed=cat)
                R(ex, "return", cat, ())
                return ex
            owners = list(st.owners)
            frm_decl = src.get("from")
            holders = [o for o in owners if o != frm_decl] if frm_decl else owners
            if src.get("holders") is not None:
                holders = list(await self.force(await self.call(src["holders"], [q.text, cat, st], cx), cx))
            if not holders:
                ex = replace(ex, needed=cat)
                R(ex, "return", cat, ())
                return ex
            dl = src.get("deadline_s", 15)
            waiting, granted, denied, expired = [], [], [], []
            for hd in holders:
                frm = frm_decl or next((o for o in owners if o != hd), cx.uid)
                req = {"from": frm, "cat": cat, "purpose": q.text, "q": q.key or q.text[:40],
                       "asker_display": frm, "deadline_s": dl}
                eng.put(cx, inbox, (hd,), req, lx.node, internal=True)
                eng.disclose_sent(cx, hd, frm, cat, q.text)
                rv = eng.read_cell(cx, reply, (hd, frm, cat), "settled", lx.node, wait_deadline=dl)
                if is_pend(rv):
                    (expired if rv.cause == "deadline" else waiting).append(hd)
                    continue
                rv = await self.deep(rv, cx)
                eng.disclose_result(cx, hd, frm, cat, rv)
                if isinstance(rv, dict) and rv.get("denied"):
                    denied.append(hd)
                elif rv is not None:
                    granted.append((hd, rv))
            if waiting:
                ex = replace(ex, needed=cat, ask_to=tuple(holders), waiting=True)
                R(ex, "disclose_request", cat, tuple(holders))
                return ex
            if not granted:
                why = "拒绝" if denied else "截止前没有回复"
                cause = ex.cause if denied else ("deadline" if not eng.flags.get("no_deadline") else ex.cause)
                ex = replace(ex, cause=cause, needed=f"缺{cat}，持有者 {'、'.join(denied or expired)} {why}",
                             ask_to=tuple(holders))
                R(ex, "return", cat, tuple(holders))
                return ex
            extra = {f"补充·{cat}·{hd}": render(strip_flags(rv)) for hd, rv in granted}
        # 3 再判：补来的只进这道题的输入，同一道题、同一条线
        ctx = st.ctx
        if isinstance(ctx, dict):
            nctx = {**ctx, **extra}
        elif ctx is None:
            nctx = extra
        else:
            nctx = {"语境": ctx, **extra}
        st2 = State(st.on, nctx, st.ref, st.over, st.owners)
        lr2 = LazyReading(eng.sched.register(cx, st2, q, "refill"), q, st2)
        rd2 = await self.force_reading(lr2, cx)
        ex2 = Exit("unsure", cause=rd2.cause) if is_pend(rd2) else core_cut(rd2, lx.line)
        ex2 = replace(ex2, from_key=lx.lr.req.key, ask_to=tuple(holders))
        if ex2.kind == "unsure":
            ex2 = replace(ex2, needed=cat)
            R(ex2, "return", cat, tuple(holders))
        else:
            R(ex2, "refine", cat, tuple(holders))
        return ex2

    # ============================================================ 内置函数
    # ---- 题、状态、判断、桥
    async def b_mat(self, cx, a, n):
        opts = a[1] if len(a) > 1 and isinstance(a[1], dict) else {}
        c = await self.deep(a[0], cx) if a else None
        owner = opts.get("owner", opts.get("origin"))
        tier = opts.get("tier") or "t0"
        if tier not in TIERS:
            raise JxError(f"mat 的 tier 只能是 {'/'.join(TIERS)}，得到 {tier!r}", n)
        if tier == "never":
            raise JxError("E-tier：never 层的片段不能做成材料（永远不出端）", n)
        return Mat(c, None if owner is None else str(owner), opts.get("taint", "trusted"), opts.get("addr"), tier)

    async def b_state(self, cx, a, n):
        if not a:
            raise JxError("state 要一个判断对象", n)
        on = await self.deep(a[0], cx)
        opts = await self.deep(a[1], cx) if len(a) > 1 and a[1] is not None else {}
        for part in [on] + list((opts or {}).values() if isinstance(opts, dict) else []):
            pp = find_pend(part)
            if pp is not None:
                return pp            # 材料里还有未决值：这道题的状态不成立，结果随之未决
        if not isinstance(opts, dict):
            raise JxError("state 的第二个参数要是记录 {ctx, ref, over, owners}", n)
        owners = opts.get("owners")
        if owners is None:
            owners = origins(on) + [o for o in origins(opts.get("ctx")) if o not in origins(on)]
        owners = tuple(str(o) for o in owners)
        self.check_tiers(cx, [on, opts.get("ctx"), opts.get("ref"), opts.get("over")], owners, n)
        return State(on, opts.get("ctx"), opts.get("ref"), opts.get("over"), owners)

    def check_tiers(self, cx, parts, owners, n):
        """E-tier：带层的材料只能装进它的主人已向这一状态的其他主体解锁到该层的状态（解锁记录在 unlocked[主人, 对方]）。"""
        gate = self.eng.tier_gate
        if gate is None:
            return
        for m in mats_in(parts):
            r = TIERS.index(m.tier)
            if r == 0 or not m.origin:
                continue
            for p in owners:
                if p == m.origin:
                    continue
                got = self.eng.read_cell(cx, gate, (m.origin, p), "peek", n) or []
                top = max((TIERS.index(f.get("tier", "t0")) for f in got
                           if isinstance(f, dict) and f.get("tier") in TIERS), default=0)
                if r > top:
                    raise JxError(f"E-tier：{m.origin} 的 {m.tier} 层材料装进了含 {p} 的状态，"
                                  f"但 {m.origin} 只向 {p} 解锁到 {TIERS[top]}", n)

    async def b_test(self, cx, a, n):
        text, key, lacks = _qargs(a, 1, n)
        return q_test(text, key, lacks)

    async def b_select(self, cx, a, n):
        if len(a) < 2:
            raise JxError("select(题面, 候选, 键?, 缺的类别?)", n)
        opts = a[1]
        if not isinstance(opts, (dict, list)):
            raise JxError("select 的候选要是记录 {标签: 描述} 或列表", n)
        _, key, lacks = _qargs([a[0]] + list(a[2:]), 1, n)
        return q_select(a[0], {str(k): show(v) for k, v in opts.items()} if isinstance(opts, dict) else [show(x) for x in opts], key, lacks)

    async def b_measure(self, cx, a, n):
        if len(a) < 2 or not isinstance(a[1], list):
            raise JxError("measure(题面, [档位…], 键?)", n)
        _, key, lacks = _qargs([a[0]] + list(a[2:]), 1, n)
        return q_measure(a[0], [show(x) for x in a[1]], key, lacks)

    async def b_judge(self, cx, a, n):
        if len(a) < 2:
            raise JxError("judge(state, 题 或 [题…])", n)
        st, qs = a[0], a[1]
        if is_pend(st):
            return [st for _ in qs] if isinstance(qs, list) else st
        if isinstance(st, Mat) or not isinstance(st, State):
            if isinstance(st, Mat):
                st = State(st, owners=tuple(origins(st)))
            else:
                raise JxError(f"judge 的第一个参数要是 state，得到{type_name(st)}", n)
        many = isinstance(qs, list)
        qs = qs if many else [qs]
        out = []
        for q in qs:
            if not isinstance(q, Question):
                raise JxError(f"judge 的题要是 test/select/measure 造的题，得到{type_name(q)}", n)
            out.append(self.register(cx, st, q, n))
        return out if many else out[0]

    def register(self, cx, st, q, node):
        eng = self.eng
        req = eng.sched.register(cx, st, q, node.where if node else "")
        lr = LazyReading(req, q, st)
        if not eng.flags.get("no_fill"):
            src = cx.unsure_source or {}
            cats = list(src.get("need") or q.lacks or [])
            if len(cats) >= 2:
                cq = companion_q(q, cats)
                lr.companion = LazyReading(eng.sched.register(cx, st, cq, "companion"), cq, st)
        return lr

    async def b_cut(self, cx, a, n):
        if not a:
            raise JxError("cut(读数, 线?)", n)
        r = a[0]
        if is_pend(r):
            return r
        if isinstance(r, list):
            raise JxError("cut 一次过一个读数；一组读数用 map(rs, fn(r) { cut(r) })", n)
        line = await self.deep(a[1], cx) if len(a) > 1 else None
        if isinstance(r, Reading):
            return core_cut(r, line)
        if not isinstance(r, LazyReading):
            raise JxError(f"cut 要一个读数，得到{type_name(r)}", n)
        return LazyExit(r, line, n)

    async def b_handle(self, cx, a, n):
        if len(a) < 2 or not isinstance(a[1], dict):
            raise JxError("handle(出口, {act: fn() {…}, ignore: …, pick: fn(k) {…}, at: fn(l) {…}, unsure: fn(u) {…}})", n)
        e, arms = a[0], a[1]
        if is_pend(e):
            return e
        ex = await self.force(e, cx, chain=("unsure" not in arms)) if isinstance(e, LazyExit) else await self.force(e, cx)
        if is_pend(ex):
            return ex
        if not isinstance(ex, Exit):
            raise JxError(f"handle 的第一个参数要是出口（cut 的结果），得到{type_name(ex)}", n)
        if ex.kind == "unsure" and "unsure" in arms:
            cx.pending_handled(ex)
            return await self.call(arms["unsure"], [ex], cx, n)
        arm = arms.get(ex.kind)
        if arm is None and ex.kind == "pick" and ex.value in arms:
            arm = arms[ex.value]
            return await self.call(arm, [], cx, n)
        if arm is None:
            return ex          # 没有对应臂：出口随值转交
        if ex.kind in ("pick", "at"):
            return await self.call(arm, [ex.value], cx, n)
        return await self.call(arm, [], cx, n)

    async def b_consume(self, cx, a, n):
        u = await self.force(a[0], cx, chain=False) if a else None
        route = a[1] if len(a) > 1 else "drop"
        if isinstance(u, Exit):
            cx.pending_handled(u)
            self.eng.ledger.add("consume", unit=cx.uid, route=route, cause=u.cause)
        return None

    async def b_unsure_source(self, cx, a, n):
        cx.unsure_source = a[0] if a and isinstance(a[0], dict) else None
        return None

    async def b_pending(self, cx, a, n):
        return [dict(p) for p in cx.pending]

    # ---- 出口谓词
    async def b_act(self, cx, a, n):
        e = await self.force(a[0], cx)
        return isinstance(e, Exit) and e.kind == "act"

    async def b_ignored(self, cx, a, n):
        e = await self.force(a[0], cx)
        return isinstance(e, Exit) and e.kind == "ignore"

    async def b_is_unsure(self, cx, a, n):
        e = await self.force(a[0], cx)
        return isinstance(e, Exit) and e.kind == "unsure"

    async def b_is_exit(self, cx, a, n):
        e = await self.force(a[0], cx)
        return isinstance(e, Exit)

    async def b_reading(self, cx, a, n):
        """出口所在块的读数（给人看；不参与跨题算术）。"""
        e = await self.force(a[0], cx)
        return exit_field(e, "p", n) if isinstance(e, Exit) else None

    # ---- 生成与执行
    async def b_gen(self, cx, a, n):
        args = [await self.deep(x, cx) for x in a]
        return self.eng.gen(cx, "gen", args, n)

    async def b_gen_json(self, cx, a, n):
        args = [await self.deep(x, cx) for x in a]
        if any(is_pend(x) for x in args):
            return Pend("waiting")
        return self.eng.gen(cx, "gen_json", args, n)

    async def b_do(self, cx, a, n):
        if not a:
            raise JxError("do(动作名, [参数…], 序号)", n)
        name = a[0]
        args = await self.deep(a[1], cx) if len(a) > 1 else []
        if not isinstance(args, list):
            args = [args]
        if any(is_pend(x) for x in args):
            self.eng.ledger.add("skip", unit=cx.uid, action=name, why="参数未决")
            return Pend("waiting")
        return await self.eng.do_action(cx, name, args, n)

    async def b_fail(self, cx, a, n):
        return Fail(show(a[0]) if a else "失败")

    async def b_is_fail(self, cx, a, n):
        return isinstance(await self.force(a[0], cx), Fail)

    async def b_is_pend(self, cx, a, n):
        return is_pend(await self.force(a[0], cx))

    # ---- 单元族读（内核读原语）
    async def b_peek_family(self, cx, a, n):
        fam = a[0].name if isinstance(a[0], Builtin) else a[0]
        return self.eng.read_family(cx, fam, a[1] if len(a) > 1 else None, n)

    async def b_members(self, cx, a, n):
        fam = a[0]
        return self.eng.family_keys(cx, fam, n)

    async def b_status(self, cx, a, n):
        h_ = a[0]
        if isinstance(h_, Handle):
            return self.eng.unit_status(cx, h_.name, h_.key)
        return self.eng.unit_status(cx, h_, tuple(a[1]) if len(a) > 1 else ())

    # ---- 高阶
    async def b_map(self, cx, a, n):
        xs, f = await self.force(a[0], cx), a[1]
        if is_pend(xs):
            return xs
        xs = need_list(xs, "map", n)
        return await self.amap(f, [[x] for x in xs], cx, n)

    async def amap(self, f, arglists, cx, n):
        """有效应的回调并发跑（各自登记，静止时一起发出）；纯回调顺序跑。"""
        if len(arglists) <= 1 or self.eng.flags.get("no_batch") or not self.may_block(f):
            return [await self.call(f, al, cx, n) for al in arglists]
        ws = [self.eng.fiber(self.call(f, al, cx, n), cx) for al in arglists]
        out = []
        for w in ws:
            out.append(await self.eng.wait(w, cx))
        return out

    async def b_filter(self, cx, a, n):
        xs = await self.force(a[0], cx)
        if is_pend(xs):
            return xs
        xs = need_list(xs, "filter", n)
        keep = await self.amap(a[1], [[x] for x in xs], cx, n)
        out = []
        for x, k in zip(xs, keep):
            k = await self.force(k, cx)
            if k is True:
                out.append(x)
        return out

    async def b_fold(self, cx, a, n):
        xs, acc, f = await self.force(a[0], cx), a[1], a[2]
        if is_pend(xs):
            return xs
        for x in need_list(xs, "fold", n):
            acc = await self.call(f, [acc, x], cx, n)
        return acc

    async def b_loop(self, cx, a, n):
        bound, acc, f = a[0], a[1], a[2]
        try:
            for i in range(int(bound)):
                acc = await self.call(f, [acc, i], cx, n)
        except Stop as s:
            return s.value
        return acc

    async def b_stop(self, cx, a, n):
        raise Stop(a[0] if a else None)

    async def b_iterate(self, cx, a, n):
        """iterate(初值, 上限, {patience, measure?}, fn(cur) → next)：不再进步 patience 轮即停（B132）。"""
        if len(a) < 4:
            raise JxError("iterate(初值, 上限轮数, {patience: k, measure?: fn(cur)}, fn(cur) { 下一步 })", n)
        cur, bound, opts, f = a[0], int(a[1]), a[2] or {}, a[3]
        patience = int(opts.get("patience", 2))
        meas = opts.get("measure")
        best = await self.force(await self.call(meas, [cur], cx, n), cx) if meas else None
        stall = 0
        try:
            for _ in range(bound):
                nxt = await self.deep(await self.call(f, [cur], cx, n), cx)
                if is_pend(nxt):
                    return nxt
                if meas:
                    m = await self.force(await self.call(meas, [nxt], cx, n), cx)
                    better = m is not None and (best is None or m > best)
                    if better:
                        best = m
                else:
                    better = canon(to_py(nxt)) != canon(to_py(cur))
                stall = 0 if better else stall + 1
                if better:
                    cur = nxt
                if stall >= patience:
                    break
        except Stop as s:
            return s.value
        return cur

    async def b_sort_by(self, cx, a, n):
        xs = await self.force(a[0], cx)
        if is_pend(xs):
            return xs
        xs = need_list(xs, "sort_by", n)
        ks = await self.amap(a[1], [[x] for x in xs], cx, n)
        ks = [await self.force(k, cx) for k in ks]
        idx = sorted(range(len(xs)), key=lambda i: sort_key(ks[i]))
        return [xs[i] for i in idx]

    async def b_any(self, cx, a, n):
        xs = need_list(await self.force(a[0], cx), "any", n)
        if len(a) > 1:
            xs = await self.amap(a[1], [[x] for x in xs], cx, n)
        for x in xs:
            if (await self.force(x, cx)) is True:
                return True
        return False

    async def b_all(self, cx, a, n):
        xs = need_list(await self.force(a[0], cx), "all", n)
        if len(a) > 1:
            xs = await self.amap(a[1], [[x] for x in xs], cx, n)
        for x in xs:
            if (await self.force(x, cx)) is not True:
                return False
        return True

    # ---- 纯函数
    async def b_len(self, cx, a, n):
        x = await self.force(a[0], cx)
        if is_pend(x):
            return x
        if isinstance(x, Mat):
            x = x.content
        if isinstance(x, Fail):
            raise JxError(f"len 得到一个失败值：{x.msg}", n)
        if x is None:
            return 0
        if not hasattr(x, "__len__"):
            raise JxError(f"len 要列表/文字/记录，得到{type_name(x)}", n)
        return len(x)

    async def b_keys(self, cx, a, n):
        x = await self.force(a[0], cx)
        return list(x.keys()) if isinstance(x, dict) else []

    async def b_values(self, cx, a, n):
        x = await self.force(a[0], cx)
        return list(x.values()) if isinstance(x, dict) else []

    async def b_items(self, cx, a, n):
        x = await self.force(a[0], cx)
        return [{"key": k, "value": v} for k, v in x.items()] if isinstance(x, dict) else []

    async def b_concat(self, cx, a, n):
        xs = [await self.force(x, cx) for x in a]
        for x in xs:
            if is_pend(x):
                return x
        if xs and all(isinstance(x, str) for x in xs):
            return "".join(xs)
        out = []
        for x in xs:
            if x is None:
                continue
            if not isinstance(x, list):
                raise JxError(f"concat 要列表（或全是文字），得到{type_name(x)}", n)
            out.extend(x)
        return out

    async def b_has(self, cx, a, n):
        x, k = await self.force(a[0], cx), await self.force(a[1], cx)
        if isinstance(x, dict):
            return k in x
        if isinstance(x, (list, str)):
            return k in x if not isinstance(x, list) else any(eq(k, y) for y in x)
        return False

    async def b_contains(self, cx, a, n):
        return await self.b_has(cx, a, n)

    async def b_get(self, cx, a, n):
        x, k = await self.force(a[0], cx), await self.force(a[1], cx)
        d = a[2] if len(a) > 2 else None
        if isinstance(x, dict):
            return x.get(k, d)
        if isinstance(x, list) and isinstance(k, int) and -len(x) <= k < len(x):
            return x[k]
        if isinstance(x, Mat) and isinstance(x.content, dict):
            return x.content.get(k, d)
        return d

    async def b_set(self, cx, a, n):
        """set(记录, 键, 值) → 新记录（不改原记录）。"""
        x = dict(await self.force(a[0], cx) or {})
        x[a[1]] = a[2]
        return x

    async def b_merge(self, cx, a, n):
        out = {}
        for x in a:
            x = await self.force(x, cx)
            if isinstance(x, dict):
                out.update(x)
        return out

    async def b_range(self, cx, a, n):
        a = [int(await self.force(x, cx)) for x in a]
        return list(range(*a))

    async def b_str(self, cx, a, n):
        return show(await self.deep(a[0], cx))

    async def b_json(self, cx, a, n):
        return canon(to_py(await self.deep(a[0], cx)))

    async def b_parse_json(self, cx, a, n):
        try:
            return json.loads(a[0])
        except Exception as e:
            return Fail(f"不是 JSON：{e}")

    async def b_hash(self, cx, a, n):
        return h(to_py(await self.deep(a[0], cx)))

    async def b_join(self, cx, a, n):
        xs = await self.force(a[0], cx)
        sep = a[1] if len(a) > 1 else ""
        return sep.join(show(x) for x in xs)

    async def b_split(self, cx, a, n):
        return a[0].split(a[1]) if len(a) > 1 else a[0].split()

    async def b_replace(self, cx, a, n):
        return a[0].replace(a[1], a[2])

    async def b_trim(self, cx, a, n):
        return a[0].strip()

    async def b_upper(self, cx, a, n):
        return a[0].upper()

    async def b_lower(self, cx, a, n):
        return a[0].lower()

    async def b_starts_with(self, cx, a, n):
        return isinstance(a[0], str) and a[0].startswith(a[1])

    async def b_substr(self, cx, a, n):
        s = a[0]
        i = int(a[1])
        j = int(a[2]) if len(a) > 2 else len(s)
        return s[i:j]

    async def b_round(self, cx, a, n):
        x = num(await self.force(a[0], cx), n)
        d = int(a[1]) if len(a) > 1 else 0
        r = round(x, d)
        return int(r) if d == 0 else r

    async def b_floor(self, cx, a, n):
        return math.floor(num(a[0], n))

    async def b_ceil(self, cx, a, n):
        return math.ceil(num(a[0], n))

    async def b_abs(self, cx, a, n):
        return abs(num(a[0], n))

    async def b_min(self, cx, a, n):
        xs = a[0] if len(a) == 1 and isinstance(a[0], list) else a
        xs = [await self.force(x, cx) for x in xs]
        return min(xs, key=sort_key) if xs else None

    async def b_max(self, cx, a, n):
        xs = a[0] if len(a) == 1 and isinstance(a[0], list) else a
        xs = [await self.force(x, cx) for x in xs]
        return max(xs, key=sort_key) if xs else None

    async def b_sum(self, cx, a, n):
        xs = await self.force(a[0], cx)
        return sum(num(x, n) for x in xs)

    async def b_not(self, cx, a, n):
        x = await self.force(a[0], cx)
        if is_pend(x):
            return x
        return not bool(x)

    async def b_slice(self, cx, a, n):
        xs = await self.force(a[0], cx)
        i = int(a[1])
        j = int(a[2]) if len(a) > 2 and a[2] is not None else len(xs)
        return xs[i:j]

    async def b_flatten(self, cx, a, n):
        xs = need_list(await self.force(a[0], cx), "flatten", n)
        out = []
        for x in xs:
            x = await self.force(x, cx)
            if isinstance(x, list):
                out.extend(x)
            elif x is not None:
                out.append(x)
        return out

    async def b_sort(self, cx, a, n):
        xs = [await self.force(x, cx) for x in (await self.force(a[0], cx))]
        return sorted(xs, key=sort_key)

    async def b_reverse(self, cx, a, n):
        return list(reversed(await self.force(a[0], cx)))

    async def b_unique(self, cx, a, n):
        xs = need_list(await self.force(a[0], cx), "unique", n)
        seen, out = set(), []
        for x in xs:
            k = canon(to_py(x))
            if k not in seen:
                seen.add(k)
                out.append(x)
        return out

    async def b_index_of(self, cx, a, n):
        xs, x = await self.force(a[0], cx), a[1]
        for i, y in enumerate(xs):
            if eq(x, y):
                return i
        return -1

    async def b_enumerate(self, cx, a, n):
        return [{"i": i, "x": x} for i, x in enumerate(await self.force(a[0], cx))]

    async def b_zip(self, cx, a, n):
        return [list(t) for t in zip(*[await self.force(x, cx) for x in a])]

    async def b_type(self, cx, a, n):
        return type_name(await self.force(a[0], cx))

    async def b_is_list(self, cx, a, n):
        return isinstance(await self.force(a[0], cx), list)

    async def b_is_record(self, cx, a, n):
        return isinstance(await self.force(a[0], cx), dict)

    async def b_is_str(self, cx, a, n):
        return isinstance(await self.force(a[0], cx), str)

    async def b_print(self, cx, a, n):
        self.eng.ledger.add("print", unit=cx.uid, text=show(to_py(await self.deep(a[0], cx))))
        return None

    # ============================================================ 效应分析
    def may_block(self, f) -> bool:
        if isinstance(f, Builtin):
            return f.name in EFFECT_BUILTINS
        if not isinstance(f, Closure):
            return True
        return self._fn_effectful(f.fn, set())

    def _fn_effectful(self, fn: A.FnLit, seen) -> bool:
        if fn.nid in self._eff_memo:
            return self._eff_memo[fn.nid]
        if fn.nid in seen:
            return False
        seen.add(fn.nid)
        found = [False]
        local = set(fn.params)

        def visit(x):
            if found[0]:
                return
            if isinstance(x, A.Let):
                local.add(x.name)
            elif isinstance(x, A.Call):
                f = x.fn
                if not isinstance(f, A.Var):
                    found[0] = True
                    return
                nm = f.name
                if nm in local:
                    found[0] = True
                elif nm in EFFECT_BUILTINS:
                    found[0] = True
                elif nm in self.global_fns:
                    if self._fn_effectful(self.global_fns[nm], seen):
                        found[0] = True
                elif nm in self.builtins:
                    # 高阶内置的回调若是名字（不是字面函数），按名字查
                    for arg in x.args:
                        if isinstance(arg, A.Var) and arg.name not in self.builtins:
                            g = self.global_fns.get(arg.name)
                            if g is None or self._fn_effectful(g, seen):
                                if g is not None or arg.name in local:
                                    found[0] = True
                else:
                    found[0] = True
        A.walk(fn.body, visit)
        self._eff_memo[fn.nid] = found[0]
        return found[0]


# ================================================================ 工具

def companion_q(q: Question, cats) -> Question:
    opts = {f"c{i}": c for i, c in enumerate(cats)}
    opts["none"] = NONE_CAT
    return q_select(f"要更有把握地回答「{q.text}」，材料里最缺哪一类信息？", opts, "companion:" + (q.key or "q"))


class _Const(A.Node):
    def __init__(self, v):
        super().__init__()
        self.v = v


def has_lazy(v, depth=0) -> bool:
    if isinstance(v, (LazyExit, LazyReading, Lazy, Pend)):
        return True
    if depth > 20:
        return False
    if isinstance(v, dict):
        return any(has_lazy(x, depth + 1) for x in v.values())
    if isinstance(v, (list, tuple)):
        return any(has_lazy(x, depth + 1) for x in v)
    if isinstance(v, Mat):
        return has_lazy(v.content, depth + 1)
    return False


def mats_in(parts, acc=None):
    acc = [] if acc is None else acc
    for v in parts:
        if isinstance(v, Mat):
            acc.append(v)
            mats_in([v.content], acc)
        elif isinstance(v, dict):
            mats_in(list(v.values()), acc)
        elif isinstance(v, (list, tuple)):
            mats_in(list(v), acc)
    return acc


def strip_flags(rv):
    if isinstance(rv, dict):
        return {k: v for k, v in rv.items() if k not in ("granted", "denied")}
    return rv


def _qargs(a, start, n):
    if not a or not isinstance(a[0], str):
        raise JxError("题面要是文字", n)
    key, lacks = "", ()
    rest = list(a[start:])
    for x in rest:
        if isinstance(x, str):
            key = x
        elif isinstance(x, list):
            lacks = tuple(show(c) for c in x)
        elif isinstance(x, dict):
            lacks = tuple(show(c) for c in x.get("lacks", lacks))
            key = x.get("key", key)
    return a[0], key, lacks


def exit_field(e: Exit, name, node=None):
    if name == "kind":
        return e.kind
    if name in ("act", "ignore", "unsure"):
        return e.kind == name
    if name in ("value", "label"):
        return e.value
    if name == "cause":
        return e.cause or None
    if name == "grade":
        return e.grade
    if name in ("p", "conf"):
        return round(e.conf, 4)
    if name == "q":
        return e.reading.q.text if e.reading else None
    if name == "key":
        return e.reading.q.key if e.reading else None
    if name == "needed":
        return e.needed
    if name == "lean":
        return e.lean
    if name == "waiting":
        return e.waiting
    if name == "ask_to":
        return list(e.ask_to)
    if name == "pick":
        return e.value if e.kind == "pick" else None
    if name == "at":
        return e.value if e.kind == "at" else None
    raise JxError(f"出口没有字段 `{name}`（有 kind/act/ignore/unsure/value/cause/grade/p/q/key/needed/lean）", node)


def num(x, node=None):
    if isinstance(x, bool) or not isinstance(x, (int, float)):
        raise JxError(f"要数，得到{type_name(x)}", node)
    return x


def eq(a, b) -> bool:
    if isinstance(a, Exit) or isinstance(b, Exit):
        return a is b
    try:
        return canon(to_py(a)) == canon(to_py(b))
    except Exception:
        return a == b


def show(x) -> str:
    if isinstance(x, str):
        return x
    if x is None:
        return "unit"
    if isinstance(x, bool):
        return "true" if x else "false"
    if isinstance(x, float) and x.is_integer():
        return str(int(x))
    if isinstance(x, (int, float)):
        return str(x)
    return canon(to_py(x))


def sort_key(k):
    if isinstance(k, bool):
        return (0, int(k))
    if isinstance(k, (int, float)):
        return (0, k)
    if isinstance(k, str):
        return (1, k)
    if k is None:
        return (2, 0)
    if isinstance(k, list):
        return (3, tuple(sort_key(x) for x in k))
    return (4, canon(to_py(k)))
