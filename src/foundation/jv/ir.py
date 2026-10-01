"""六形式 IR 的数据结构（12-IR与类契约-v0.1 §2）。

只有数据与类型纪律，没有执行：Mat / Q / State / Readings / Exit 族 / Fail / Pending /
CalibRef / FitRef / Action / Budget。J-01 在这里用 Python 类型层拦：Reading 与 Readings 没有
`__float__`、`__lt__`、`__gt__`、`__add__`，碰到即 JvTypeError。
"""

from __future__ import annotations

import dis
import sys

import inspect
import os
from dataclasses import dataclass, field
from typing import Any, Iterable

from foundation.core.canon import H, canon

TRUSTED, UNTRUSTED = "trusted", "untrusted"
OPS = ("test", "select", "measure")
CAUSES = ("band", "tie", "cold", "insufficient", "fail", "budget", "noprogress", "drift", "taint")
RENDER_VERSION = "r1"


class JvError(Exception):
    """检查器与运行期纪律错误。报文格式：`J-xx: 一句话。修法：…`。"""


class JvTypeError(JvError, TypeError):
    pass


class Pending(Exception):
    """`ask` 无答案时的程序级出口（§2.6）：程序整体挂起，恢复 = 重放。"""

    def __init__(self, key: str, state_hash: str, q_hash: str):
        super().__init__(f"Pending: ask 等待人答 key={key}")
        self.key, self.state_hash, self.q_hash = key, state_hash, q_hash


def _is(x: Any, cls) -> bool:
    """内部用的类型检查：不经 __instancecheck__，因此不会把出口标成「已消费」。"""
    return issubclass(type(x), cls)


def _join_taint(taints: Iterable[str]) -> str:
    return UNTRUSTED if any(t == UNTRUSTED for t in taints) else TRUSTED


def site_of(depth_hint: int = 0) -> str:
    """调用点：第一个不在 jv 包内的栈帧，`文件名:行号`。同程序重放时稳定。"""
    here = os.path.dirname(os.path.abspath(__file__))
    for fr in inspect.stack()[1:]:
        fn = os.path.abspath(fr.filename)
        if not fn.startswith(here):
            return f"{os.path.basename(fn)}:{fr.lineno}"
    return "?:0"


# ---------------------------------------------------------------- Mat

@dataclass(frozen=True, eq=False)
class Mat:
    """唯一材料类型（§2）。不可变。

    相等与哈希只看内容 + 地址 + 模态 + 渲染版本（`hash` 属性），不看来源链与 taint：
    同一段文字从两处取来是「同一份材料」，可以进 set / dict / `in`。
    Mat 不是字符串：`"x" in m`、`for ch in m`、`m == "x"` 都是类型错，要看内容用 `m.content` / `m.text()`。
    """
    content: Any                                   # str | dict | list
    addr: str = ""
    modality: str = "text"
    origin: tuple = ()                             # ("lit",) | ("do", site, key) | ("gen", …) | ("transform", …) | ("cut", q_hash)
    taint: str = TRUSTED
    derived_from: frozenset = frozenset()          # 由哪些题派生（J-02）
    render_version: str = RENDER_VERSION

    @property
    def hash(self) -> str:
        return H(self.content, self.addr, self.modality, self.render_version)

    @property
    def tokens(self) -> int:
        """估算 token：按 profile cost.regression 的 state_char_coef≈1 的量级，1.3 字符/token。"""
        s = self.content if isinstance(self.content, str) else canon(self.content)
        return int(len(s) / 1.3) + 1

    def text(self) -> str:
        return self.content if isinstance(self.content, str) else canon(self.content)

    def __repr__(self) -> str:
        s = self.text()
        return f"Mat({s[:40]!r}{'…' if len(s) > 40 else ''}, taint={self.taint}, origin={self.origin[:1]})"

    def __eq__(self, other):
        if isinstance(other, Mat):
            return self.hash == other.hash
        if isinstance(other, _Future):
            return self.hash == other.resolve().hash
        if isinstance(other, (str, int, float, dict, list)):
            raise JvTypeError("Mat 不能与裸值比较。修法：m.content == 值，或 m.text() == 字符串。")
        return NotImplemented

    def __hash__(self):
        return hash(self.hash)

    def __contains__(self, item):
        raise JvTypeError("Mat 不是字符串，`x in m` 无定义。修法：x in m.content（或 m.text()）。")

    def __iter__(self):
        raise JvTypeError("Mat 不可迭代；它是一份材料不是字符串/列表。修法：用 m.content。")

    def __len__(self):
        raise JvTypeError("Mat 无长度。修法：len(m.content)。")


