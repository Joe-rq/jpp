"""上交与人答（施工单 §3.8）。

unsure → 本分区有人接 → 没人接就上交父分区 → 根分区进人队列。
人答进校准集、重算两条线，**不写入验题集**（红队 A2）。
"""

from __future__ import annotations

import json
import os
from typing import Any

from foundation.core.calib import Lines, append_calib, compute_lines, load_calib, should_suspend
from foundation.core.item import Item
from foundation.core.params import Params
from foundation.core.unit import SUSPENDED, UnitState

ABSORBED = "absorbed"


def unsure_draft(unit_name: str, item: Item, p: float, view_fp: str,
                 extra: dict | None = None) -> dict:
    body = {"unit": unit_name, "p": p, "view_fp": view_fp}
    if extra:
        body.update(extra)
    return {"kind": "unsure", "body": body, "about": item.id}


def is_absorbed(table, unsure: Item) -> bool:
    for m in table.alive(unsure.scope):
        if m.kind == "mark" and m.about == unsure.id:
            label = (m.body or {}).get("label", "") if isinstance(m.body, dict) else ""
            if ABSORBED in str(label):
                return True
    return False


def answered_ids(events: list[dict]) -> set[str]:
    return {ev["about"] for ev in events if ev.get("t") == "answer" and ev.get("about")}


def open_queue(table, events: list[dict], root: str = "/") -> list[dict]:
    """根分区待人处理的东西：未吸收的 unsure，以及未答的 ask。"""
    done = answered_ids(events)
    out = []
    for it in table.alive(root):
        if it.id in done:
            continue
        if it.kind == "unsure" and not is_absorbed(table, it):
            body = it.body if isinstance(it.body, dict) else {}
            out.append({"id": it.id, "kind": "unsure", "about": it.about,
                        "unit": body.get("unit"), "p": body.get("p"),
                        "view_fp": body.get("view_fp"), "beat": it.beat,
                        "scope": it.scope, "body": body})
        elif it.kind == "ask":
            body = it.body if isinstance(it.body, dict) else {"question": it.body}
            out.append({"id": it.id, "kind": "ask", "about": it.about,
                        "unit": body.get("unit"), "question": body.get("question"),
                        "beat": it.beat, "scope": it.scope, "body": body})
    out.sort(key=lambda r: (r["beat"], r["id"]))
    return out


def save_queue(path: str, rows: list[dict]) -> None:
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(rows, fh, sort_keys=True, ensure_ascii=False, indent=2)


def blocked_items(table, events: list[dict], scope: str) -> set[str]:
    """ask 只挡被问的那一件东西（S3.7）。"""
    done = answered_ids(events)
    out = set()
    for it in table.alive(scope):
        if it.kind == "ask" and it.about and it.id not in done:
            out.add(it.about)
    return out


# ——————————————————— 向上一层搬运（施工单 §3.8 / §3.9）———————————————————
#
# "上一层"是一个只有一个方法的接口。包实例的上一层是父分区（ScopeUpstream），
# 根分区的上一层是人队列（HumanUpstream）——**两者走同一个调用点**，
# `core/beat.py::run_scope` 结尾无条件调一次 `upstream.receive(...)`，
# 没有 `if scope == "/"`，也没有按层数分支（S5.3）。
HANDOFF_ALWAYS = ("unsure", "ask")


def handoff_items(table, scope: str, outlets: tuple[str, ...] = ()) -> list:
    """分区安静时要交给上一层的东西：outlets 种类的 alive Item，
    外加未被 `absorbed` 标记吸收的 unsure 与未答的 ask。"""
    kinds = set(outlets) | set(HANDOFF_ALWAYS)
    out = []
    for it in sorted(table.alive(scope), key=lambda i: i.id):
        if it.kind not in kinds:
            continue
        if it.kind == "unsure" and is_absorbed(table, it):
            continue
        out.append(it)
    return out


class Upstream:
    """上一层的收件口。`receive` 返回要在上一层新建的 Item 草稿（可能为空）。"""

    kind = "upstream"

    def receive(self, engine, items: list, from_scope: str, beat: int) -> list[dict]:
        raise NotImplementedError


