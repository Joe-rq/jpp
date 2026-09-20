"""IR 检查器的静态面（§3、§6.3）：对 `@jv.program` 函数做 AST 分析。

静态：J-01（match/比较 judge 结果）、J-02（派生材料回问同题，名字可追时）、J-03（线字面）、J-06、J-08（不可逆动作无守卫）、
J-11（宿主调用结果直接进槽）、J-12（do 无任何失败处理）、J-13、J-14（on 多对象）、J-16（fit 未注册，运行期再核）、
J-17（§6.3 六种模式）。报文 `J-xx: 一句话。修法：…`，warn 前缀 `W-`。
"""

from __future__ import annotations

import ast
import inspect
import textwrap
from dataclasses import dataclass, field

from .ir import Action

JV_FORMS = {"state", "judge", "cut", "fit", "gen", "do", "ask", "transform", "lit", "mat", "handle", "consume",
            "escalate", "on_fail", "on_truth"}


@dataclass
class CheckReport:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return not self.errors

    def __repr__(self):
        return f"CheckReport(errors={len(self.errors)}, warnings={len(self.warnings)})"


def _is_jv(node: ast.AST, name: str | None = None) -> bool:
    """`jv.x(...)` 或 `jv.x`。"""
    if isinstance(node, ast.Call):
        node = node.func
    if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id == "jv":
        return name is None or node.attr == name
    return False


def _jv_name(node: ast.AST) -> str | None:
    if isinstance(node, ast.Call):
        node = node.func
    if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id == "jv":
        return node.attr
    return None


def _kw(call: ast.Call, name: str):
    for k in call.keywords:
        if k.arg == name:
            return k.value
    return None


def check(fn) -> CheckReport:
    rep = CheckReport()
    try:
        src = textwrap.dedent(inspect.getsource(fn))
    except (OSError, TypeError):
        rep.warnings.append("W-nosource: 取不到源码，静态检查跳过")
        return rep
    tree = ast.parse(src)
    fdef = next((n for n in ast.walk(tree) if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))), None)
    if fdef is None:
        return rep
    _Checker(rep, _scope_of(fn)).visit(fdef)
    return rep


def _scope_of(fn) -> dict:
    """编译期可见的名字：全局 + 闭包非局部。参数与局部名不可见（→ W-dynamic，运行期核）。"""
    g = dict(getattr(fn, "__globals__", {}))
    try:
        g.update(inspect.getclosurevars(fn).nonlocals)
    except (TypeError, ValueError):
        pass
    return g