def lit(content: Any, addr: str = "", modality: str = "text") -> Mat:
    """字面量材料：trusted，来源 ("lit",)。"""
    return Mat(content=content, addr=addr, modality=modality, origin=("lit",), taint=TRUSTED)


# ---------------------------------------------------------------- CalibRef / FitRef / Action / Budget

@dataclass(frozen=True)
class CalibRef:
    """指向校准记录（§2.3）。线不在这里，线在 store 的记录里；这里只有键。"""
    key: str

    def __repr__(self) -> str:
        return f"calib({self.key!r})"


@dataclass(frozen=True)
class FitRef:
    name: str


@dataclass(frozen=True)
class Action:
    """`do` 的动作声明（§2.5）。"""
    name: str
    fn: Any = None                              # 宿主可调用；None 时按 name 从 registry 查
    cost: float = 0.0
    latency: float = 0.0
    reversible: bool = True
    fail_types: tuple = ()
    taint_out: str = "inherit"                  # trusted | untrusted | inherit
    registered: bool = False                    # 经 jv.register_action 登记（可信来自来源登记，W-self-trusted）
    reason: str = ""                            # 登记时给的「为什么它的输出可信/不可信」

    def __post_init__(self):
        if self.taint_out not in ("trusted", "untrusted", "inherit"):
            raise JvError(f"Action.taint_out 只能是 trusted|untrusted|inherit：{self.taint_out}")


ACTIONS: dict = {}                                  # S 库动作注册表：name → Action


def register_action(name: str, fn: Any = None, *, taint_out: str = "inherit", reason: str = "",
                    cost: float = 0.0, latency: float = 0.0, reversible: bool = True, fail_types: tuple = ()) -> Action:
    """S 库登记一个动作（§2.5、§2.11）。taint_out="trusted" 必须给 reason（可信来自来源登记，不来自程序自报）。"""
    if taint_out == "trusted" and not reason.strip():
        raise JvError(f"register_action({name!r}): taint_out=trusted 必须给 reason（这个执行器的输出为什么可信）。"
                      f"修法：jv.register_action(name, fn, taint_out='trusted', reason='仓库自带测试框架，输出不含用户文本')")
    a = Action(name=name, fn=fn, cost=cost, latency=latency, reversible=reversible, fail_types=tuple(fail_types),
               taint_out=taint_out, registered=True, reason=reason)
    ACTIONS[name] = a
    return a


@dataclass(frozen=True)
class Budget:
    calls: int | None = None
    cost: float | None = None
    layers: int | None = None
    escalate: int | None = None
    unsure: float | None = None

    def to_dict(self) -> dict:
        return {"calls": self.calls, "cost": self.cost, "layers": self.layers,
                "escalate": self.escalate, "unsure": self.unsure}


# ---------------------------------------------------------------- Q

@dataclass(frozen=True)
class Q:
    """题（§2.2）。op 三种；题式由 op × slot_shape 推断，不进 IR。"""
    op: str
    text: str
    calib: CalibRef
    scale: tuple = ()                           # measure 的档位名
    anchors: str | None = None                  # measure 的锚集名
    evidence: tuple = ()                        # J-09：决定性证据槽名（如 "ctx"）
    prior: str = "none"                         # select 策略：none | pass_count
    phys: str | None = None                     # 显式物理形式（一般由下沉决定）
    agg: str = "exists"                         # 裂变合回：test → exists | all

    def __post_init__(self):
        if self.op not in OPS:
            raise JvError(f"Q.op 只能是 test|select|measure：{self.op}")
        if not isinstance(self.calib, CalibRef):
            raise JvTypeError("J-03: cut 的校准参数必须是 CalibRef，线不可字面。"
                              "修法：用 jv.calib(\"键\")，或把 calib 挂在题上。")
        if self.op == "measure" and len(self.scale) < 2:
            raise JvError("measure 题必须给 scale（≥ 2 档）")

    @property
    def text_hash(self) -> str:
        return H(self.op, self.text, list(self.scale))

    def __repr__(self) -> str:
        return f"Q({self.op}, {self.text[:24]!r})"


