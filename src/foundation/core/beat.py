# -*- coding: utf-8 -*-
"""心跳：一个分区一拍（施工单 §3.5 / §3.9）。

四条硬规矩：
1. Jev 调用可以并发，写 Log 的顺序严格按 (layer, -priority, unit.name, item.id)，与返回先后无关；
2. 同一 (item, view) 上的多个单元合并成一次调用，先查账本；
3. 同一 (unit, version, view_fp) 在本 run 的**本分区**只判一次（once）；
4. 分区与分区之间没有特殊情形：`run_scope` 对根分区和对任何一层包实例是同一个函数，
   差别只在传进去的**数据**（观察者名单、outlets、拍数预算、上一层是谁），
   不在代码路径。根分区的"上一层"是人队列，通过和父分区同一个 `Upstream` 接口注入（S5.3）。
"""

from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import Any

from foundation.core import escalation, guard as guard_mod
from foundation.core.arbiter import arbitrate
from foundation.core.canon import H, canon
from foundation.core.escalation import HumanUpstream, ScopeUpstream, Upstream
from foundation.core.guard import Guards
from foundation.core.hand import Proposal, execute as execute_hand
from foundation.core.item import Item, draft_to_item
from foundation.core.ledger import Ledger, ledger_key
from foundation.core.log import Log
from foundation.core.outlet import ACT, IGNORE, UNSURE, Reading, read_answer, read_code
from foundation.core.pack import PACK_EYE, PackDef, PackObserver
from foundation.core.params import Params
from foundation.core.table import Table
from foundation.core.unit import DRAFT, ON_DUTY, SUSPENDED, Unit, UnitState, defs_fingerprint
from foundation.core.view import build_view
from foundation.clients.writer import write_key

QUIET = "quiet"
WAITING_ON_HUMAN = "waiting_on_human"
GUARD_STOP = "guard_stop"
BUDGET_STOP = "budget_stop"
MAX_BEATS_STOP = "max_beats_stop"

#: 全局停止：不管在根分区还是在第三层包实例里撞上，都要一路穿回 run_end。
#: 今晚只有花费是全局的（红队 A6：花费池全局唯一）；拍数按分区，不在这里面。
GLOBAL_STOPS = (BUDGET_STOP,)


@dataclass
class Ctx:
    """给 code 眼 / code 手 / 包的手的只读上下文。"""
    table: Table
    beat: int
    scope: str
    writer: Any = None
    run_id: str = ""

    def marks_on(self, item_id: str) -> list[Item]:
        return self.table.marks_on(item_id)

    def alive(self) -> list[Item]:
        return self.table.alive(self.scope)

    def generation(self, item_id: str) -> int:
        return self.table.generation(item_id)


@dataclass
class BeatReport:
    beat: int
    scope: str
    new_items: int = 0
    calls: int = 0
    cost: float = 0.0
    readings: int = 0
    escalated: int = 0
    stop: str | None = None


@dataclass
class ScopeRun:
    """一个分区跑完之后留下的东西（包实例把它交回给父分区的手）。"""
    scope: str
    status: str
    depth: int
    beats: int
    handed: list[dict] = field(default_factory=list)