class _Checker(ast.NodeVisitor):
    def __init__(self, rep: CheckReport, globals_: dict):
        self.rep = rep
        self.g = globals_
        self.judge_vars: set[str] = set()          # 直接绑定 jv.judge(...) 结果的名字
        self.cut_vars: dict[str, set[str]] = {}    # 出口变量 → 它用的题名
        self.loop_depth = 0
        self.loop_assigned_do: list[set[str]] = []
        self.loop_vars: list[set[str]] = []
        self.has_do = False
        self.has_unsure_handling = False
        self.q_names: dict[str, str] = {}          # 变量 → 题文本（追 J-02）
        self.warned_dynamic = False                # W-dynamic 只报一次
        self.warned_self_trusted: set = set()      # W-self-trusted 每个动作一次
        self.params: set[str] = set()              # 程序参数名（jv.mat 的实参若是参数不算宿主计算）

    # —— 记录
    def _line(self, node) -> str:
        return f"行 {getattr(node, 'lineno', '?')}"

    def err(self, node, msg):
        self.rep.errors.append(f"{msg}（{self._line(node)}）")

    def warn(self, node, msg):
        self.rep.warnings.append(f"{msg}（{self._line(node)}）")

    # —— 赋值追踪
    def visit_Assign(self, node: ast.Assign):
        tgt = node.targets[0]
        if isinstance(tgt, ast.Name):
            v = node.value
            if _is_jv(v, "judge"):
                self.judge_vars.add(tgt.id)
                self.cut_vars.pop(tgt.id, None)
            elif _is_jv(v, "cut"):
                self.judge_vars.discard(tgt.id)
                self.cut_vars[tgt.id] = self._q_names_in(v)
            elif _jv_name(v) in ("test", "select", "measure"):
                self.q_names[tgt.id] = ast.unparse(v.args[0]) if v.args else tgt.id
            else:
                self.judge_vars.discard(tgt.id)
                self.cut_vars.pop(tgt.id, None)
            if self.loop_assigned_do and _is_jv(v, "do"):
                self.loop_assigned_do[-1].add(tgt.id)
        self.generic_visit(node)

    def visit_NamedExpr(self, node: ast.NamedExpr):
        if isinstance(node.target, ast.Name) and _is_jv(node.value, "judge"):
            self.judge_vars.add(node.target.id)
        self.generic_visit(node)

    def _q_names_in(self, node) -> set[str]:
        out = set()
        for n in ast.walk(node):
            if isinstance(n, ast.Call) and _is_jv(n, "judge"):
                for a in n.args[1:]:
                    if isinstance(a, ast.Name):
                        out.add(a.id)
        return out

    # —— match（§6.3-1、J-05 静态 warn）
    def visit_Match(self, node: ast.Match):
        subj = node.subject
        if isinstance(subj, ast.Name) and subj.id in self.judge_vars:
            self.err(node, f"J-01: match 的对象 {subj.id} 是 judge 返回的读数（向量或其解包元素），不是出口。修法：取 [i] 再 jv.cut，"
                           f"如 match jv.cut({subj.id}[0])；向量用 jv.cut(jv.judge([...], q)) 再解包")
        elif isinstance(subj, ast.Subscript) and self._reading_expr(subj):
            self.err(node, f"J-01: match 的对象 {ast.unparse(subj)} 是读数向量的元素，不是出口。修法：match jv.cut({ast.unparse(subj)})")
        if _is_jv(subj, "judge"):
            self.err(node, "J-01: match 直接作用在 jv.judge(...) 上；judge 返回读数向量。修法：match jv.cut(jv.judge(...)[0])")
        kinds = set()
        wildcard = False
        for c in node.cases:
            p = c.pattern
            if isinstance(p, ast.MatchAs) and p.pattern is None:
                wildcard = True
            if isinstance(p, ast.MatchClass) and _is_jv(p.cls):
                kinds.add(_jv_name(p.cls))
                if _jv_name(p.cls) == "Unsure":
                    self.has_unsure_handling = True
        if kinds and "Unsure" not in kinds and not wildcard:
            self.warn(node, "W-exhaust: match 没有 case jv.Unsure(c) 也没有通配；未消费的 Unsure 在返回前按 J-05 报错")
        self.generic_visit(node)

    # —— 比较（§6.3-2、§6.3-3）
    def visit_Compare(self, node: ast.Compare):
        parts = [node.left] + node.comparators
        for p in parts:
            if _is_jv(p) and not isinstance(p, ast.Call) and _jv_name(p) in ("Act", "Ignore", "Unsure", "Pick", "At"):
                self.warn(node, f"W-cmp-type: 出口与类 jv.{_jv_name(p)} 比较恒为假。修法：用 match 或 isinstance(e, jv.{_jv_name(p)})")
        names = [p for p in parts if isinstance(p, ast.Name) and p.id in self.judge_vars]
        subs = [p for p in parts if isinstance(p, ast.Subscript) and isinstance(p.value, ast.Name) and p.value.id in self.judge_vars]
        if (names or subs) and any(isinstance(op, (ast.Lt, ast.Gt, ast.LtE, ast.GtE)) for op in node.ops):
            self.err(node, "J-01: 读数不可比较。修法：同题跨对象用 .order()，出口用 jv.cut")
        self.generic_visit(node)

    # —— 调用
    def visit_Call(self, node: ast.Call):
        name = _jv_name(node)
        if name in ("test", "select", "measure", "cut"):
            c = _kw(node, "calib")
            if isinstance(c, ast.Constant):
                self.err(node, f"J-03: jv.{name} 的 calib 是字面量 {c.value!r}；线不可字面。修法：jv.calib({c.value!r})，或把 calib 挂在题上")
            if name == "cut" and c is None and len(node.args) >= 2 and isinstance(node.args[1], ast.Constant):
                self.err(node, "J-03: cut 的第二个位置参数是字面量。修法：cost=(fp, fn) 或 calib=jv.calib(...)")
        if name == "do":
            self.has_do = True
            seq = _kw(node, "iter_seq")
            if self.loop_depth > 0:
                if seq is None:
                    self.err(node, "J-13: 循环里的 jv.do 缺 iter_seq。修法：iter_seq=it.n（jv.loop）或 range 变量")
                elif isinstance(seq, ast.Constant):
                    self._const_seq(node, f"jv.do 用常量序号 iter_seq={seq.value}", "it.n / range 变量")
            act = node.args[0] if node.args else None
            if isinstance(act, ast.Name):
                obj = self.g.get(act.id)
                if isinstance(obj, Action) and not obj.reversible and _kw(node, "guard") is None:
                    self.err(node, f"J-08: 不可逆动作 {obj.name} 没有 guard=。修法：guard=[e]，e 来自 trusted 状态的 Act，或经 jv.ask")
                if isinstance(obj, Action) and obj.taint_out == "trusted" and not obj.registered \
                        and obj.name not in self.warned_self_trusted:
                    self.warned_self_trusted.add(obj.name)
                    self.warn(node, f"W-self-trusted: 动作 {obj.name} 在程序模块里自声明 taint_out=trusted；可信来自来源登记（I6），"
                                    f"不来自程序自报。修法：jv.register_action({obj.name!r}, fn, taint_out='trusted', reason='为什么可信')，"
                                    f"或改 'inherit'/'untrusted' 并让不可逆动作的守卫经 jv.ask")
            if self.loop_assigned_do:
                for a in ast.walk(node):
                    if isinstance(a, ast.Name) and a.id in self.loop_assigned_do[-1]:
                        self.warn(node, f"W-serial: 循环体内 jv.do 依赖同一循环里上一次 do 的结果 {a.id}，只能串行。"
                                        f"修法：无依赖请先收集句柄再刷新；有依赖则接受串行")
                        break
        if name == "gen":
            seq = _kw(node, "retry_seq")
            if self.loop_depth > 0:
                if seq is None:
                    self.err(node, "J-13: 循环里的 jv.gen 缺 retry_seq。修法：retry_seq=it.n / range 变量")
                elif isinstance(seq, ast.Constant):
                    self._const_seq(node, f"jv.gen 用常量 retry_seq={seq.value}，regen 必须递增", "it.n / range 变量")
        if name == "loop":
            if _kw(node, "bound") is None or _kw(node, "variant") is None:
                self.err(node, "J-06: jv.loop 必带 bound 与 variant。修法：jv.loop(bound=N, variant=jv.decreasing(lambda: 计量))")
        if name == "state":
            self._check_state(node)
        if name == "judge":
            self._check_judge(node)
        if name == "fit":
            ref = node.args[0] if node.args else None
            if ref is not None and not _is_jv(ref, "fitref"):
                self.err(node, "J-16: fit 的第一个参数必须是 jv.fitref(\"名\")（注册表签名）")
        if name in ("handle", "consume"):
            self.has_unsure_handling = True
        if name in ("mat", "lit") and node.args and not self._literal_arg(node.args[0]):
            self.warn(node, f"W-literal-from-host: jv.{name}({ast.unparse(node.args[0])[:40]}) 的实参不是字面量也不是程序参数，"
                            f"是宿主计算的结果；它进槽会绕过来源链（J-11）。修法：jv.transform(f, *mats) 记账后再进槽")
        if isinstance(node.func, ast.Name) and node.func.id == "isinstance" and len(node.args) == 2 \
                and self._reading_expr(node.args[0]) and _is_jv(node.args[1]):
            self.err(node, f"J-01: isinstance({ast.unparse(node.args[0])}, jv.{_jv_name(node.args[1])}) 的对象是读数不是出口。"
                           f"修法：先 jv.cut 得出口（向量：jv.cut(jv.judge([...], q)) 再解包）")
        self.generic_visit(node)

    def _literal_arg(self, e) -> bool:
        """jv.mat / jv.lit 的合法实参：常量、常量容器、程序参数名、f-string 常量。"""
        if isinstance(e, ast.Constant):
            return True
        if isinstance(e, (ast.List, ast.Tuple, ast.Set)):
            return all(self._literal_arg(x) for x in e.elts)
        if isinstance(e, ast.Dict):
            return all(self._literal_arg(x) for x in list(e.keys) + list(e.values) if x is not None)
        if isinstance(e, ast.Name):
            return e.id in self.params
        if isinstance(e, ast.JoinedStr):
            return all(isinstance(v, ast.Constant) or (isinstance(v, ast.FormattedValue) and self._literal_arg(v.value)) for v in e.values)
        return False

    def _const_seq(self, node: ast.Call, what: str, fix: str):
        """J-13：循环里常量序号。若调用的参数含循环变量（键随参数变，不碰撞）降为 W-seq-const。"""
        lv = set().union(*self.loop_vars) if self.loop_vars else set()
        exprs = list(node.args) + [k.value for k in node.keywords if k.arg not in ("iter_seq", "retry_seq")]
        uses_loop_var = any(isinstance(n, ast.Name) and n.id in lv for a in exprs for n in ast.walk(a))
        if uses_loop_var:
            self.warn(node, f"W-seq-const: 循环里的 {what}；参数含循环变量故键不碰撞，但同参数重入会取账本。修法：用 {fix}")
        else:
            self.err(node, f"J-13: 循环里的 {what}。修法：用 {fix}")

    def _check_state(self, node: ast.Call):
        on = _kw(node, "on")
        if on is None and node.args:
            on = node.args[0]
        if isinstance(on, (ast.List, ast.ListComp)) or (isinstance(on, ast.Tuple) and len(on.elts) != 2):
            self.err(node, "J-14: on 恰一个判断对象（关系用二元组）。修法：多个对象用 jv.judge([jv.state(on=x) for x in xs], q) 或 over")
        for slot in ("on", "ctx", "ref", "over"):
            v = _kw(node, slot)
            if v is None:
                continue
            elts = v.elts if isinstance(v, (ast.List, ast.Tuple)) else [v]
            for e in elts:
                if isinstance(e, ast.Starred):
                    e = e.value
                if isinstance(e, ast.Call) and not _is_jv(e):
                    callee = self.g.get(e.func.id) if isinstance(e.func, ast.Name) else None
                    if callee is not None and getattr(callee, "__jv_program__", False):
                        continue                                  # 程序调用程序：返回值视为效应输出（J-11 合法）
                    if callee is None:                            # 参数 / 局部名 / 方法：编译期不可见 → 运行期核
                        if not self.warned_dynamic:
                            self.warned_dynamic = True
                            self.warn(node, f"W-dynamic: 槽 {slot} 收到编译期不可见的调用 {ast.unparse(e)[:30]}；"
                                            f"若它是 @jv.program 合法，否则运行期按 J-11 报错")
                        continue
                    self.err(node, f"J-11: 槽 {slot} 直接收宿主调用 {ast.unparse(e)[:30]} 的返回值。"
                                   f"修法：经 jv.transform(f, ...) 或 jv.lit(...)")
                if isinstance(e, ast.Constant) and isinstance(e.value, str):
                    self.err(node, f"J-11: 槽 {slot} 收到裸字符串。修法：jv.lit({e.value[:12]!r})")

    def _check_judge(self, node: ast.Call):
        """J-02（名字可追）：状态里含由题 q 派生的出口变量，再问 q。"""
        if len(node.args) < 2:
            return
        st = node.args[0]
        used = set()
        for n in ast.walk(st):
            if isinstance(n, ast.Name) and n.id in self.cut_vars:
                used |= self.cut_vars[n.id]
        for a in node.args[1:]:
            if isinstance(a, ast.Name) and a.id in used:
                self.err(node, f"J-02: 状态里含由题 {a.id} 派生的出口，再问 {a.id}（禁自指）。修法：换题或换校准键")

    # —— 循环
    def _enter_loop(self, node, targets=(), iters=()):
        self.loop_depth += 1
        self.loop_assigned_do.append(set())
        self.loop_vars.append({n.id for t in targets for n in ast.walk(t) if isinstance(n, ast.Name)})
        for t, it in zip(targets, iters):
            self._bind_reading_elems(t, it)
        self.generic_visit(node)
        self.loop_vars.pop()
        self.loop_assigned_do.pop()
        self.loop_depth -= 1

    def _reading_expr(self, e) -> bool:
        """表达式是读数向量（judge 变量、其下标、或 jv.judge(...) 本身）。"""
        if isinstance(e, ast.Name):
            return e.id in self.judge_vars
        if isinstance(e, ast.Subscript):
            return self._reading_expr(e.value)
        return _is_jv(e, "judge")

    def _bind_reading_elems(self, target, it):
        """`for e in R` / `for c, e in zip(cmds, R)` / `for i, e in enumerate(R)`：解包出来的元素名记为读数变量（§6.3 模式 1 扩展）。"""
        names = [n for n in ast.walk(target) if isinstance(n, ast.Name)]
        if self._reading_expr(it):
            for n in names:
                self.judge_vars.add(n.id)
            return
        if isinstance(it, ast.Call) and isinstance(it.func, ast.Name) and it.func.id in ("zip", "enumerate"):
            args = it.args if it.func.id == "zip" else [None] + it.args
            elts = target.elts if isinstance(target, ast.Tuple) else [target]
            if len(elts) == len(args):
                for t, a in zip(elts, args):
                    if a is not None and self._reading_expr(a):
                        for n in ast.walk(t):
                            if isinstance(n, ast.Name):
                                self.judge_vars.add(n.id)
            elif any(a is not None and self._reading_expr(a) for a in args):
                for n in names:
                    self.judge_vars.add(n.id)

    def visit_For(self, node: ast.For):
        self._enter_loop(node, [node.target], [node.iter])

    def visit_While(self, node: ast.While):
        self._enter_loop(node)

    def visit_ListComp(self, node: ast.ListComp):
        self._enter_loop(node, [g.target for g in node.generators], [g.iter for g in node.generators])

    visit_GeneratorExp = visit_ListComp
    visit_SetComp = visit_ListComp

    def visit_FunctionDef(self, node: ast.FunctionDef):
        a = node.args
        self.params |= {x.arg for x in a.args + a.posonlyargs + a.kwonlyargs}
        if a.vararg:
            self.params.add(a.vararg.arg)
        if a.kwarg:
            self.params.add(a.kwarg.arg)
        self.generic_visit(node)
        if self.has_do and not self.has_unsure_handling:
            self.warn(node, "W-fail: 程序含 jv.do 但没有任何 Unsure 处理（handle/consume/case jv.Unsure）；do 失败按 J-12 走 Unsure(fail)，返回前会按 J-05 报错")
