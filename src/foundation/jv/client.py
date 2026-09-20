"""判断客户端：真机 JevClient（包 clients.eye_client，密钥只从 ~/.typesafe-key 读）与测试用 FakeClient。

协议：`ask(state_json, questions: {qid: {type, instructions, criteria?}}) -> (answers, input_tokens, cost_usd)`。
"""

from __future__ import annotations

import json
import re
from typing import Any, Callable

from foundation.core.canon import H

from .ir import JvError


def validate_answers(questions: dict[str, dict], answers: Any) -> dict[str, dict]:
    """客户端返回体校验（真机与 FakeClient 共用，运行时每次调用后立即核）。

    键不合即 JvError 带修法，不许静默变成全 Unsure。返回规范化后的答案：
      noul  → {"type": "noul", "noul": p}                        p ∈ [0, 1]
      choice→ {"type": "choice", "choice": "c1", "probabilities": {"c0": …, "c1": …}}   键 = criteria 的键
      score → {"type": "score", "score": 档位下标(float), "probabilities": {"0": …, "1": …}}  键 = "0".."n-1"
    """
    if not isinstance(answers, dict):
        raise JvError(f"客户端返回体必须是 {{qid: 答案}} 字典，收到 {type(answers).__name__}")
    out: dict[str, dict] = {}
    for qid, q in questions.items():
        t = q["type"]
        a = answers.get(qid)
        if not isinstance(a, dict):
            raise JvError(f"客户端返回缺题 {qid}（{t}）或答案不是字典：{a!r}。修法：rule/客户端对每个 qid 返回一个字典")
        if t == "noul":
            v = a.get("noul")
            if not isinstance(v, (int, float)) or isinstance(v, bool) or not 0.0 <= float(v) <= 1.0:
                raise JvError(f"noul 题 {qid} 的返回体要 {{'type': 'noul', 'noul': p}}，p 是 0–1 的数；收到 {a!r}。"
                              f"修法：{{'type': 'noul', 'noul': 0.93}}")
            out[qid] = {"type": "noul", "noul": float(v)}
        elif t == "choice":
            opts = [str(k) for k in q["criteria"]]
            probs = a.get("probabilities")
            if not isinstance(probs, dict) or not probs:
                raise JvError(f"choice 题 {qid} 的返回体缺 probabilities（按选项键 {opts} 的字典）；收到 {a!r}。"
                              f"修法：{{'type': 'choice', 'choice': 'c0', 'probabilities': {{'c0': 0.9, 'c1': 0.1}}}}")
            bad = [k for k in probs if str(k) not in opts]
            if bad:
                raise JvError(f"choice 题 {qid} 的 probabilities 键 {bad} 不是选项键；选项键是 {opts}（候选按 over 下标叫 c0, c1, …）。"
                              f"修法：probabilities 的键用 q['criteria'] 的键")
            choice = a.get("choice")
            if choice is not None and str(choice) not in opts:
                raise JvError(f"choice 题 {qid} 的 choice={choice!r} 不是选项键 {opts}。修法：choice 用 q['criteria'] 的键")
            pr = {str(k): float(v) for k, v in probs.items()}
            out[qid] = {"type": "choice", "choice": str(choice) if choice is not None else max(pr, key=pr.get),
                        "probabilities": pr, **({"confidence": a["confidence"]} if "confidence" in a else {})}
        elif t == "score":
            levels = list(q["criteria"])
            n = len(levels)
            sc = a.get("score")
            if isinstance(sc, bool) or not isinstance(sc, (int, float)):
                raise JvError(f"score 题 {qid} 的 score 必须是档位**下标**（0..{n - 1} 的数），不是标签；收到 {sc!r}（档位 {levels}）。"
                              f"修法：{{'type': 'score', 'score': {levels.index(sc) if sc in levels else 0}.0, 'probabilities': {{'0': …}}}}")
            probs = a.get("probabilities")
            if not isinstance(probs, dict) or not probs:
                raise JvError(f"score 题 {qid} 的返回体缺 probabilities（按档位下标 '0'..'{n - 1}' 的字典）；收到 {a!r}。"
                              f"修法：{{'type': 'score', 'score': 2.0, 'probabilities': {{'0': 0.05, '1': 0.05, '2': 0.9}}}}")
            pr: dict[str, float] = {}
            for k, v in probs.items():
                ks = str(k)
                if not ks.lstrip("-").isdigit() or not 0 <= int(ks) < n:
                    hint = f"（{ks!r} 看起来是标签；档位 {levels} 的下标是 0..{n - 1}）" if ks in [str(l) for l in levels] else ""
                    raise JvError(f"score 题 {qid} 的 probabilities 键 {ks!r} 不是档位下标{hint}。"
                                  f"修法：probabilities 的键用 '0'..'{n - 1}'（或 int），值是该档概率")
                pr[str(int(ks))] = float(v)
            out[qid] = {"type": "score", "score": float(sc), "probabilities": pr,
                        **({"confidence": a["confidence"]} if "confidence" in a else {})}
        else:
            raise JvError(f"未知题型 {t}（qid={qid}）")
    return out


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