# ---------------------------------------------------------------- State

class _Future:
    """惰性材料句柄的共同基类（`do` 的期物）。运行时负责解析。"""
    _rt = None
    _resolved: Mat | None = None

    def resolve(self) -> Mat:
        if self._resolved is None:
            if self._rt is None:
                raise JvError("期物没有运行时，无法解析")
            self._rt.flush(reason="content")
            if self._resolved is None:
                raise JvError("刷新后期物仍未解析（依赖环或执行失败）")
        return self._resolved

    @property
    def content(self):
        return self.resolve().content

    @property
    def taint(self):
        return self.resolve().taint

    def text(self) -> str:
        return self.resolve().text()

    def __getattr__(self, name):
        if name.startswith("_"):
            raise AttributeError(name)
        return getattr(self.resolve(), name)

    def __eq__(self, other):
        if isinstance(other, _Future):                  # 期物之间按身份比（运行时内部用，不触发刷新）
            return self is other
        if isinstance(other, Mat):
            return self.resolve() == other
        if isinstance(other, (str, int, float, dict, list)):
            raise JvTypeError("期物不能与裸值比较。修法：f.content == 值（读 .content 是刷新点）。")
        return NotImplemented

    def __hash__(self):
        return id(self)

    def __contains__(self, item):
        raise JvTypeError("期物不是字符串，`x in f` 无定义。修法：x in f.content（读 .content 是刷新点）。")

    def __iter__(self):
        raise JvTypeError("期物不可迭代。修法：用 f.content。")


MatLike = Mat | _Future


def _check_mat(x: Any, slot: str) -> None:
    if isinstance(x, (Mat, _Future)):
        return
    if _is(x, Exit):
        return
    raise JvTypeError(f"J-11: 槽 {slot} 收到的不是材料（{type(x).__name__}）；进槽的材料只能来自字面量、"
                      f"IR 形式的输出或 jv.transform 的输出。修法：字面量用 jv.lit(...)，宿主函数经 jv.transform(f, ...)。")


@dataclass
class State:
    """槽构造子的结果（§2.1）。惰性：槽里可含期物，调用前由运行时解析。"""
    on: Any                                     # MatLike | (MatLike, MatLike)
    ctx: list = field(default_factory=list)
    ref: list = field(default_factory=list)
    over: list = field(default_factory=list)
    repr: str = "json"

    def __post_init__(self):
        if isinstance(self.on, (list, tuple)):
            if len(self.on) != 2:
                raise JvError("J-14: on 恰一个判断对象；关系用 on=(a, b) 的二元复合对象。修法：拆成多个 state 或用 over。")
            for x in self.on:
                _check_mat(x, "on")
        else:
            _check_mat(self.on, "on")
        for name in ("ctx", "ref", "over"):
            xs = getattr(self, name)
            if not isinstance(xs, list):
                raise JvError(f"J-14: 槽 {name} 必须是列表")
            for x in xs:
                _check_mat(x, name)

    # —— 解析（运行时调用）——
    def _mats(self) -> dict:
        def r(x):
            if isinstance(x, _Future):
                return x.resolve()
            if _is(x, Exit):
                return x.as_mat()
            return x
        on = tuple(r(x) for x in self.on) if isinstance(self.on, (list, tuple)) else r(self.on)
        return {"on": on, "ctx": [r(x) for x in self.ctx], "ref": [r(x) for x in self.ref],
                "over": [r(x) for x in self.over]}

    def resolved(self) -> "ResolvedState":
        m = self._mats()
        return ResolvedState(on=m["on"], ctx=m["ctx"], ref=m["ref"], over=m["over"], repr=self.repr)

    def is_ready(self) -> bool:
        xs = list(self.on) if isinstance(self.on, (list, tuple)) else [self.on]
        xs += self.ctx + self.ref + self.over
        return all(not isinstance(x, _Future) or x._resolved is not None for x in xs)

    def futures(self) -> list:
        xs = list(self.on) if isinstance(self.on, (list, tuple)) else [self.on]
        xs += self.ctx + self.ref + self.over
        return [x for x in xs if isinstance(x, _Future) and x._resolved is None]


