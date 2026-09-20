# -*- coding: utf-8 -*-
"""包（施工单 §3.9 / §2.5）。

包在心跳眼里**与单元同接口**：`PackObserver` 鸭子类型成一个 Unit——有 `watches()`、
`eye_type`、`view`、`hand`、`layer`、`priority`、`made_by`，所以 `core/beat.py` 的配对、
读数、提议、裁决、执行五步对它和对一个普通单元走的是同一条路，没有"这是包所以特殊处理"
的分支，更没有按层数分支（S5.3）。

它与普通单元只差在两个地方，都是"数据不同"而不是"代码路径不同"：
- `eye_type == "pack"`：看见自己的 inlet 种类就是 act（实例化的条件就是"有东西进来"），
  不花一次 Jev 调用；
- `hand == {"type": "pack"}`：手的内容是"跑一个子分区到安静，把 outlets 与未吸收的
  unsure 交给上一层"，由 `core/hand.py::execute` 转给 `PackObserver.observe()`。

**递归深度**：`PackObserver.depth` 是"这个观察者会造出来的实例分区的深度"。根分区
深度 0，根上挂的包造出深度 1 的实例，实例里面的包造出深度 2……`depth > max_pack_depth`
（默认 4）就拒绝实例化并上交（§3.9）。这是一道保险的阈值判断，不是按层数走不同代码。
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field

import yaml

from foundation.core.canon import H
from foundation.core.params import LAYER_RANK

PACK_HAND = "pack"
PACK_EYE = "pack"


@dataclass
class PackDef:
    """包定义（§2.5）。`layer` / `priority` 不在施工单的 YAML 示意里，
    但包要和单元一起进裁决就必须有这两个字段；缺省与单元的缺省一致。"""
    name: str
    inlets: tuple[str, ...] = ()
    outlets: tuple[str, ...] = ()
    units: tuple[str, ...] = ()
    packs: tuple[str, ...] = ()
    budget: dict = field(default_factory=dict)
    layer: str = "correctness"
    priority: int = 0
    prefilter: str | None = None      # 与单元的 watches.prefilter 同一个约定：fn(item) -> bool
    source_path: str | None = None

    @property
    def beats_budget(self) -> int | None:
        b = self.budget.get("beats")
        return int(b) if b is not None else None

    def fp(self) -> str:
        return H(self.name, sorted(self.inlets), sorted(self.outlets), sorted(self.units),
                 sorted(self.packs), self.budget, self.layer, self.priority, self.prefilter)


def load_pack(path: str) -> PackDef:
    with open(path, encoding="utf-8") as fh:
        d = yaml.safe_load(fh) or {}
    return PackDef(
        name=d.get("pack") or os.path.splitext(os.path.basename(path))[0],
        inlets=tuple(d.get("inlets") or ()),
        outlets=tuple(d.get("outlets") or ()),
        units=tuple(d.get("units") or ()),
        packs=tuple(d.get("packs") or ()),
        budget=dict(d.get("budget") or {}),
        layer=d.get("layer", "correctness"),
        priority=int(d.get("priority", 0)),
        prefilter=(d.get("watches") or {}).get("prefilter") or d.get("prefilter"),
        source_path=path,
    )


def load_packs(testbed_dir: str) -> dict[str, PackDef]:
    """试验台里的全部包：`pack.yaml`（施工单 §6 的位置）+ `packs/*.yaml`（多个包时）。"""
    out: dict[str, PackDef] = {}
    single = os.path.join(testbed_dir, "pack.yaml")
    if os.path.exists(single):
        d = load_pack(single)
        out[d.name] = d
    many = os.path.join(testbed_dir, "packs")
    if os.path.isdir(many):
        for fn in sorted(os.listdir(many)):
            if fn.endswith((".yaml", ".yml")):
                d = load_pack(os.path.join(many, fn))
                out[d.name] = d
    return out


def packs_fingerprint(packs: dict[str, PackDef]) -> str:
    return H(sorted(d.fp() for d in packs.values()))


class PackObserver:
    """与单元同接口的观察者。一个 (包定义, 深度) 对应一个观察者。"""

    eye_type = PACK_EYE
    jev_eye = None
    code_eye = None
    view = "self"
    version = 1
    exclusive_with: tuple[str, ...] = ()
    probes_path = None
    source_path = None
    is_pack = True

    def __init__(self, defn: PackDef, engine, depth: int):
        self.defn = defn
        self.engine = engine
        self.depth = depth
        self.name = defn.name
        self.watch_kinds = tuple(defn.inlets)
        self.hand = {"type": PACK_HAND}
        self.layer = defn.layer
        self.priority = defn.priority
        self.prefilter_name = defn.prefilter

    # —— 与 Unit 同名的查询 ——
    @property
    def layer_rank(self) -> int:
        return LAYER_RANK.get(self.layer, 1)

    @property
    def is_safety(self) -> bool:
        return self.layer == "safety"

    @property
    def primitive(self) -> str | None:
        return None

    @property
    def made_by(self) -> str:
        return f"pack:{self.name}"

    def watches(self, item) -> bool:
        if item.kind not in self.watch_kinds:
            return False
        if self.prefilter_name:
            from foundation.core import registry
            try:
                return bool(registry.lookup(self.prefilter_name)(item))
            except KeyError:
                return False
        return True

    def question_fp(self) -> str:
        return H("pack", self.defn.fp())

    def fp(self) -> str:
        return H("pack", self.defn.fp(), self.depth)

    def default_lines(self, params):
        return params.default_hi, params.default_lo

    def gap_threshold(self, params) -> float:
        return params.gap_threshold

    # —— 实例 ——
    def instance_scope(self, inlet_ids: list[str]) -> str:
        return f"/{self.defn.name}/{H(sorted(inlet_ids))}"

    def observe(self, item, ctx) -> list[dict]:
        """与单元的手同一个返回约定：一串 Item 草稿，由内核补 made_by / scope / beat。"""
        return self.engine.run_pack_instance(self, item, ctx)
