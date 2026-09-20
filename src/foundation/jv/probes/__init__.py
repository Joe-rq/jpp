"""E-PROBE-10：设计/E §26.5 十条「可运行级」探针（建造第 5 步）。

跑：`cd 地基 && .venv/bin/python -m foundation.jv.probes [--real] [--only p13,p17] [--tag t]`
"""

from . import p13, p17, p22, p42, p45, p49, p77, p81, p85, p87  # noqa: F401  注册
from ._common import PROBES, Probe, Result, Sample, main, run_probe  # noqa: F401

ALL = list(PROBES)