@dataclass(frozen=True)
class ResolvedState:
    on: Any
    ctx: tuple
    ref: tuple
    over: tuple
    repr: str = "json"

    def __post_init__(self):
        object.__setattr__(self, "ctx", tuple(self.ctx))
        object.__setattr__(self, "ref", tuple(self.ref))
        object.__setattr__(self, "over", tuple(self.over))

    @property
    def all_mats(self) -> list[Mat]:
        on = list(self.on) if isinstance(self.on, tuple) else [self.on]
        return on + list(self.ctx) + list(self.ref) + list(self.over)

    @property
    def taint(self) -> str:
        return _join_taint(m.taint for m in self.all_mats)

    @property
    def derived_from(self) -> frozenset:
        s: set = set()
        for m in self.all_mats:
            s |= set(m.derived_from)
        return frozenset(s)

    @property
    def size_tokens(self) -> int:
        return sum(m.tokens for m in self.all_mats)

    def slots(self, over_perm: list[int] | None = None) -> dict:
        """渲染为 JSON 具名槽（H1、P24）。over_perm 给候选顺序（置换）。"""
        def c(m: Mat):
            return m.content
        d: dict = {}
        if isinstance(self.on, tuple):
            d["on"] = {"a": c(self.on[0]), "b": c(self.on[1])}
        else:
            d["on"] = c(self.on)
        if self.ctx:
            d["ctx"] = [c(m) for m in self.ctx]
        if self.ref:
            d["ref"] = [c(m) for m in self.ref]
        if self.over:
            order = over_perm if over_perm is not None else list(range(len(self.over)))
            d["over"] = {f"c{i}": c(self.over[j]) for i, j in enumerate(order)}
        return d

    def render(self, over_perm: list[int] | None = None) -> Any:
        if self.repr == "json":
            return self.slots(over_perm)
        s = self.slots(over_perm)
        parts = []
        on = s["on"]
        parts.append("[对象]\n" + (on if isinstance(on, str) else canon(on)))
        for k in ("ctx", "ref"):
            for i, x in enumerate(s.get(k, [])):
                parts.append(f"[{k}{i}]\n" + (x if isinstance(x, str) else canon(x)))
        for k, x in s.get("over", {}).items():
            parts.append(f"[{k}]\n" + (x if isinstance(x, str) else canon(x)))
        return "\n\n".join(parts)

    @property
    def structure_hash(self) -> str:
        """含槽结构的规范化哈希（§2.10：on=A,ctx=[B] 与 on=B,ctx=[A] 不同键）。"""
        return H(self.slots(), self.repr)

    @property
    def slot_kinds(self) -> str:
        return "+".join(k for k in ("on", "ctx", "ref", "over") if getattr(self, k)) + \
               ("(pair)" if isinstance(self.on, tuple) else "")


# ---------------------------------------------------------------- Readings

class _NoArith:
    """J-01：读数不是数。"""

    def _j01(self, what: str):
        raise JvTypeError(f"J-01: 读数不可{what}；读数只能经 jv.cut 或 jv.fit 离开。"
                          f"修法：同题跨对象用 .order()，同题跨运行用 .agg()，出口用 jv.cut(r)。")

    def __float__(self):
        self._j01("转数")

    def __int__(self):
        self._j01("转数")

    def __lt__(self, o):
        self._j01("比较")

    def __gt__(self, o):
        self._j01("比较")

    def __le__(self, o):
        self._j01("比较")

    def __ge__(self, o):
        self._j01("比较")

    def __add__(self, o):
        self._j01("做算术")

    def __radd__(self, o):
        self._j01("做算术")

    def __sub__(self, o):
        self._j01("做算术")

    def __mul__(self, o):
        self._j01("做算术")

    def __truediv__(self, o):
        self._j01("做算术")

    def __bool__(self):
        self._j01("当真值用")


