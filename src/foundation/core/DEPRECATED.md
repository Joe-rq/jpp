# core 里已由 jv 取代、不再被引用的模块（2026-09-21）

| 模块 | 状态 | 取代者 |
|---|---|---|
| `beat.py`（心跳引擎） | 废弃，不删；旧测试仍跑 | `jv/runtime.py` 的刷新点 + 层（惰性效应、需求驱动） |
| `pack.py`（包/观察者） | 废弃，不删 | 无对应物；组合由宿主控制流承担 |
| `arbiter.py`（提案裁决） | 废弃，不删 | `jv` 的出口 + handler 库；`do` 守卫走 J-08 |
| `outlet.py` | 保留（`score_level` 被 jv 复用）；三分出口逻辑由 `jv/runtime.py::cut` 取代 | |
| `calib.py`、`view.py`、`checker.py`、`unit.py`、`probe.py`、`hand.py`、`guard.py`、`escalation.py` | 旧内核（v0 单元/试验台）继续可用，jv 不引用 | `jv/calib.py`、`jv/ir.py::State.render`、`jv/checker.py` |
| `item/log/table/canon/ledger/eye/registry` | **沿用** | jv 直接 import |
