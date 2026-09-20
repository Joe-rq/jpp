"""单元定义与运行态（施工单 §2.4 / §2.6）。

加载器故意宽容：可选字段一律有默认，probes 路径缺省 probes/<unit>.yaml，
score 的 expect 允许整数或档位描述原文。
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Any

import yaml

from foundation.core.canon import H
from foundation.core.eye import CodeEye, JevEye
from foundation.core.params import LAYER_RANK, Params
from foundation.core import registry

DRAFT, ON_DUTY, SUSPENDED = "draft", "on_duty", "suspended"


@dataclass
class Unit:
    name: str
    version: int = 1
    watch_kinds: tuple[str, ...] = ()
    prefilter_name: str | None = None
    eye_type: str = "jev"
    jev_eye: JevEye | None = None
    code_eye: CodeEye | None = None
    view: str = "self"
    hand: dict = field(default_factory=dict)
    layer: str = "correctness"
    priority: int = 0
    exclusive_with: tuple[str, ...] = ()
    probes_path: str | None = None
    source_path: str | None = None

    # —— 查询 ——
    @property
    def layer_rank(self) -> int:
        return LAYER_RANK.get(self.layer, 1)

    @property
    def is_safety(self) -> bool:
        return self.layer == "safety"

    @property
    def primitive(self) -> str | None:
        return self.jev_eye.primitive if self.jev_eye else None

    @property
    def made_by(self) -> str:
        return f"unit:{self.name}@{self.version}"

    def watches(self, item) -> bool:
        if self.watch_kinds and item.kind not in self.watch_kinds:
            return False
        # 注意：不加"不看自己产出的东西"这条——S3.3 的 flipflop 与 S3.4 的 grow 正是
        # 靠自己看自己的产出才跑得起来；挡重复是 once 与四道保险的事（施工单 §3.5/§3.7）。
        if self.prefilter_name:
            try:
                return bool(registry.lookup(self.prefilter_name)(item))
            except KeyError:
                return False
        return True

    def question_fp(self) -> str:
        return self.jev_eye.fp() if self.jev_eye else self.code_eye.fp()

    def fp(self) -> str:
        """单元定义指纹，用于 run_start 的 defs 指纹。"""
        return H(self.name, self.version, sorted(self.watch_kinds), self.prefilter_name,
                 self.eye_type, self.question_fp(), self.view, self.hand, self.layer,
                 self.priority, sorted(self.exclusive_with))

    def default_lines(self, params: Params) -> tuple[float, float]:
        if self.is_safety:
            return params.safety_default_hi, params.safety_default_lo
        return params.default_hi, params.default_lo

    def gap_threshold(self, params: Params) -> float:
        return params.safety_gap_threshold if self.is_safety else params.gap_threshold


def load_unit(path: str) -> Unit:
    with open(path, encoding="utf-8") as fh:
        d = yaml.safe_load(fh) or {}
    name = d.get("unit") or os.path.splitext(os.path.basename(path))[0]
    watches = d.get("watches") or {}
    kinds = watches.get("kinds") or []
    if isinstance(kinds, str):
        kinds = [kinds]
    eye = d.get("eye") or {}
    eye_type = eye.get("type", "jev")
    jev_eye = code_eye = None
    if eye_type == "jev":
        jev_eye = JevEye(primitive=eye.get("primitive", "noul"),
                         instructions=eye.get("instructions", ""),
                         criteria=eye.get("criteria"))
    elif eye_type == "code":
        code_eye = CodeEye(fn_name=eye.get("fn") or eye.get("name") or f"{name}_eye")
    else:
        raise ValueError(f"{name}: 今晚只支持 jev / code 眼，收到 {eye_type}（model 眼推迟）")
    probes = d.get("probes")
    if probes is None:
        probes = os.path.join("probes", f"{name}.yaml")
    hand = dict(d.get("hand") or {})
    return Unit(
        name=name,
        version=int(d.get("version", 1)),
        watch_kinds=tuple(kinds),
        prefilter_name=watches.get("prefilter"),
        eye_type=eye_type,
        jev_eye=jev_eye,
        code_eye=code_eye,
        view=d.get("view", "self"),
        hand=hand,
        layer=d.get("layer", "correctness"),
        priority=int(d.get("priority", 0)),
        exclusive_with=tuple(d.get("exclusive_with") or ()),
        probes_path=probes,
        source_path=path,
    )


def load_units(defs_dir: str) -> list[Unit]:
    units = []
    for fn in sorted(os.listdir(defs_dir)):
        if fn.endswith((".yaml", ".yml")):
            units.append(load_unit(os.path.join(defs_dir, fn)))
    units.sort(key=lambda u: u.name)          # 与加载顺序无关（S2.5）
    return units


def defs_fingerprint(units: list[Unit]) -> str:
    return H(sorted(u.fp() for u in units))


@dataclass
class UnitState:
    """运行态（§2.6），随 run 存 state/units.json。"""
    name: str
    status: str = DRAFT
    gap: float | None = None
    hi: float = 0.65
    lo: float = 0.35
    calibrated: bool = False
    calib_n: int = 0
    probes_pass: bool = False
    note: str = ""
    degenerate: bool = False

    def to_dict(self) -> dict:
        return {"status": self.status, "gap": self.gap,
                "lines": {"hi": self.hi, "lo": self.lo, "calibrated": self.calibrated,
                          "degenerate": self.degenerate},
                "calib_n": self.calib_n, "probes_pass": self.probes_pass, "note": self.note}

    @staticmethod
    def from_dict(name: str, d: dict) -> "UnitState":
        lines = d.get("lines") or {}
        return UnitState(name=name, status=d.get("status", DRAFT), gap=d.get("gap"),
                         hi=lines.get("hi", 0.65), lo=lines.get("lo", 0.35),
                         calibrated=lines.get("calibrated", False),
                         degenerate=lines.get("degenerate", False),
                         calib_n=d.get("calib_n", 0), probes_pass=d.get("probes_pass", False),
                         note=d.get("note", ""))