class Reading(_NoArith):
    """一题一状态的读数（惰性）。运行时在刷新时填 `_ans`。"""

    def __init__(self, effect, q_index: int, obj_index: int = 0):
        self.effect = effect
        self.q_index = q_index
        self.obj_index = obj_index
        self._ans: dict | None = None           # {"phys", "p", "value", "probs", "perm_seeds", "runs": [...]}
        self.q: Q = effect.qs[q_index]
        self.run_seq = 0

    @property
    def ready(self) -> bool:
        return self._ans is not None

    def _need(self) -> dict:
        if self._ans is None:
            self.effect.rt.flush(reason="cut")
            if self._ans is None:
                raise JvError("刷新后读数仍未就绪")
        return self._ans

    @property
    def fingerprint(self) -> str:
        """J-04：题 + 候选集/刻度指纹种类。"""
        return H(self.q.text_hash, self.q.op, len(self.effect.states[self.obj_index].over)
                 if self.q.op == "select" else list(self.q.scale))

    def agg(self) -> "Reading":
        """同题跨运行：把已有 run 的读数按均值（noul/score）或众数（choice）合并成一条读数。"""
        a = self._need()
        runs = a.get("runs") or [a]
        if len(runs) <= 1:
            return self
        merged = _merge_runs(runs, a["phys"])
        merged["runs"] = runs
        r = Reading(self.effect, self.q_index, self.obj_index)
        r._ans = merged
        return r

    def __repr__(self):
        return f"Reading({self.q!r}, {'ready' if self.ready else 'lazy'})"


def _merge_runs(runs: list[dict], phys: str) -> dict:
    n = len(runs)
    if phys == "noul":
        p = sum(r["p"] for r in runs) / n
        return {"phys": phys, "p": p, "value": p, "probs": {"noul": p}}
    if phys == "score":
        from collections import Counter
        lv = Counter(r["value"] for r in runs).most_common(1)[0][0]
        p = sum(r["probs"].get(str(lv), 0.0) for r in runs) / n
        return {"phys": phys, "p": p, "value": lv, "probs": runs[0]["probs"]}
    from collections import Counter
    opt = Counter(r["value"] for r in runs).most_common(1)[0][0]
    p = sum(r["probs"].get(opt, 0.0) for r in runs) / n
    return {"phys": phys, "p": p, "value": opt, "probs": runs[0]["probs"],
            "mode_share": sum(1 for r in runs if r["value"] == opt) / n}


class Readings(_NoArith):
    """`jv.judge(s, q1, q2, …)` 的结果：一状态多题，按题下标取 Reading。"""

    def __init__(self, effect, obj_index: int = 0):
        self.effect = effect
        self.obj_index = obj_index
        self._items = [Reading(effect, i, obj_index) for i in range(len(effect.qs))]

    def __getitem__(self, i: int) -> Reading:
        return self._items[i]

    def __len__(self):
        return len(self._items)

    def __iter__(self):
        return iter(self._items)

    def agg(self) -> "Readings":
        r = Readings.__new__(Readings)
        r.effect, r.obj_index = self.effect, self.obj_index
        r._items = [x.agg() for x in self._items]
        return r

    def order(self):
        raise JvTypeError("J-01: .order() 只对向量化读数（jv.judge([s1, s2, …], q)）有定义。")

    def __repr__(self):
        return f"Readings({len(self._items)} 题)"


