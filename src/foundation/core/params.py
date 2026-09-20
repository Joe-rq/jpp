"""全局参数（施工单 §3.4 / §3.7 / §4）。

数值来自第零步《前提结论》(`foundation/experiments/前提结论.md`, 2026-09-20)，
没测到的才用施工单默认值。契约实测只允许改这些全局参数，永远不改单个单元的线。
"""

from __future__ import annotations

from dataclasses import dataclass, asdict, field


@dataclass
class Params:
    # —— Jev 调用 ——
    model_version: str = "jev-1.13.0"
    # E4（补测版，顺序随机化 + 热身请求排除冷启动）：ratio(50问/1问)=1.039，
    # ratio(200问/1问)=1.22——宽度成本远低于线性，但不是零。200 是量过的最大规模，
    # 拿它当切批大小；再往上没人量过，耗时会随 n 温和上升，别默默放行。
    max_questions_per_call: int = 200
    max_concurrency: int = 6                  # E4 定，默认 6
    # E5：掺纯背景性中性材料，到 8000 token（按 1.293 字符/token 约 10300 字符）中位漂移
    # 仍 ≤0.05，但个别贴边候选最大漂移 0.08-0.09、样本只有 6 对。所以留在施工单的 6000 字符
    # （≈4600 token），在测到的安全区里面再收一道。真正危险的是"往视野里塞另一份带主张的
    # 文档"——500 token 就能把判断的标的换掉，那是单元设计要人工把关的事，不是调这个数。
    view_char_limit: int = 6000
    usd_per_input_token: float = 0.042 / 1e6

    # —— 账本 ——
    # E2 字面不过，但诊断出"并排"本身不额外引入偏差：补充问题从 10 涨到 200、顺序打乱，
    # 均值偏移都不跟着变大，测到的差就是 E1 的噪声地板。所以不采纳"键里加批次指纹"这一
    # 原定药方（加了只会让账本永远不命中），保持 False；真正需要的是容忍 δ 量级噪声，
    # 那件事由下面的迟滞做。
    ledger_key_includes_batch: bool = False

    # —— 两条线（§3.4）——
    default_hi: float = 0.65
    default_lo: float = 0.35
    safety_default_hi: float = 0.75
    safety_default_lo: float = 0.25
    calib_min_n: int = 20
    eps_act: float = 0.05
    eps_ignore: float = 0.05
    # E1 不过（exact match 34.7%，最大偏差 0.18），按红队 §133 给两条线加迟滞：
    # 线附近 ±δ 的读数一律算拿不准，送人。δ 按原语分开——noul 的噪声明显小于 choice/score。
    delta_noul: float = 0.05                  # E1 的 p99；max 是 0.09
    delta_choice: float = 0.15                # p99 0.10-0.15，max 0.18，选中项本身约 3% 会换
    delta_score: float = 0.15
    question_language: str = "zh"             # E3：默认中文；单元缝不够时先改句子，再试英文

    # —— 验题闸门（§3.3）——
    gap_threshold: float = 0.20
    safety_gap_threshold: float = 0.30
    score_tolerance: float = 0.5

    # —— 保险（§3.7）——
    beats_budget: int = 30
    runaway_generations: int = 6
    cost_usd: float = 1.0
    write_calls: int = 40
    max_pack_depth: int = 4

    # —— 写手 ——
    writer_timeout_s: int = 90
    writer_model: str = "haiku"

    disabled_guards: tuple[str, ...] = field(default_factory=tuple)

    def delta_for(self, primitive: str | None) -> float:
        """某个原语的迟滞带宽（E1）。code 眼没有读数噪声，δ=0。"""
        return {"noul": self.delta_noul, "choice": self.delta_choice,
                "score": self.delta_score}.get(primitive or "", 0.0)

    def guard_on(self, name: str) -> bool:
        return name not in self.disabled_guards

    def to_dict(self) -> dict:
        d = asdict(self)
        d["disabled_guards"] = list(self.disabled_guards)
        return d


DEFAULTS = Params()
LAYER_RANK = {"safety": 0, "correctness": 1, "efficiency": 2}
