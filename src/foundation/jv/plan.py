"""`jv plan`：计划期估计（J-07 符号成本签名、J-10 静态 unsure 上界、§4-6 预算 pass 的计划期一半）。

对 `@jv.program` 函数做 AST 走查，给每个效应调用点一条符号成本签名：
    调用数 / 题数 / 层数 / gen 时延 / do 成本 = 输入规模的函数
未知规模用符号（`|xs|`、`n@行`、`w@行`）；能算出数的算出数；只告警不拒（W-cost / W-taint / W-window /
W-untested / W-serial / W-unsure-bound）。运行期核（层边界）在 runtime._run_layer，这里只做估计。

层数是**上界**：一条语句里的多个 `jv.cut` 记一层（向量化 cut 也是一层）；两条语句各含 cut 记两层，即使
它们在运行期可能因为读数都已就绪而不再触发刷新。
"""

from __future__ import annotations

import ast
import inspect
import textwrap
from collections.abc import Mapping
from dataclasses import dataclass, field

from .checker import _is_jv, _jv_name, _kw, check
from .ir import Action, Budget, JvError

T_GEN = "T_gen"                       # 生成器时延符号（档案 profile_gen 未建，留符号）
S_TOK = "s"                           # 状态 token 符号


# ---------------------------------------------------------------- 符号多项式
class Sym:
    """整系数多项式：{按名排序的符号元组: 系数}。`Sym(3)`、`Sym.var("n")`；支持 + 与 ×。"""

    __slots__ = ("terms",)

    def __init__(self, v: int | dict = 0):
        if isinstance(v, dict):
            self.terms = {k: c for k, c in v.items() if c}
        else:
            self.terms = {(): int(v)} if v else {}

    @staticmethod
    def var(name: str) -> "Sym":
        return Sym({(name,): 1})

    def __add__(self, o):
        o = o if isinstance(o, Sym) else Sym(o)
        t = dict(self.terms)
        for k, c in o.terms.items():
            t[k] = t.get(k, 0) + c
        return Sym(t)

    __radd__ = __add__

    def __mul__(self, o):
        o = o if isinstance(o, Sym) else Sym(o)
        t: dict = {}
        for k1, c1 in self.terms.items():
            for k2, c2 in o.terms.items():
                k = tuple(sorted(k1 + k2))
                t[k] = t.get(k, 0) + c1 * c2
        return Sym(t)

    __rmul__ = __mul__

    @property
    def is_numeric(self) -> bool:
        return all(k == () for k in self.terms)

    @property
    def value(self) -> int:
        return self.terms.get((), 0)

    @property
    def degree(self) -> int:
        """最高次项含几个符号因子；≥ 2 即笛卡尔积（W-cost）。"""
        return max((len(k) for k in self.terms), default=0)

    @property
    def symbols(self) -> set[str]:
        return {s for k in self.terms for s in k}

    def subs(self, env: dict[str, int]) -> "Sym":
        out = Sym(0)
        for k, c in self.terms.items():
            term = Sym(c)
            for s in k:
                term = term * (Sym(env[s]) if s in env else Sym.var(s))
            out = out + term
        return out

    def __repr__(self):
        if not self.terms:
            return "0"
        parts = []
        for k, c in sorted(self.terms.items(), key=lambda kv: (-len(kv[0]), kv[0])):
            body = "·".join(k)
            if not k:
                parts.append(str(c))
            elif c == 1:
                parts.append(body)
            else:
                parts.append(f"{c}·{body}")
        return " + ".join(parts)

    def __eq__(self, o):
        o = o if isinstance(o, Sym) else Sym(o)
        return self.terms == o.terms

    def __hash__(self):
        return hash(tuple(sorted(self.terms.items())))


# ---------------------------------------------------------------- 报告
@dataclass
class PlanRow:
    line: int
    stmt: str
    effect: str
    mult: Sym
    count: Sym                    # 该点的效应次数（调用 / gen 次 / do 次 / 层）
    note: str = ""


@dataclass
class PlanReport:
    program: str
    budget: Budget
    rows: list[PlanRow] = field(default_factory=list)
    calls: Sym = field(default_factory=Sym)
    questions: Sym = field(default_factory=Sym)
    layers: Sym = field(default_factory=Sym)
    gen_calls: Sym = field(default_factory=Sym)
    gen_latency: Sym = field(default_factory=Sym)
    do_calls: Sym = field(default_factory=Sym)
    do_cost: Sym = field(default_factory=Sym)
    asks: Sym = field(default_factory=Sym)
    unsure_bound: Sym = field(default_factory=Sym)
    cost_formula: str = ""
    warnings: list[str] = field(default_factory=list)
    structure: object | None = None       # __jv_structure__ 路径读到的 root 节点（供核对，不参与序列化）

    def render(self) -> str:
        w = max([len(r.stmt) for r in self.rows] + [8])
        lines = [f"jv plan · {self.program}  budget={self.budget.to_dict()}",
                 f"{'行':>4}  {'效应':<9} {'倍率':<12} {'次数':<14} 语句"]
        for r in self.rows:
            lines.append(f"{r.line:>4}  {r.effect:<9} {r.mult!r:<12} {r.count!r:<14} {r.stmt[:w]}"
                         + (f"   # {r.note}" if r.note else ""))
        lines += ["—" * 40,
                  f"judge 调用 ≈ {self.calls!r}   题 ≈ {self.questions!r}   层 ≤ {self.layers!r}",
                  f"gen 次 = {self.gen_calls!r}   gen 时延 = {self.gen_latency!r}",
                  f"do 次 = {self.do_calls!r}   do 成本 = {self.do_cost!r}   ask = {self.asks!r}",
                  f"unsure 联合上界 Σuᵢ = {self.unsure_bound!r}",
                  f"钱 ≈ {self.cost_formula}"]
        for x in self.warnings:
            lines.append("  ! " + x)
        return "\n".join(lines)


