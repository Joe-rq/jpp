"""判断客户端：真机 JevClient（包 clients.eye_client，密钥只从 ~/.typesafe-key 读）与测试用 FakeClient。

协议：`ask(state_json, questions: {qid: {type, instructions, criteria?}}) -> (answers, input_tokens, cost_usd)`。
"""

from __future__ import annotations

import json
import re
from typing import Any, Callable

from foundation.core.canon import H


class JevClient:
    def __init__(self, model_version: str = "jev-1.13.0", transport: Callable[[dict], dict] | None = None):
        from foundation.clients.eye_client import EyeClient
        from foundation.core.params import Params
        self.params = Params(model_version=model_version)
        self._eye = EyeClient(mode="record", params=self.params, transport=transport)
        self.model_id = model_version
        self.calls = 0

    def ask(self, state: Any, questions: dict[str, dict]) -> tuple[dict, int, float]:
        qfps = {k: H(v) for k, v in questions.items()}
        res = self._eye.ask(H(state), state, questions, qfps)
        self.calls += 1
        return res.answers, res.input_tokens, res.cost_usd


class FakeClient:
    """确定性读数。默认规则：noul 看题面关键词是否出现在状态里；choice 选第一个在状态里出现的选项；
    score 按状态里「！」个数。可用 `rule(state_text, qid, q) -> answer|None` 覆盖。"""

    def __init__(self, rule: Callable[[str, str, dict], dict | None] | None = None,
                 model_id: str = "fake-0", fail_on: Callable[[Any], bool] | None = None):
        self.rule = rule
        self.model_id = model_id
        self.calls = 0
        self.questions_asked = 0
        self.log: list[dict] = []
        self.fail_on = fail_on

    def ask(self, state: Any, questions: dict[str, dict]) -> tuple[dict, int, float]:
        if self.fail_on and self.fail_on(state):
            raise RuntimeError("FakeClient 故意失败")
        text = state if isinstance(state, str) else json.dumps(state, ensure_ascii=False)
        answers = {}
        for qid, q in questions.items():
            a = self.rule(text, qid, q) if self.rule else None
            if a is None:
                a = self._default(text, q)
            answers[qid] = a
        self.calls += 1
        self.questions_asked += len(questions)
        self.log.append({"state": state, "questions": questions, "answers": answers})
        tokens = int(len(text) / 1.3) + 271 + 38 * len(questions)
        return answers, tokens, tokens * 4.2e-8

    @staticmethod
    def _default(text: str, q: dict) -> dict:
        t = q["type"]
        if t == "noul":
            ins = re.sub(r"[吗？?。，,：:的了是有在这那对候选]", " ", q["instructions"])
            grams = set()
            for w in ins.split():
                if re.fullmatch(r"[A-Za-z0-9_]+", w):
                    grams.add(w)
                else:
                    grams |= {w[i:i + 2] for i in range(len(w) - 1)}
            hit = any(g in text for g in grams if len(g) >= 2)
            return {"type": "noul", "noul": 0.93 if hit else 0.05}
        if t == "choice":
            opts = list(q["criteria"].keys())
            pick = next((o for o in opts if q["criteria"][o] and str(q["criteria"][o]) in text), opts[0])
            probs = {o: (0.86 if o == pick else round(0.14 / max(1, len(opts) - 1), 4)) for o in opts}
            return {"type": "choice", "choice": pick, "probabilities": probs, "confidence": 0.8}
        levels = q["criteria"]
        lvl = min(len(levels) - 1, text.count("！"))
        probs = {str(i): (0.8 if i == lvl else round(0.2 / max(1, len(levels) - 1), 4)) for i in range(len(levels))}
        return {"type": "score", "score": float(lvl), "probabilities": probs, "confidence": 0.8}