class ScopeUpstream(Upstream):
    """上一层是父分区：把东西复制过去（§3.9 "scope 改回"）。

    复制件**不带 supersedes**：被取代的那一代活在子分区里，父分区没有它，
    照抄 supersedes 会让父分区的 alive 计算去指一件它看不见的东西。
    `about` 照抄——Table 是全局的，子分区的 id 在父分区仍然查得到，
    上交单指回原物这件事跨分区仍然成立。
    """

    kind = "scope"

    def __init__(self, scope: str):
        self.scope = scope

    def receive(self, engine, items: list, from_scope: str, beat: int) -> list[dict]:
        engine.log.emit("handoff", beat=beat, scope=from_scope, to=self.scope,
                        to_kind=self.kind,
                        items=[{"id": it.id, "kind": it.kind} for it in items])
        return [{"kind": it.kind, "body": it.body, "about": it.about} for it in items]


class HumanUpstream(Upstream):
    """上一层是人：根分区的 unsure / ask 进人队列 `queue/<run>.json`（§3.8）。

    人队列不复制 Item——东西已经在根分区桌面上活着，再复制一份只会让桌面翻倍。
    这个收件口做的是"登记成待答条目"，返回空草稿列表。
    """

    kind = "human"

    def __init__(self):
        self.rows: list[dict] = []

    def receive(self, engine, items: list, from_scope: str, beat: int) -> list[dict]:
        engine.log.emit("handoff", beat=beat, scope=from_scope, to="human",
                        to_kind=self.kind,
                        items=[{"id": it.id, "kind": it.kind} for it in items])
        self.rows = open_queue(engine.table, engine.log.events(), from_scope)
        return []


def record_human_answer(calib_dir: str, unit_name: str, view_fp: str, p: float,
                        label: str, params: Params, state: UnitState,
                        safety: bool = False) -> dict:
    """人答 → 校准集 +1 → 重算两条线 → 必要时停岗。返回给 Log 的摘要。"""
    append_calib(calib_dir, unit_name,
                 {"view_fp": view_fp, "p": p, "label": label, "source": "human"})
    records = load_calib(calib_dir, unit_name)
    lines = compute_lines(records, params, safety=safety)
    working = Lines(hi=state.hi, lo=state.lo, calibrated=state.calibrated,
                    degenerate=state.degenerate, n=state.calib_n)
    suspend, why = should_suspend(records, lines, params, working=working)
    state.hi, state.lo = lines.hi, lines.lo
    state.calibrated = lines.calibrated
    state.degenerate = lines.degenerate
    state.calib_n = len(records)
    if suspend:
        state.status = SUSPENDED
        state.note = f"停岗：{why}"
    return {"unit": unit_name, "calib_n": len(records),
            "lines": {"hi": lines.hi, "lo": lines.lo, "calibrated": lines.calibrated,
                      "degenerate": lines.degenerate},
            "suspended": suspend, "why": why}


def recalibrate(calib_dir: str, unit_name: str, params: Params, state: UnitState,
                safety: bool = False) -> dict:
    records = load_calib(calib_dir, unit_name)
    lines = compute_lines(records, params, safety=safety)
    working = Lines(hi=state.hi, lo=state.lo, calibrated=state.calibrated,
                    degenerate=state.degenerate, n=state.calib_n)
    suspend, why = should_suspend(records, lines, params, working=working)
    state.hi, state.lo = lines.hi, lines.lo
    state.calibrated = lines.calibrated
    state.degenerate = lines.degenerate
    state.calib_n = len(records)
    if suspend:
        state.status = SUSPENDED
        state.note = f"停岗：{why}"
    # 注意：停过岗的单元不在这里自动放回去。停岗是人要看一眼的事，
    # 自动复岗会让"错误率超线"变成一阵一阵的闪灯（施工单 §3.4 只写了停岗条件）。
    return {"unit": unit_name, "calib_n": len(records),
            "lines": {"hi": lines.hi, "lo": lines.lo, "calibrated": lines.calibrated,
                      "degenerate": lines.degenerate},
            "suspended": suspend, "why": why}