class Engine:
    def __init__(self, units: list[Unit], log: Log, ledger: Ledger, eye_client, writer,
                 params: Params, states: dict[str, UnitState], run_dir: str,
                 run_id: str = "run", table: Table | None = None,
                 packs: dict[str, PackDef] | None = None,
                 root_packs: tuple[str, ...] = (),
                 root_units: tuple[str, ...] | None = None,
                 upstream: Upstream | None = None):
        self.units = sorted(units, key=lambda u: u.name)
        self.by_name = {u.name: u for u in self.units}
        self.log = log
        self.ledger = ledger
        self.eye = eye_client
        self.writer = writer
        self.params = params
        self.states = states
        self.run_dir = run_dir
        self.run_id = run_id
        self.calib_dir = os.path.join(run_dir, "calib")
        self.table = table if table is not None else Table.from_log(log.events())
        self.guards = Guards(params)
        self.once: dict[str, set[str]] = {}
        self.beat_no = 0
        self.beats_in_scope: dict[str, int] = {}
        self.stopped_scopes: dict[str, str] = {}
        self.run_stop: str | None = None
        self.applied_answers: set[str] = set()
        self.history: list[BeatReport] = []

        # —— 包 ——
        self.pack_defs: dict[str, PackDef] = dict(packs or {})
        self.observers_by_scope: dict[str, list] = {}
        self.scope_budget: dict[str, int] = {}
        self.scope_runs: dict[str, ScopeRun] = {}
        self.upstream = upstream if upstream is not None else HumanUpstream()

        base = [u for u in self.units
                if root_units is None or u.name in set(root_units)]
        self.root_observers = self._sorted(base + [self._pack_observer(n, depth=1)
                                                   for n in root_packs])

    # ——————————————————————— 观察者名单 ———————————————————————
    @staticmethod
    def _sorted(observers: list) -> list:
        return sorted(observers, key=lambda o: o.name)

    def _pack_observer(self, name: str, depth: int) -> PackObserver:
        if name not in self.pack_defs:
            raise SystemExit(f"找不到包定义：{name}；已知的包有 {sorted(self.pack_defs)}")
        obs = PackObserver(self.pack_defs[name], self, depth)
        st = self.states.get(obs.name)
        if st is None:
            # 包不走验题闸门（施工单 §1 "包级验题"推迟），直接在岗；
            # 名册里留一行是为了让 viewer 的名册与包树对得上。
            st = UnitState(name=obs.name, status=ON_DUTY, probes_pass=True,
                           note="包（§3.9）：不走验题闸门，本阶段直接在岗")
            self.states[obs.name] = st
        else:
            st.status = ON_DUTY
        return obs

    def _instance_observers(self, defn: PackDef, depth: int) -> list:
        missing = [n for n in defn.units if n not in self.by_name]
        if missing:
            raise SystemExit(f"包 {defn.name} 点名了不存在的单元：{missing}")
        obs = [self.by_name[n] for n in defn.units]
        obs += [self._pack_observer(n, depth=depth + 1) for n in defn.packs]
        return self._sorted(obs)

    def observers(self, scope: str) -> list:
        return self.observers_by_scope.get(scope, self.root_observers)

    # ——————————————————————— 公开入口 ———————————————————————
    def run(self, scope: str = "/", max_beats: int | None = None) -> str:
        """根分区。上一层是人队列，通过和父分区同一个 Upstream 接口注入。"""
        r = self.run_scope(scope, self.root_observers, self.upstream, depth=0,
                           beats_budget=self.params.beats_budget, outlets=(),
                           max_beats=max_beats)
        return r.status

    def open_queue(self, scope: str = "/") -> list[dict]:
        return escalation.open_queue(self.table, self.log.events(), scope)

    def human_rows(self) -> list[dict]:
        rows = getattr(self.upstream, "rows", None)
        return rows if rows is not None else self.open_queue("/")

    # ——————————————————————— 一个分区跑到安静 ———————————————————————
    def run_scope(self, scope: str, observers: list, upstream: Upstream, depth: int,
                  beats_budget: int, outlets: tuple[str, ...] = (),
                  max_beats: int | None = None) -> ScopeRun:
        self.observers_by_scope[scope] = observers
        self.scope_budget[scope] = beats_budget
        self.guards.check_return_to_same(scope, self.table.fingerprint(scope))  # 起点指纹
        status, n = QUIET, 0
        while True:
            if self.run_stop:                       # 全局停止（花费）一路穿回去
                status = self.run_stop
                break
            if max_beats is not None and n >= max_beats:
                self._escalate_stop(scope, "max_beats", {"max_beats": max_beats})
                status = MAX_BEATS_STOP
                break
            rep = self.beat(scope)
            n += 1
            if rep.stop:
                status = rep.stop
                break
            if rep.new_items == 0:
                status = WAITING_ON_HUMAN if self.open_queue(scope) else QUIET
                break

        items = escalation.handoff_items(self.table, scope, outlets)
        handed = upstream.receive(self, items, scope, self.beat_no)
        run = ScopeRun(scope=scope, status=status, depth=depth,
                       beats=self.beats_in_scope.get(scope, 0), handed=handed)
        self.scope_runs[scope] = run
        self.stopped_scopes[scope] = status
        return run

    # ——————————————————————— 包实例 ———————————————————————
    def run_pack_instance(self, pobs: PackObserver, inlet_item: Item, ctx: Ctx) -> list[dict]:
        """`PackObserver.observe` 的实现（§3.9）。返回要在父分区新建的草稿。"""
        defn = pobs.defn
        parent_scope = ctx.scope
        beat = ctx.beat
        if self.run_stop:
            return []
        if pobs.depth > self.params.max_pack_depth:
            self.log.emit("guard", beat=beat, scope=parent_scope,
                          guard=guard_mod.PACK_DEPTH, pack=defn.name, depth=pobs.depth,
                          limit=self.params.max_pack_depth, item=inlet_item.id)
            return [escalation.unsure_draft(
                defn.name, inlet_item, 1.0, "",
                {"guard": guard_mod.PACK_DEPTH, "pack": defn.name, "depth": pobs.depth,
                 "limit": self.params.max_pack_depth})]

        scope = pobs.instance_scope([inlet_item.id])
        if scope in self.scope_runs:
            # 同一个包、同样的入料内容已经跑过一个实例（实例分区名按 inlet 内容算）。
            # 再跑一遍只会得到同一批东西，直接把上次交出来的再交一次。
            self.log.emit("pack_reuse", beat=beat, scope=parent_scope, pack=defn.name,
                          instance=scope, item=inlet_item.id)
            return list(self.scope_runs[scope].handed)

        rep = BeatReport(beat=beat, scope=scope)
        copies = self._add_items([{"kind": inlet_item.kind, "body": inlet_item.body,
                                   "about": inlet_item.about}],
                                 pobs.made_by, scope, beat, rep)
        self.log.emit("pack_copy", beat=beat, scope=scope, pack=defn.name, depth=pobs.depth,
                      parent=parent_scope, origin=inlet_item.id, copy=copies[0])

        obs = self._instance_observers(defn, pobs.depth)
        # 预置 once：这个实例就是为这件东西而生的，它里面的同名包不能再为同一件
        # 东西开一个实例——不挡这一下，`drill` 这种"包里有自己"的递归包会在自己的
        # inlet 复制件上无限往下开实例，一层都推进不了。
        once = self.once.setdefault(scope, set())
        copy_item = self.table.get(copies[0])
        for o in obs:
            if getattr(o, "eye_type", "") == PACK_EYE and o.defn.name == defn.name:
                _v, vfp, _t = build_view(copy_item, self.table, o.view, self.params)
                once.add(H(o.name, o.version, vfp))

        run = self.run_scope(scope, obs, ScopeUpstream(parent_scope), depth=pobs.depth,
                             beats_budget=defn.beats_budget or self.params.beats_budget,
                             outlets=defn.outlets)
        self.log.emit("pack_done", beat=self.beat_no, scope=scope, pack=defn.name,
                      depth=pobs.depth, parent=parent_scope, status=run.status,
                      beats=run.beats, handed=len(run.handed))
        return list(run.handed)

    # ——————————————————————— 一拍 ———————————————————————
    def beat(self, scope: str = "/") -> BeatReport:
        self.beat_no += 1
        b = self.beat_no
        self.beats_in_scope[scope] = self.beats_in_scope.get(scope, 0) + 1
        rep = BeatReport(beat=b, scope=scope)
        self.log.emit("beat_start", beat=b, scope=scope)

        self._apply_pending_answers(scope, b, rep)

        pairs = self._pairs(scope, b)
        groups = self._group(pairs)
        answers = self._dispatch(groups, b, rep)
        readings = self._readings(groups, answers, b, rep)
        proposals, unsures = self._outlets(readings, b)

        arb = arbitrate(proposals)
        for rec in arb.records:
            self.log.emit("arbitration", beat=b, scope=scope, **rec)
        for tie in arb.ties:
            self.log.emit("guard", beat=b, scope=scope, guard=guard_mod.TIE_CONFLICT,
                          units=[p.unit.name for p in tie], item=tie[0].item.id)
            unsures.append((tie[0].unit, tie[0].item,
                            escalation.unsure_draft(tie[0].unit.name, tie[0].item,
                                                    tie[0].reading.p, "",
                                                    {"guard": guard_mod.TIE_CONFLICT,
                                                     "tie": sorted(p.unit.name for p in tie)})))

        for p in sorted(proposals, key=lambda p: p.key()):
            self.log.emit("proposal", beat=b, scope=scope, **p.to_dict())

        self._execute(arb.winners, scope, b, rep)
        self._emit_unsures(unsures, scope, b, rep)
        rep.stop = self._guards(scope, b, rep)

        self.log.emit("beat_end", beat=b, scope=scope, new_items=rep.new_items,
                      calls=rep.calls, cost=rep.cost)
        self.history.append(rep)
        return rep

    # ——————————————————————— 配对 ———————————————————————
    def _pairs(self, scope: str, beat: int) -> list[dict]:
        events = self.log.events()
        blocked = escalation.blocked_items(self.table, events, scope)
        once = self.once.setdefault(scope, set())
        out: list[dict] = []
        for item in sorted(self.table.alive(scope), key=lambda i: i.id):
            if item.id in blocked:
                continue
            if self.guards.chain_stopped(self.table, item.id):
                continue
            for unit in self.observers(scope):
                st = self.states.get(unit.name)
                if st is None or st.status == SUSPENDED:
                    continue
                if not unit.watches(item):
                    continue
                view, view_fp, truncated = build_view(item, self.table, unit.view, self.params)
                if truncated:
                    self.log.emit("guard", beat=beat, scope=scope,
                                  guard=guard_mod.VIEW_TRUNCATED, unit=unit.name, item=item.id)
                key = H(unit.name, unit.version, view_fp)
                if self.params.guard_on(guard_mod.ONCE) and key in once:
                    continue
                once.add(key)
                out.append({"unit": unit, "item": item, "view": view, "view_fp": view_fp,
                            "once_key": key})
        return out

    def _group(self, pairs: list[dict]) -> list[dict]:
        buckets: dict[tuple[str, str], dict] = {}
        for p in pairs:
            k = (p["item"].id, p["view_fp"])
            g = buckets.setdefault(k, {"item": p["item"], "view": p["view"],
                                       "view_fp": p["view_fp"], "pairs": []})
            g["pairs"].append(p)
        groups = [buckets[k] for k in sorted(buckets)]
        for g in groups:
            g["pairs"].sort(key=lambda p: p["unit"].name)
        return groups

    # ——————————————————————— 调用 ———————————————————————
    def _dispatch(self, groups: list[dict], beat: int, rep: BeatReport) -> dict[tuple, dict]:
        """返回 {(item_id, view_fp, unit_name): 原始答案}。并发调用，主线程写 Log。"""
        answers: dict[tuple, dict] = {}
        tasks: list[dict] = []
        for gi, g in enumerate(groups):
            jev_pairs = [p for p in g["pairs"] if p["unit"].eye_type == "jev"]
            if not jev_pairs:
                continue
            cached: dict[str, dict] = {}
            todo: list[dict] = []
            for p in jev_pairs:
                qfp = p["unit"].jev_eye.fp()
                key = ledger_key(g["view_fp"], qfp, self.params.model_version,
                                 include_batch=self.params.ledger_key_includes_batch)
                hit = self.ledger.get(key)
                if hit is not None:
                    cached[p["unit"].name] = hit
                    answers[(g["item"].id, g["view_fp"], p["unit"].name)] = hit
                else:
                    todo.append(p)
            if cached:
                self.log.emit("ask", beat=beat, scope=g["item"].scope, view_fp=g["view_fp"],
                              question_fps={p: self.by_name[p].jev_eye.fp() for p in sorted(cached)},
                              cache_hit=True, request=None, response=None, cost=0.0,
                              input_tokens=0)
            size = max(1, self.params.max_questions_per_call)
            for bi in range(0, len(todo), size):           # 超出上限按 unit 名排序切批
                tasks.append({"gi": gi, "bi": bi // size, "group": g, "pairs": todo[bi:bi + size]})

        if tasks:
            def work(t):
                g, ps = t["group"], t["pairs"]
                questions = {p["unit"].name: p["unit"].jev_eye.question() for p in ps}
                qfps = {p["unit"].name: p["unit"].jev_eye.fp() for p in ps}
                return t, self.eye.ask(g["view_fp"], g["view"], questions, qfps), qfps

            with ThreadPoolExecutor(max_workers=max(1, self.params.max_concurrency)) as ex:
                done = list(ex.map(work, tasks))
            done.sort(key=lambda d: (d[0]["group"]["item"].id, d[0]["group"]["view_fp"], d[0]["bi"]))
            for t, res, qfps in done:                      # 排好序再写 Log
                g = t["group"]
                self.log.emit("ask", beat=beat, scope=g["item"].scope, view_fp=g["view_fp"],
                              question_fps=qfps, cache_hit=False, request=res.request,
                              response=res.response, cost=res.cost_usd,
                              input_tokens=res.input_tokens)
                rep.calls += 1
                rep.cost += res.cost_usd
                self.guards.add_cost(res.cost_usd)
                for p in t["pairs"]:
                    name = p["unit"].name
                    ans = res.answers[name]
                    answers[(g["item"].id, g["view_fp"], name)] = ans
                    self.ledger.put(ledger_key(g["view_fp"], qfps[name],
                                               self.params.model_version,
                                               include_batch=self.params.ledger_key_includes_batch),
                                    ans)
        return answers

    # ——————————————————————— 读数 ———————————————————————
    def _readings(self, groups: list[dict], answers: dict, beat: int,
                  rep: BeatReport) -> list[dict]:
        out: list[dict] = []
        ctx = Ctx(table=self.table, beat=beat, scope="", writer=self.writer, run_id=self.run_id)
        for g in groups:
            ctx.scope = g["item"].scope
            for p in g["pairs"]:
                unit, item = p["unit"], p["item"]
                st = self.states[unit.name]
                if unit.eye_type == PACK_EYE:
                    # 包的眼：看见自己的 inlet 种类就是 act，不花一次调用。
                    r = read_code(("act", {"pack": unit.name, "depth": unit.depth}))
                elif unit.eye_type == "code":
                    from foundation.core import registry
                    r = read_code(registry.lookup(unit.code_eye.fn_name)(p["view"], item, ctx))
                else:
                    ans = answers.get((item.id, g["view_fp"], unit.name))
                    if ans is None:
                        continue
                    r = read_answer(unit.primitive, ans, st.hi, st.lo,
                                    delta=self.params.delta_for(unit.primitive))
                out.append({"unit": unit, "item": item, "view": p["view"],
                            "view_fp": g["view_fp"], "reading": r})
        out.sort(key=lambda d: (d["item"].id, d["unit"].name))
        for d in out:
            r = d["reading"]
            self.log.emit("reading", beat=beat, scope=d["item"].scope, unit=d["unit"].name,
                          item=d["item"].id, view_fp=d["view_fp"], value=r.value, p=r.p,
                          outlet=r.outlet, detail=r.detail, status=self.states[d["unit"].name].status,
                          hi=self.states[d["unit"].name].hi, lo=self.states[d["unit"].name].lo,
                          delta=self.params.delta_for(d["unit"].primitive))
            rep.readings += 1
        return out

    def _outlets(self, readings: list[dict], beat: int) -> tuple[list[Proposal], list]:
        proposals: list[Proposal] = []
        unsures: list = []
        for d in readings:
            unit, item, r = d["unit"], d["item"], d["reading"]
            st = self.states[unit.name]
            if r.outlet == ACT:
                proposals.append(Proposal(unit=unit, item=item, reading=r, hand=unit.hand,
                                          downgraded=(st.status == DRAFT), view=d["view"]))
            elif r.outlet == UNSURE:
                unsures.append((unit, item,
                                escalation.unsure_draft(unit.name, item, r.p, d["view_fp"])))
        return proposals, unsures

    # ——————————————————————— 执行 ———————————————————————
    def _execute(self, winners: list[Proposal], scope: str, beat: int, rep: BeatReport) -> None:
        ctx = Ctx(table=self.table, beat=beat, scope=scope, writer=self.writer, run_id=self.run_id)
        for prop in winners:                     # 已按 (layer, -priority, name, item) 排好
            outcome = execute_hand(prop, ctx)
            if outcome.write_used:
                self.guards.add_write_call()
                view = prop.view if isinstance(prop.view, str) else canon(prop.view)
                self.log.emit("write_call", beat=beat, scope=scope, unit=prop.unit.name,
                              item=prop.item.id,
                              instruction=outcome.detail.get("instruction", ""),
                              write_fp=write_key(outcome.detail.get("instruction", ""), view),
                              ok=not outcome.failed, text=outcome.detail.get("text", ""),
                              error=outcome.failed)
            if outcome.failed:
                self.log.emit("action", beat=beat, scope=scope, unit=prop.unit.name,
                              hand=prop.hand_type, item=prop.item.id, result_ids=[],
                              failed=outcome.failed)
                self._add_items([escalation.unsure_draft(prop.unit.name, prop.item,
                                                         prop.reading.p, "",
                                                         {"failed": outcome.failed})],
                                prop.unit.made_by, scope, beat, rep)
                continue
            ids = self._add_items(outcome.drafts, prop.unit.made_by, scope, beat, rep)
            self.log.emit("action", beat=beat, scope=scope, unit=prop.unit.name,
                          hand=prop.hand_type, item=prop.item.id, result_ids=ids)
            if ids:
                self.guards.note_mover(scope, prop.unit.name)

    def _add_items(self, drafts: list[dict], made_by: str, scope: str, beat: int,
                   rep: BeatReport) -> list[str]:
        ids = []
        for d in drafts:
            item = draft_to_item(d, made_by=made_by, scope=scope, beat=beat)
            before = self.table.ids()
            self.table._absorb(item)
            self.log.emit("item", beat=beat, scope=scope, item=item.to_dict())
            ids.append(item.id)
            if item.id not in before:
                rep.new_items += 1
        return ids

    def _emit_unsures(self, unsures: list, scope: str, beat: int, rep: BeatReport) -> None:
        for unit, item, draft in sorted(unsures, key=lambda t: (t[1].id, t[0].name)):
            ids = self._add_items([draft], unit.made_by, scope, beat, rep)
            self.log.emit("escalate", beat=beat, scope=scope, unit=unit.name, item=item.id,
                          unsure=ids[0] if ids else None)
            rep.escalated += 1

    # ——————————————————————— 保险 ———————————————————————
    def _guards(self, scope: str, beat: int, rep: BeatReport) -> str | None:
        hit = self.guards.check_cost_budget(scope)
        if hit:
            self._log_guard(hit, beat)
            self._escalate_stop(scope, hit.name, hit.detail)
            return self._stop(BUDGET_STOP)
        for item in sorted(self.table.alive(scope), key=lambda i: i.id):
            gen = self.table.generation(item.id)
            hit = self.guards.check_runaway(scope, item.id, gen)
            if hit:
                self._log_guard(hit, beat)
                self._escalate_stop(scope, hit.name, hit.detail)
                return GUARD_STOP
        if rep.new_items:
            hit = self.guards.check_return_to_same(scope, self.table.fingerprint(scope))
            if hit:
                self._log_guard(hit, beat)
                self._escalate_stop(scope, hit.name, hit.detail)
                return GUARD_STOP
        hit = self.guards.check_beat_budget(scope, self.beats_in_scope[scope],
                                            self.scope_budget.get(scope))
        if hit:
            self._log_guard(hit, beat)
            self._escalate_stop(scope, hit.name, hit.detail)
            return GUARD_STOP
        return None

    def _stop(self, status: str) -> str:
        """全局的停止要置位 run_stop，好让每一层的 run_scope 都在下一拍开头看见它。"""
        if status in GLOBAL_STOPS:
            self.run_stop = status
        return status

    def _log_guard(self, hit, beat: int) -> None:
        self.log.emit("guard", beat=beat, scope=hit.scope, guard=hit.name, **hit.detail)

    def _escalate_stop(self, scope: str, guard_name: str, detail: dict) -> None:
        rep = BeatReport(beat=self.beat_no, scope=scope)
        self._add_items([{"kind": "unsure",
                          "body": {"guard": guard_name, **detail}, "about": None}],
                        "system", scope, self.beat_no, rep)

    # ——————————————————————— 人答 ———————————————————————
    def _apply_pending_answers(self, scope: str, beat: int, rep: BeatReport) -> None:
        """人答的动作在下一拍开头执行（§3.8）。"""
        for ev in self.log.events():
            if ev.get("t") != "answer" or ev.get("about") in self.applied_answers:
                continue
            aid = ev["about"]
            self.applied_answers.add(aid)
            unsure = self.table.get(aid)
            if unsure is None:
                continue
            body = unsure.body if isinstance(unsure.body, dict) else {}
            unit = self.by_name.get(body.get("unit") or ev.get("unit"))
            target = self.table.get(unsure.about) if unsure.about else None
            if ev.get("outlet") != ACT or unit is None or target is None:
                continue
            r = Reading(value=body.get("p", 1.0), p=float(body.get("p") or 1.0),
                        outlet=ACT, detail={"source": "human"})
            view, _fp, _tr = build_view(target, self.table, unit.view, self.params)
            prop = Proposal(unit=unit, item=target, reading=r, hand=unit.hand, view=view)
            self._execute([prop], scope, beat, rep)