# ---------------------------------------------------------------- 估计器
def plan(fn, budget: Budget | None = None, rt=None, profile: dict | None = None) -> PlanReport:
    """计划期估计。`fn` 可以是 `@jv.program` 包装后的函数（取其 budget 与原函数）或裸函数。"""
    inner = getattr(fn, "__jv_fn__", fn)
    budget = budget or getattr(fn, "__jv_budget__", None) or Budget()
    if rt is None:
        try:
            from .runtime import current
            rt = current()
        except Exception:
            rt = None
    profile = profile or (rt.profile if rt is not None else {})
    structure = getattr(fn, "__jv_structure__", None)
    if structure is None and inner is not fn:
        structure = getattr(inner, "__jv_structure__", None)
    if structure is not None:
        name = getattr(inner, "__name__", None) or getattr(fn, "__name__", None) or "?"
        return _plan_from_structure(structure, budget, rt, profile, name)
    rep = PlanReport(program=inner.__name__, budget=budget)
    try:
        src = textwrap.dedent(inspect.getsource(inner))
    except (OSError, TypeError):
        rep.warnings.append("W-nosource: 取不到源码，计划期估计跳过")
        return rep
    tree = ast.parse(src)
    fdef = next((n for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))), None)
    if fdef is None:
        return rep
    from .checker import _scope_of
    est = _Estimator(rep, _scope_of(inner), rt, profile, src.splitlines())
    est.visit(fdef)
    est.finish()
    # 静态检查里的 W-serial 等 warn 并入（§6.3 第六模式）
    for w in check(inner).warnings:
        if w.startswith("W-serial") and w not in rep.warnings:
            rep.warnings.append(w)
    return rep


# ---------------------------------------------------------------- __jv_structure__（组合库贯通路径）
# 协议（Claude的评审回复.md §「三处要改」第 3 条）：程序包装前 entry 与包装后函数均可挂
#     __jv_structure__ = {"version": 1, "root": node}
# node 是 Mapping，字段 operation/name/input/output/effects/effects_contract/children（tuple of node）。
# operation ∈ {leaf, identity, then, branch, opaque, product, iterate, bind}。
# 这条路径只读结构、只做符号成本合成，绝不触发任何执行（尤其 bind 的 factory 绝不在计划期被调用）；
# PlanReport 仍是唯一返回类型。
_SYM_FIELDS = ("calls", "questions", "layers", "gen_calls", "gen_latency",
               "do_calls", "do_cost", "asks", "unsure_bound")
# 每种 effect 关联的 Sym 字段：第一个是判断「AST 是否见到」的主字段，其余是同一调用点派生的字段——
# 主字段查零后，整组一起记未知（judge 未见到时 questions/layers/unsure_bound 也不该显示确定 0）。
_EFFECT_FIELDS = {"judge": ("calls", "questions", "layers", "unsure_bound"),
                   "gen": ("gen_calls", "gen_latency"),
                   "do": ("do_calls", "do_cost"),
                   "ask": ("asks",)}
_STRUCT_OPS = {"leaf", "identity", "then", "branch", "opaque", "product", "iterate", "bind"}


def _zero_cost() -> dict[str, Sym]:
    return {k: Sym(0) for k in _SYM_FIELDS}


def _struct_warn(rep: PlanReport, msg: str) -> None:
    if msg not in rep.warnings:
        rep.warnings.append(msg)


def _sym_upper_bound(a: Sym, b: Sym, field: str, rep: PlanReport) -> Sym:
    """branch 两臂的保守上界：数值取 max；同表达式直接复用；不可比较的符号按和值取上界并告警。"""
    if a.is_numeric and b.is_numeric:
        return Sym(max(a.value, b.value))
    if a.terms == b.terms:
        return a
    _struct_warn(rep, f"W-branch-bound: 字段 {field} 两臂开销 {a!r} 与 {b!r} 不可比较，按和值取保守上界")
    return a + b


def _node_name(node: Mapping) -> str:
    return str(node.get("name") or node.get("operation") or "?")