class ReadingsVec(_NoArith):
    """`jv.judge([s1, s2, …], q)` 的结果：同题跨对象，同层并发。"""

    def __init__(self, effect):
        self.effect = effect
        self._items = [Readings(effect, k) for k in range(len(effect.states))]

    def __getitem__(self, k: int) -> Readings:
        return self._items[k]

    def __len__(self):
        return len(self._items)

    def __iter__(self):
        return iter(self._items)

    def agg(self) -> "ReadingsVec":
        v = ReadingsVec.__new__(ReadingsVec)
        v.effect = self.effect
        v._items = [x.agg() for x in self._items]
        return v

    def order(self, q_index: int = 0) -> list[list[int]]:
        """同题同锚单独渲染的跨对象偏序：按 p 降序，相邻差 ≤ δ 的并列成一档（§2.2）。"""
        if len(self.effect.qs) != 1 and q_index is None:
            raise JvError(".order() 需要指定 q_index")
        rs = [it[q_index] for it in self._items]
        for r in rs:
            r._need()
        delta = self.effect.rt.delta_for(rs[0]._ans["phys"]) if rs else 0.0
        keyed = sorted(range(len(rs)), key=lambda k: -_rank_value(rs[k]._ans))
        tiers: list[list[int]] = []
        failed: list[int] = []
        for k in keyed:
            v = _rank_value(rs[k]._ans)
            if v == float("-inf"):                       # 失败 / 停止的读数（J-12）单独一档排最后
                failed.append(k)
                continue
            if tiers and abs(_rank_value(rs[tiers[-1][-1]]._ans) - v) <= delta:
                tiers[-1].append(k)
            else:
                tiers.append([k])
        if failed:
            tiers.append(failed)
        return tiers


def _rank_value(ans: dict) -> float:
    """排序键。失败/停止的读数（J-12：值不是数）排最后一档，不抛 TypeError。"""
    if ans.get("fail") or ans.get("stop") or ans.get("value") is None and ans["phys"] == "score":
        return float("-inf")
    if ans["phys"] == "score":
        return float(ans["value"]) + float(ans["p"]) * 0.001
    return float(ans["p"])


# ---------------------------------------------------------------- Exit

class _ExitMeta(type):
    """`isinstance(e, jv.Act)` 与 `match e: case jv.Act():` 命中即消费（J-05）。

    CPython 的 isinstance / MATCH_CLASS 在「类型完全相等」时走快路径、不调 __instancecheck__，
    所以 Act() 实际构造的是隐藏子类 _ActImpl 的实例，让每次检查都经过这里。
    """
    _impls: dict = {}

    def __call__(cls, *args, **kw):
        impl = _ExitMeta._impls.get(cls)
        if impl is None:
            impl = type.__new__(_ExitMeta, cls.__name__ + "Impl", (cls,), {"__module__": cls.__module__,
                                                                            "_public": cls})
            _ExitMeta._impls[cls] = impl
        return type.__call__(impl, *args, **kw)

    def __instancecheck__(cls, inst):
        if isinstance(inst, _NoArith):                 # 读数 / 读数向量当出口 match（§6.3 模式 1，向量元素形态）
            raise JvTypeError(f"J-01: 把读数 {type(inst).__name__} 当出口用（match / isinstance jv.{cls.__name__}）；"
                              f"读数不是出口。修法：先 jv.cut(r) 得出口，向量用 jv.cut(jv.judge([...], q)) 再解包。")
        ok = type.__instancecheck__(cls, inst)
        if ok and isinstance(getattr(inst, "__dict__", None), dict):
            # J-05：只有 match / jv.handle / jv.consume 消费 Unsure；裸 isinstance(e, jv.Unsure) 只是看一眼，不算处理
            # （否则任何包装层一判种类，J-05 就失效）。Act / Ignore / Pick / At 不需消费，命中即标记只作记账。
            public = getattr(cls, "_public", cls)
            if public is not Unsure or _caller_is_match():
                inst.__dict__["consumed"] = True
                Exit._last_matched = inst
        return ok


_MATCH_OFFSETS: dict = {}


