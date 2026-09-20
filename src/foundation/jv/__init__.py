"""jv：以判断为一等效应的通用语言，Python 构建器表面（12-IR与类契约-v0.1 §6）。

    import foundation.jv as jv
    with jv.Runtime(client=jv.FakeClient()) as rt:
        @jv.program(budget=jv.Budget(calls=40))
        def f(x): ...
        f(...)
"""

from __future__ import annotations

import functools
from typing import Any

from . import ir as _ir
from .calib import CalibRecord, CalibStore, FitRegistry
from .client import FakeClient, JevClient
from .ir import (Act, Action, At, Budget, CalibRef, Escalated, Exit, Fail, FitRef, Ignore, JvError, JvTypeError,
                 Mat, Pending, Pick, Q, Reading, Readings, ReadingsVec, State, Unsure, lit)
from .runtime import (Runtime, current, decreasing, drop, escalate, prior, provisional, _Score)
from .checker import check, CheckReport
from .plan import plan, PlanReport, Sym

__all__ = ["Runtime", "FakeClient", "JevClient", "Budget", "Action", "Mat", "lit", "state", "test", "select",
           "measure", "judge", "cut", "fit", "gen", "do", "ask", "transform", "loop", "decreasing", "handle",
           "consume", "on_truth", "on_fail", "program", "calib", "fitref", "anchors", "prior", "Act", "Ignore",
           "Unsure", "Pick", "At", "drop", "escalate", "provisional", "Pending", "Fail", "Escalated", "JvError",
           "JvTypeError", "check", "current", "use", "mat", "plan", "PlanReport", "Sym", "stats"]


def stats() -> dict:
    """当前运行时最近一条程序的统计（层数、每层题数/调用数、融合率、账本命中、钱）。"""
    return current().stats_report()


# ---------------------------------------------------------------- 题构造子
def calib(key: str) -> CalibRef:
    return CalibRef(key)


def fitref(name: str) -> FitRef:
    return FitRef(name)


def anchors(name: str) -> str:
    return name


def test(text: str, *, calib: CalibRef, evidence: tuple | list = (), agg: str = "exists") -> Q:
    _need_calib(calib)
    return Q(op="test", text=text, calib=calib, evidence=tuple(evidence), agg=agg)


def select(text: str, *, calib: CalibRef, prior: str = "none", evidence: tuple | list = (), phys: str | None = None) -> Q:
    _need_calib(calib)
    return Q(op="select", text=text, calib=calib, prior=prior, evidence=tuple(evidence), phys=phys)


def measure(text: str, *, scale, anchors: str | None = None, calib: CalibRef, evidence: tuple | list = ()) -> Q:
    _need_calib(calib)
    return Q(op="measure", text=text, calib=calib, scale=tuple(scale), anchors=anchors, evidence=tuple(evidence))


def _need_calib(c):
    if not isinstance(c, CalibRef):
        raise JvTypeError("J-03: calib 必须是 jv.calib(\"键\") 返回的 CalibRef，不能是字符串或数字（线不可字面）。")


# ---------------------------------------------------------------- 效应与桥（转发到当前运行时）
def mat(content: Any, addr: str = "") -> Mat:
    return lit(content, addr)


def state(on, ctx=None, ref=None, over=None, repr="json") -> State:
    return Runtime.state(on, ctx, ref, over, repr)


def judge(s, *qs: Q):
    return current().judge(s, *qs)


def cut(r, cost: tuple | None = None, calib: CalibRef | None = None):
    if isinstance(calib, str):
        raise JvTypeError("J-03: cut 的校准参数必须是 CalibRef。修法：jv.calib(\"键\")，或把 calib 挂在题上。")
    if isinstance(r, _Score):
        if calib is None:
            raise JvError("fit 的分数必须 cut(score, calib=jv.calib(...)) 才能出口（§2.9-4）")
        return current().cut_score(r, calib)
    if isinstance(r, (int, float, str)) or isinstance(cost, str):
        raise JvTypeError("J-01/J-03: cut 只接受读数；线不可字面。")
    return current().cut(r, cost)


def fit(ref: FitRef, *rs):
    return current().fit(ref, *rs)


def gen(prompt: str, ctx=None, n: int = 4, retry_seq: int | None = None, generator=None):
    return current().gen(prompt, ctx=ctx, n=n, retry_seq=retry_seq, generator=generator)


def do(action: Action, *args, iter_seq: int | None = None, guard=None):
    return current().do(action, *args, iter_seq=iter_seq, guard=guard)


def ask(s: State, q: Q):
    return current().ask(s, q)


def transform(f, *args):
    return current().transform(f, *args)


def loop(bound: int | None = None, variant=None):
    return current().loop(bound, variant)


def handle(c, then=None, regen: bool = False, **kw):
    return current().handle(c, then=then, regen=regen, **kw)


def consume(exits, unsure=drop):
    return current().consume(exits, unsure=unsure)


def on_truth(key: str, fn):
    return current().on_truth(key, fn)


def on_fail(expr, alt):
    return current().on_fail(expr, alt)


def use(rt: Runtime) -> Runtime:
    return rt.use()


# ---------------------------------------------------------------- program
def program(budget: Budget | None = None, *, check_static: bool = True):
    """装饰器：进入时静态检查（一次）、开账本；返回前刷新、核 J-05、落账本。Pending 原样抛出。"""
    budget = budget or Budget()

    def deco(fn):
        report_holder: dict = {}

        @functools.wraps(fn)
        def wrapper(*args, **kwargs):
            rt = current()
            if check_static and "report" not in report_holder:
                report_holder["report"] = check(fn)
            rep: CheckReport | None = report_holder.get("report")
            frame = rt.begin(fn.__name__, budget)                # 最外层开账本；内层压子账帧
            try:
                if rep is not None:
                    for w in rep.warnings:
                        rt.warn(w)
                    if rep.errors:
                        raise JvError(f"静态检查未过（{fn.__name__}）：\n" + "\n".join(rep.errors))
                if rt.passes.get("plan", True) and "plan" not in report_holder:      # 计划期估计（J-07 / J-10），只告警
                    try:
                        report_holder["plan"] = plan(fn, budget=budget, rt=rt)
                    except Exception as e:                                         # 估计器不得挡程序
                        report_holder["plan"] = None
                        rt.warn(f"W-plan-fail: 计划期估计出错 {type(e).__name__}: {e}")
                prep = report_holder.get("plan")
                if prep is not None and rt.passes.get("plan", True):
                    for w in prep.warnings:
                        rt.warn(w)
                    rt.stats.setdefault("plan", {})[fn.__name__] = {
                        "calls": repr(prep.calls), "layers": repr(prep.layers), "unsure_bound": repr(prep.unsure_bound)}
                result = fn(*args, **kwargs)
            except Pending:
                rt.flush(reason="pending")
                if rt.books and frame.parent is None:
                    rt.books.save()
                rt.abort_frame()
                raise
            except BaseException:
                rt.abort_frame()
                raise
            return rt.end(result)

        wrapper.__jv_program__ = True
        wrapper.__jv_budget__ = budget
        wrapper.__jv_fn__ = fn
        wrapper.check = lambda: check(fn)
        wrapper.plan = lambda rt=None: plan(fn, budget=budget, rt=rt)
        return wrapper

    return deco