def _cost_of_leaf(node: Mapping, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    name = _node_name(node)
    declared = tuple(node.get("effects") or ())
    contract = node.get("effects_contract")
    fn = node.get("function")
    if fn is not None and not callable(fn):
        raise JvError(f"jv.plan: 叶 {name} 的 function 不可调用（{type(fn).__name__}），坏结构")

    # 无 function / 声明 '*'（未声明动态能力）/ contract=unknown：整叶未经证明，不摊到单个 effect 上，
    # 全部 9 项资源记未知符号——协议明文「未知不当纯证明」，这条不是可选优化。
    if fn is None or "*" in declared or contract == "unknown":
        reason = "无 function" if fn is None else ("effects 含 '*'" if "*" in declared else f"effects_contract={contract!r}")
        _struct_warn(rep, f"W-opaque: 叶 {name}（{reason}）未经证明，全部资源记未知符号，不当 0 计划")
        return {k: Sym.var(f"leaf:{name}:{k}") for k in _SYM_FIELDS}

    sub = plan(fn, rt=rt, profile=profile)
    for w in sub.warnings:
        _struct_warn(rep, w)
    if any(w.startswith("W-nosource") for w in sub.warnings):
        _struct_warn(rep, f"W-opaque: 叶 {name} 无源码，全部资源记未知符号，不当 0 计划")
        return {k: Sym.var(f"leaf:{name}:{k}") for k in _SYM_FIELDS}

    cost = {k: getattr(sub, k) for k in _SYM_FIELDS}
    rep.rows.extend(sub.rows)
    for eff in declared:
        fields = _EFFECT_FIELDS.get(eff)
        if fields is None:
            continue
        primary = cost[fields[0]]
        primary_is_zero = primary.is_numeric and primary.value == 0   # 只在明确数值 0 时补未知；符号非零估计原样保留
        if primary_is_zero:
            for f_ in fields:
                cost[f_] = Sym.var(f"leaf:{name}:{eff}:{f_}")
            _struct_warn(rep, f"W-opaque: 叶 {name} 声明 effect={eff}，AST 未见对应调用，"
                              f"相关字段 {fields} 记为未知符号，不当 0 计划")
    return cost


def _cost_of_then(node: Mapping, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    children = node.get("children") or ()
    if len(children) != 2:
        raise JvError(f"jv.plan: then 节点 {_node_name(node)} 需要恰好 2 个 children（左右两段），实得 {len(children)}")
    left = _cost_of_node(children[0], rt, profile, rep)
    right = _cost_of_node(children[1], rt, profile, rep)
    return {k: left[k] + right[k] for k in _SYM_FIELDS}


def _cost_of_branch(node: Mapping, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    children = node.get("children") or ()
    if len(children) != 3:
        raise JvError(f"jv.plan: branch 节点 {_node_name(node)} 需要恰好 3 个 children（predicate/yes/no），实得 {len(children)}")
    pred = _cost_of_node(children[0], rt, profile, rep)
    yes = _cost_of_node(children[1], rt, profile, rep)
    no = _cost_of_node(children[2], rt, profile, rep)
    return {k: pred[k] + _sym_upper_bound(yes[k], no[k], k, rep) for k in _SYM_FIELDS}


def _cost_of_product(node: Mapping, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    children = node.get("children") or ()
    if len(children) == 0:
        raise JvError(f"jv.plan: product 节点 {_node_name(node)} 需要至少 1 个 children，实得 0")
    total = _zero_cost()
    for child in children:
        c = _cost_of_node(child, rt, profile, rep)
        total = {k: total[k] + c[k] for k in _SYM_FIELDS}
    return total


def _cost_of_iterate(node: Mapping, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    children = node.get("children") or ()
    if len(children) != 2:
        raise JvError(f"jv.plan: iterate 节点 {_node_name(node)} 需要恰好 2 个 children（step/done），实得 {len(children)}")
    limit = (node.get("parameters") or {}).get("limit")
    if not isinstance(limit, int) or isinstance(limit, bool) or limit < 0:
        raise JvError(f"jv.plan: iterate 节点 {_node_name(node)} 的 parameters.limit 必须是非负整数，实得 {limit!r}")
    step = _cost_of_node(children[0], rt, profile, rep)
    done = _cost_of_node(children[1], rt, profile, rep)
    # 先查 done 再决定是否进 step：最多 N 步、最多 N+1 次 done 检查；N=0 只查一次 done，不进 step。保守上界，不是精确值。
    return {k: step[k] * limit + done[k] * (limit + 1) for k in _SYM_FIELDS}


def _cost_of_bind(node: Mapping, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    children = node.get("children") or ()
    if len(children) != 2:
        raise JvError(f"jv.plan: bind 节点 {_node_name(node)} 需要恰好 2 个 children（前段/factory），实得 {len(children)}")
    name = _node_name(node)
    prefix = _cost_of_node(children[0], rt, profile, rep)
    factory_node = children[1]
    # factory 子节点是原 Component 工厂的真实结构（不再包一层 leaf.function），原样递归分析，不隐藏、不执行；
    # 沿用它自身 operation 的既有规则（多为 leaf：AST 分析 + 声明未见到记未知）。
    factory = _cost_of_node(factory_node, rt, profile, rep)
    factory_visible = set(factory_node.get("effects") or ())
    factory_contract = factory_node.get("effects_contract")
    declared = (node.get("parameters") or {}).get("factory_effects")   # None=未声明超集；tuple=声明（可含 '*'）

    def full_unknown(reason: str) -> dict[str, Sym]:
        _struct_warn(rep, f"W-dynamic: 节点 {name}（bind）{reason}，全部资源记未知符号，不当 0 计划")
        return {k: Sym.var(f"bind:{name}:factory_effects:{k}") for k in _SYM_FIELDS}

    if declared is None:
        # 未声明 factory_effects：若 factory 自身也未分类（effects_contract=unknown），没有任何依据能界定
        # 它到底做什么，按通配处理，全部资源未知；若 factory 自身已提供可信声明，信任它，不额外加未知。
        if factory_contract == "unknown":
            extra = full_unknown("factory 未分类（effects_contract=unknown）且未声明 parameters.factory_effects")
        else:
            extra = _zero_cost()
    else:
        declared = tuple(declared)
        if "*" in declared:
            extra = full_unknown("parameters.factory_effects 含 '*'（未声明的动态能力）")
        else:
            missing = tuple(eff for eff in declared if eff not in factory_visible)
            extra = _zero_cost()
            if missing:
                for eff in missing:
                    fields = _EFFECT_FIELDS.get(eff)
                    if fields is None:
                        continue
                    for f_ in fields:
                        extra[f_] = extra[f_] + Sym.var(f"bind:{name}:factory_effects:{eff}:{f_}")
                _struct_warn(rep, f"W-opaque: 节点 {name}（bind）parameters.factory_effects 声明 {missing} 超出 "
                                   f"factory 节点自身 effects={tuple(sorted(factory_visible))}，对应资源记未知符号，"
                                   f"不当 0 计划")
    # Keep the previously agreed dynamic-continuation boundary as well as the
    # additional factory declaration: neither can stand in for the other.
    continuation = {k: Sym.var(f"bind:{name}:continuation:{k}") for k in _SYM_FIELDS}
    _struct_warn(rep, f"W-dynamic: 节点 {name}（bind）的 continuation 由 factory 运行期产生，"
                       f"静态计划不调用 factory，资源按未知符号处理，不当 0 计划")
    return {k: prefix[k] + factory[k] + extra[k] + continuation[k] for k in _SYM_FIELDS}


def _cost_of_opaque(node: Mapping, rep: PlanReport) -> dict[str, Sym]:
    name = _node_name(node)
    source_op = node.get("source_operation", "?")
    effects = tuple(node.get("effects") or ())
    cost = {k: Sym.var(f"opaque:{name}:{k}") for k in _SYM_FIELDS}
    _struct_warn(rep, f"W-opaque: 节点 {name}（{source_op}）未贯通，资源按未知符号处理，不当 0 计划")
    if "*" in effects:
        _struct_warn(rep, f"W-dynamic: 节点 {name}（{source_op}）effects 含 '*'，未声明的动态能力，禁止按 0 计划")
    return cost


def _cost_of_node(node, rt, profile: dict, rep: PlanReport) -> dict[str, Sym]:
    if not isinstance(node, Mapping):
        raise JvError(f"jv.plan: __jv_structure__ 节点必须是 Mapping，实得 {type(node).__name__}")
    op = node.get("operation")
    if op not in _STRUCT_OPS:
        raise JvError(f"jv.plan: __jv_structure__ 节点 operation={op!r} 未知；支持 {sorted(_STRUCT_OPS)}")
    if op == "leaf":
        return _cost_of_leaf(node, rt, profile, rep)
    if op == "identity":
        return _zero_cost()
    if op == "then":
        return _cost_of_then(node, rt, profile, rep)
    if op == "branch":
        return _cost_of_branch(node, rt, profile, rep)
    if op == "product":
        return _cost_of_product(node, rt, profile, rep)
    if op == "iterate":
        return _cost_of_iterate(node, rt, profile, rep)
    if op == "bind":
        return _cost_of_bind(node, rt, profile, rep)
    return _cost_of_opaque(node, rep)


def _plan_from_structure(structure, budget: Budget, rt, profile: dict, fallback_name: str) -> PlanReport:
    if not isinstance(structure, Mapping):
        raise JvError(f"jv.plan: __jv_structure__ 必须是 Mapping，实得 {type(structure).__name__}")
    version = structure.get("version")
    if version != 1:
        raise JvError(f"jv.plan: 未知 __jv_structure__ 版本 {version!r}，仅支持 version=1")
    root = structure.get("root")
    if not isinstance(root, Mapping):
        raise JvError("jv.plan: __jv_structure__ 缺少合法的 root 节点（Mapping）")
    rep = PlanReport(program=fallback_name if fallback_name != "?" else _node_name(root), budget=budget)
    cost = _cost_of_node(root, rt, profile, rep)
    for k in _SYM_FIELDS:
        setattr(rep, k, cost[k])
    price = (profile.get("cost") or {}).get("price_usd_per_input_token", 4.2e-8)
    reg = (profile.get("cost") or {}).get("regression") or {}
    icpt, per_q = reg.get("intercept_tokens", 271), (profile.get("cost") or {}).get("tokens_per_question", 38)
    rep.cost_formula = f"({rep.calls!r}) × ({icpt} + {S_TOK}) × {price:.1e} + ({rep.questions!r}) × {per_q} × {price:.1e}"
    rep.structure = root
    return rep


class _Estimator(ast.NodeVisitor):
    def __init__(self, rep: PlanReport, g: dict, rt, profile: dict, src_lines: list[str]):
        self.rep, self.g, self.rt, self.profile, self.src = rep, g, rt, profile, src_lines
        self.mult: list[Sym] = [Sym(1)]
        self.q_calib: dict[str, str] = {}          # 题变量 → 校准键
        self.q_op: dict[str, str] = {}             # 题变量 → op
        self.untrusted_names: set[str] = set()     # 绑到 untrusted do 输出的名字
        self.judge_untrusted: set[str] = set()     # judge 结果变量，其状态含 untrusted 名字
        self.exit_untrusted: set[str] = set()      # cut 结果变量，来自 untrusted judge
        self.stmt_layer_done: set[int] = set()     # 已记层的语句 id
        self.cur_stmt: ast.stmt | None = None
        self.growing_ctx = False
        self.has_select = False
        self.escalate_sites = Sym(0)
        self.visiting: set[str] = set()            # 程序调用程序的递归护栏
        self.sizes: dict[str, Sym] = {}            # 名字 → 已知的列表规模（gen 的 n、字面列表、生成式）
        self.judge_mult: dict[str, Sym] = {}       # 读数变量（含解包元素） → 登记时的倍率（D3：循环里 cut 循环外的向量只算一层）

    # —— 工具
    def m(self) -> Sym:
        return self.mult[-1]

    def stmt_text(self, node) -> str:
        ln = getattr(node, "lineno", None)
        if ln and 1 <= ln <= len(self.src):
            return self.src[ln - 1].strip()
        return ast.unparse(node)[:60]

    def row(self, node, effect: str, count: Sym, note: str = ""):
        self.rep.rows.append(PlanRow(getattr(node, "lineno", 0), self.stmt_text(node), effect, self.m(), count, note))

    def warn(self, msg: str):
        if msg not in self.rep.warnings:
            self.rep.warnings.append(msg)

    def _const_int(self, node) -> int | None:
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
            v = self._const_int(node.operand)
            return -v if v is not None else None
        return None

    def _iter_mult(self, it: ast.AST, line: int) -> Sym:
        """for 的迭代对象 → 倍率。"""
        if isinstance(it, ast.Call):
            if _is_jv(it, "loop"):
                b = _kw(it, "bound")
                v = self._const_int(b) if b is not None else None
                return Sym(v) if v is not None else Sym.var(f"bound@{line}")
            if isinstance(it.func, ast.Name) and it.func.id == "range":
                cs = [self._const_int(a) for a in it.args]
                if cs and all(c is not None for c in cs):
                    if len(cs) == 1:
                        return Sym(max(0, cs[0]))
                    if len(cs) == 2:
                        return Sym(max(0, cs[1] - cs[0]))
                    if len(cs) == 3 and cs[2]:
                        return Sym(max(0, -(-(cs[1] - cs[0]) // cs[2])))
                return Sym.var(f"n@{line}")
            if isinstance(it.func, ast.Name) and it.func.id in ("zip", "enumerate", "reversed", "sorted"):
                for a in it.args:
                    if isinstance(a, ast.Name):
                        return Sym.var(f"|{a.id}|")
                return Sym.var(f"n@{line}")
        if isinstance(it, ast.Name):
            return self.sizes.get(it.id, Sym.var(f"|{it.id}|"))
        if isinstance(it, (ast.List, ast.Tuple)):
            return Sym(len(it.elts))
        if isinstance(it, ast.ListComp):
            return self._comp_size(it, line)
        return Sym.var(f"n@{line}")

    def _comp_size(self, node: ast.ListComp, line: int) -> Sym:
        m = Sym(1)
        for gen in node.generators:
            m = m * self._iter_mult(gen.iter, line)
        return m

    def _size_of_value(self, v: ast.AST, line: int) -> Sym | None:
        """赋值右边的规模：gen(n=…) → n；字面列表 → 长度；生成式 → 倍率积；jv.cut(vec) / transform 不知。"""
        if _is_jv(v, "gen"):
            nk = _kw(v, "n")
            n = self._const_int(nk) if nk is not None else 4
            return Sym(n) if n is not None else None
        if isinstance(v, (ast.List, ast.Tuple)):
            return Sym(len(v.elts))
        if isinstance(v, ast.ListComp):
            return self._comp_size(v, line)
        return None

    def _states_count(self, s: ast.AST, line: int) -> Sym:
        """judge 的第一个参数 → 状态数。"""
        if isinstance(s, ast.List):
            return Sym(len(s.elts))
        if isinstance(s, ast.ListComp):
            m = Sym(1)
            for gen in s.generators:
                m = m * self._iter_mult(gen.iter, line)
            return m
        if isinstance(s, ast.Name):
            if s.id in self.sizes:
                return self.sizes[s.id]
            return Sym.var(f"|{s.id}|") if s.id not in self.g or isinstance(self.g.get(s.id), (list, tuple)) \
                else Sym(1)
        return Sym(1)

    # —— 语句级：层
    def generic_visit(self, node):
        if isinstance(node, ast.stmt):
            prev, self.cur_stmt = self.cur_stmt, node
            super().generic_visit(node)
            self.cur_stmt = prev
        else:
            super().generic_visit(node)

    # —— 循环
    def _enter(self, node, mult: Sym):
        self.mult.append(self.m() * mult)
        self.generic_visit(node)
        self.mult.pop()

    def _bind_elems(self, target, it):
        """`for e in R` / `for c, e in zip(cmds, R)`：解包元素继承 R 的登记倍率。"""
        names = [n.id for n in ast.walk(target) if isinstance(n, ast.Name)]
        srcs = [it] if not (isinstance(it, ast.Call) and isinstance(it.func, ast.Name) and it.func.id in ("zip", "enumerate")) else it.args
        for a in srcs:
            base = a.value if isinstance(a, ast.Subscript) else a
            if isinstance(base, ast.Name) and base.id in self.judge_mult:
                for n in names:
                    self.judge_mult[n] = self.judge_mult[base.id]

    def visit_For(self, node: ast.For):
        self._bind_elems(node.target, node.iter)
        self._enter(node, self._iter_mult(node.iter, node.lineno))

    def visit_While(self, node: ast.While):
        self.warn(f"W-cost: 行 {node.lineno} while 无界；层数与调用数无法静态估。修法：jv.loop(bound=…, variant=…)")
        self._enter(node, Sym.var(f"w@{node.lineno}"))

    def visit_ListComp(self, node: ast.ListComp):
        m = Sym(1)
        for gen in node.generators:
            m = m * self._iter_mult(gen.iter, node.lineno)
        # 生成式里的 judge/do/cut 按倍率算，但 judge([...]) 的列表参数在 _states_count 里单独处理
        for gen in node.generators:
            self._bind_elems(gen.target, gen.iter)
        self.mult.append(self.m() * m)
        self.visit(node.elt)
        for gen in node.generators:
            for cond in gen.ifs:
                self.visit(cond)
        self.mult.pop()

    visit_GeneratorExp = visit_ListComp
    visit_SetComp = visit_ListComp

    # —— 赋值追踪（题的校准键、taint 名字）
    def visit_Assign(self, node: ast.Assign):
        tgt = node.targets[0]
        v = node.value
        if isinstance(tgt, ast.Name):
            sz = self._size_of_value(v, node.lineno)
            if sz is not None:
                self.sizes[tgt.id] = sz
            else:
                self.sizes.pop(tgt.id, None)
            name = _jv_name(v)
            if name in ("test", "select", "measure"):
                self.q_op[tgt.id] = name
                c = _kw(v, "calib")
                if isinstance(c, ast.Call) and _is_jv(c, "calib") and c.args and isinstance(c.args[0], ast.Constant):
                    self.q_calib[tgt.id] = str(c.args[0].value)
            elif name == "do" and self._action_of(v) is not None and self._action_of(v).taint_out == "untrusted":
                self.untrusted_names.add(tgt.id)
            elif name == "judge":
                self.judge_mult[tgt.id] = self.m()
                if any(isinstance(n, ast.Name) and n.id in self.untrusted_names for n in ast.walk(v.args[0])) if v.args else False:
                    self.judge_untrusted.add(tgt.id)
            elif name == "cut":
                self.judge_mult.pop(tgt.id, None)
                if any(isinstance(n, ast.Name) and n.id in self.judge_untrusted for n in ast.walk(v)) or \
                        any(isinstance(n, ast.Call) and _is_jv(n, "judge") and n.args and
                            any(isinstance(x, ast.Name) and x.id in self.untrusted_names for x in ast.walk(n.args[0]))
                            for n in ast.walk(v)):
                    self.exit_untrusted.add(tgt.id)
            else:
                self.judge_mult.pop(tgt.id, None)
        self.generic_visit(node)

    def visit_NamedExpr(self, node: ast.NamedExpr):
        if isinstance(node.target, ast.Name) and _is_jv(node.value, "do"):
            a = self._action_of(node.value)
            if a is not None and a.taint_out == "untrusted":
                self.untrusted_names.add(node.target.id)
        self.generic_visit(node)

    def _action_of(self, call: ast.Call) -> Action | None:
        if call.args and isinstance(call.args[0], ast.Name):
            obj = self.g.get(call.args[0].id)
            if isinstance(obj, Action):
                return obj
        return None

    # —— 效应调用点
    def visit_Call(self, node: ast.Call):
        name = _jv_name(node)
        m = self.m()
        if name == "judge" and node.args:
            states = self._states_count(node.args[0], node.lineno)
            nq = Sym(sum(1 for a in node.args[1:] if not isinstance(a, ast.Starred)))
            if any(isinstance(a, ast.Starred) for a in node.args[1:]):
                nq = nq + Sym.var(f"q@{node.lineno}")
            calls = m * states
            self.rep.calls = self.rep.calls + calls
            self.rep.questions = self.rep.questions + calls * nq
            self.row(node, "judge", calls, f"状态 {states!r} × 题 {nq!r}（同状态融合为一次调用）")
            for a in node.args[1:]:
                if isinstance(a, ast.Name) and self._q_op(a.id) == "select":
                    self.has_select = True
            # unsure 上界：每个 cut 会消费一条读数；这里按 judge 的题数 × 状态数记 uᵢ
            for a in node.args[1:]:
                if isinstance(a, ast.Name):
                    self.rep.unsure_bound = self.rep.unsure_bound + calls * self._u_of(a.id)
                else:
                    self.rep.unsure_bound = self.rep.unsure_bound + calls * Sym.var("u")
        elif name in ("cut", "fit"):
            if self.cur_stmt is not None and id(self.cur_stmt) not in self.stmt_layer_done:
                self.stmt_layer_done.add(id(self.cur_stmt))
                # 被 cut 的读数若在循环外登记（向量解包后逐个 cut），层按登记处倍率算：整个向量一次刷新就绪
                srcs = [n.id for n in ast.walk(node) if isinstance(n, ast.Name) and n.id in self.judge_mult]
                lm = self.judge_mult[srcs[0]] if srcs else m
                self.rep.layers = self.rep.layers + lm
                self.row(node, "cut/层", lm, "刷新点：本语句记一层（上界）" + ("；读数在循环外登记，整向量一层" if srcs and lm != m else ""))
        elif name == "gen":
            nk = _kw(node, "n")
            n = self._const_int(nk) if nk is not None else 4
            self.rep.gen_calls = self.rep.gen_calls + m
            self.rep.gen_latency = self.rep.gen_latency + m * Sym.var(T_GEN)
            self.row(node, "gen", m, f"n={n if n is not None else '?'}；一登记就发，时延 {T_GEN}/次")
        elif name == "do":
            a = self._action_of(node)
            self.rep.do_calls = self.rep.do_calls + m
            if a is not None and a.cost:
                self.rep.do_cost = self.rep.do_cost + m * Sym(int(round(a.cost * 1000)))  # 千分之一单位
                note = f"{a.name} cost×1000={int(round(a.cost * 1000))} latency={a.latency}s"
            else:
                self.rep.do_cost = self.rep.do_cost + m * Sym.var(f"c_{a.name if a else 'do'}")
                note = f"{a.name if a else '?'} 成本未声明（Action.cost=0）→ 符号"
            self.row(node, "do", m, note)
            if a is not None and not a.reversible:
                g = _kw(node, "guard")
                if g is not None and any(isinstance(n, ast.Name) and n.id in self.exit_untrusted for n in ast.walk(g)):
                    self.warn(f"W-taint: 行 {node.lineno} 不可逆动作 {a.name} 的守卫出口来自 untrusted 材料上的判断；"
                              f"J-08 要求至少一个合取项来自 trusted 状态或经 jv.ask")
        elif name == "ask":
            self.rep.asks = self.rep.asks + m
            self.row(node, "ask", m, "小时到天；Pending 即程序挂起")
        elif name == "transform":
            self.row(node, "transform", m, "记账；纯性运行期核")
        elif name is None and isinstance(node.func, ast.Name):
            callee = self.g.get(node.func.id)
            if callee is not None and getattr(callee, "__jv_program__", False) and node.func.id not in self.visiting:
                # 程序调用程序：内层的签名按倍率并入
                self.visiting.add(node.func.id)
                sub = plan(callee, rt=self.rt, profile=self.profile)
                self.visiting.discard(node.func.id)
                rep = self.rep
                rep.calls = rep.calls + m * sub.calls
                rep.questions = rep.questions + m * sub.questions
                rep.layers = rep.layers + m * sub.layers
                rep.gen_calls = rep.gen_calls + m * sub.gen_calls
                rep.gen_latency = rep.gen_latency + m * sub.gen_latency
                rep.do_calls = rep.do_calls + m * sub.do_calls
                rep.do_cost = rep.do_cost + m * sub.do_cost
                rep.asks = rep.asks + m * sub.asks
                rep.unsure_bound = rep.unsure_bound + m * sub.unsure_bound
                self.row(node, "program", m, f"调用程序 {node.func.id}：judge {sub.calls!r} 层 {sub.layers!r}（子账）")
                for w in sub.warnings:
                    self.warn(f"[{node.func.id}] {w}")
        elif name == "state":
            for slot in ("ctx", "ref"):
                v = _kw(node, slot)
                if isinstance(v, (ast.List, ast.Tuple)) and any(isinstance(e, ast.Starred) for e in v.elts):
                    self.growing_ctx = True
                    self.warn(f"W-window: 行 {node.lineno} 槽 {slot} 含 *展开，语境随循环增长；JSON 槽窗口上限未测（P24），"
                              f"超 {self._json_window()} token 时裂变 pass 只报不切。修法：只放最近 k 条或先摘要（transform）")
        self.generic_visit(node)

    def _q_op(self, qname: str) -> str | None:
        """题变量的 op：程序内赋值追到的，或闭包/全局里的 Q 对象。"""
        if qname in self.q_op:
            return self.q_op[qname]
        obj = self.g.get(qname)
        return getattr(obj, "op", None) if obj is not None and hasattr(obj, "calib") else None

    def _q_calib(self, qname: str) -> str | None:
        if qname in self.q_calib:
            return self.q_calib[qname]
        obj = self.g.get(qname)
        c = getattr(obj, "calib", None)
        return getattr(c, "key", None)

    def _u_of(self, qname: str) -> Sym:
        key = self._q_calib(qname)
        if key and self.rt is not None:
            rec = self.rt.calib.get(key)
            u = getattr(rec, "unsure_rate", None)
            if rec.status == "上岗" and u is not None:
                return Sym(int(round(float(u) * 1000)))          # 千分之一单位
        return Sym.var(f"u_{key or qname}")

    def _json_window(self) -> int:
        try:
            ks = self.profile["window"]["json_slots"]["claim_bearing_ctx"]["flip_frac_by_ctx_tokens"]
            return max(int(k.strip("~")) for k in ks)
        except (KeyError, TypeError, ValueError):
            return 1800

    # —— 汇总与预算比对
    def finish(self):
        rep, b = self.rep, self.rep.budget
        price = (self.profile.get("cost") or {}).get("price_usd_per_input_token", 4.2e-8)
        reg = (self.profile.get("cost") or {}).get("regression") or {}
        icpt, per_q = reg.get("intercept_tokens", 271), (self.profile.get("cost") or {}).get("tokens_per_question", 38)
        rep.cost_formula = f"({rep.calls!r}) × ({icpt} + {S_TOK}) × {price:.1e} + ({rep.questions!r}) × {per_q} × {price:.1e}"
        if rep.calls.degree >= 2:
            self.warn(f"W-cost: judge 调用数 {rep.calls!r} 含符号乘积（笛卡尔积）；平方项会在运行期炸。修法：召回层先压 N，或 partition 后配对")
        if b.calls is not None and rep.calls.is_numeric and rep.calls.value > b.calls:
            self.warn(f"W-cost: 估计 judge 调用 {rep.calls.value} > budget.calls={b.calls}（上界估计；层边界会停并记 Unsure(budget)）")
        if b.layers is not None and rep.layers.is_numeric and rep.layers.value > b.layers:
            self.warn(f"W-cost: 层数上界 {rep.layers.value} > budget.layers={b.layers}；循环体内逐个 cut 每轮一层。"
                      f"修法：先收集句柄再 cut 向量，或调大 layers")
        if b.layers is not None and not rep.layers.is_numeric:
            self.warn(f"W-cost: 层数上界 {rep.layers!r} 含符号，budget.layers={b.layers} 只能在层边界核")
        if b.escalate is not None and rep.asks.is_numeric and rep.asks.value > b.escalate:
            self.warn(f"W-cost: ask 次数 {rep.asks.value} > budget.escalate={b.escalate}")
        if b.escalate is not None and not rep.asks.is_numeric:
            self.warn(f"W-cost: ask 次数 {rep.asks!r} 含符号，budget.escalate={b.escalate} 在运行期核")
        if b.unsure is not None:
            if rep.unsure_bound.is_numeric:
                ub = rep.unsure_bound.value / 1000.0
                if ub > b.unsure:
                    self.warn(f"W-unsure-bound: 联合上界 Σuᵢ={ub:.2f} > budget.unsure={b.unsure}（J-10；上界，独立估计只作参考）")
            else:
                self.warn(f"W-unsure-bound: Σuᵢ 含未标注键的符号 {sorted(rep.unsure_bound.symbols)}；冷键的 u 未知（J-10）")
        if self.has_select:
            kl = (self.profile.get("k_limit") or {}).get("by_candidate_tokens") or {}
            bands = [k for k, v in kl.items() if isinstance(v, dict) and v.get("K_max") == "未测"]
            if bands:
                self.warn(f"W-untested: 程序含 select；候选落档案未测档 {bands} 时按 J-15 取保守物理形式 K-noul")
        if self.rt is not None and self.has_select:
            bi = (self.profile.get("batch_invariance") or {}).get("choice_same_call_perm_crosstalk")
            if isinstance(bi, dict) and bi.get("value") == "未测":
                self.warn("W-untested: choice 两个置换同调用的串扰未测（档案 batch_invariance.choice_same_call_perm_crosstalk，"
                          "由 E-PERM-SAME-CALL 测）；下沉 pass 现按同调用融合")