def _caller_is_match() -> bool:
    """__instancecheck__ 的调用者是 `match … case jv.X()`（MATCH_CLASS）还是 `isinstance(...)`（CALL）。

    CPython 从 MATCH_CLASS 与 isinstance 都进 __instancecheck__，无法从参数区分；
    看调用帧当前指令的操作码可以区分（3.12 实测：MATCH_CLASS vs CALL）。取不到帧时保守当 isinstance。
    """
    try:
        f = sys._getframe(2)
    except ValueError:
        return False
    code = f.f_code
    offs = _MATCH_OFFSETS.get(code)
    if offs is None:
        offs = {i.offset for i in dis.get_instructions(code) if i.opname == "MATCH_CLASS"}
        _MATCH_OFFSETS[code] = offs
    return f.f_lasti in offs


class Exit(metaclass=_ExitMeta):
    """出口（§2.3）。是材料的子类型：可 as_mat() 渲染回状态，但 derived_from ∋ q。"""
    __match_args__ = ()
    _last_matched = None            # 最近一次 match / isinstance 命中的出口（handle(cause) 用）

    def __init__(self, p: float = 0.0, q_hash: str = "", taint: str = TRUSTED,
                 reading: Reading | None = None, provisional: bool = False, detail: dict | None = None):
        self.p = p
        self.q_hash = q_hash
        self.taint = taint
        self.reading = reading
        self.provisional = provisional
        self.detail = detail or {}
        self.consumed = False
        self.derived_from = frozenset({q_hash}) if q_hash else frozenset()

    @property
    def kind(self) -> str:
        return getattr(type(self), "_public", type(self)).__name__.lower()

    def as_mat(self) -> Mat:
        return Mat(content={"exit": self.kind, **({"k": self.k} if hasattr(self, "k") else {}),
                            **({"level": self.level} if hasattr(self, "level") else {}),
                            **({"cause": self.cause} if hasattr(self, "cause") else {})},
                   origin=("cut", self.q_hash), taint=self.taint, derived_from=self.derived_from)

    def __eq__(self, other):
        if isinstance(other, type) and issubclass(other, Exit):
            import warnings
            warnings.warn("W-cmp-type: 出口与类比较恒为假；用 match 或 isinstance(e, jv.Act)", stacklevel=2)
            return False
        return self is other

    def __hash__(self):
        return id(self)

    def __repr__(self):
        extra = ""
        for a in ("k", "level", "cause"):
            if hasattr(self, a):
                extra = f"({getattr(self, a)!r})"
        return f"{getattr(type(self), '_public', type(self)).__name__}{extra}[p={self.p:.2f}{', provisional' if self.provisional else ''}]"


class Act(Exit):
    pass


class Ignore(Exit):
    pass


class Pick(Exit):
    __match_args__ = ("k",)

    def __init__(self, k: int, **kw):
        super().__init__(**kw)
        self.k = k


class At(Exit):
    __match_args__ = ("level",)

    def __init__(self, level: int, **kw):
        super().__init__(**kw)
        self.level = level


class Unsure(Exit):
    __match_args__ = ("cause",)

    def __init__(self, cause: str, **kw):
        if cause not in CAUSES:
            raise JvError(f"Unsure 的 cause 只能是 {CAUSES}：{cause}")
        super().__init__(**kw)
        self.cause = cause


@dataclass(frozen=True)
class Fail:
    """`do` 失败是值（J-12），进账本，走 Unsure(fail)。"""
    reason: str
    action: str = ""
    detail: dict = field(default_factory=dict)


class FailList(list):
    """列表型 `jv.transform` 失败时的返回值（J-12「失败是值」）：形状仍是 list（空），`isinstance(x, list)` 为真、
    `if not xs` 照常用；`.fail` 记原因；`jv.on_fail(xs, 替代)` 认它。"""
    fail: "Fail | None" = None


class Escalated:
    """`jv.escalate(x, note, exits=)` 的返回值：程序把 x 交给人，记账；`exits` 是随之被消费（记为 escalate）的出口。"""

    def __init__(self, payload: Any, note: str = "", exits: list | None = None):
        self.payload, self.note = payload, note
        self.exits = list(exits or [])

    def __repr__(self):
        return f"Escalated({self.note or type(self.payload).__name__})"
