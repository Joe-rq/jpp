"""Synthetic observations for mechanism tests. Never a real calibration dataset."""
from __future__ import annotations
import ast
import json
from foundation import jv

KEYS = ("demo.membership", "demo.pick", "demo.complexity", "demo.flag")


def fixed_rule(text, qid, q):
    state = json.loads(text)
    obj = state.get("on", {})
    instructions = q.get("instructions", "")
    if q["type"] == "noul":
        if isinstance(obj, dict) and obj.get("unknown"):
            value = 0.5
        elif "[membership]" in instructions:
            value = 0.99 if obj["target"] in obj["subset"] else 0.01
        elif isinstance(obj, dict) and "flag" in obj:
            value = 0.99 if obj["flag"] else 0.01
        else:
            return None
        return {"type": "noul", "noul": value}
    if q["type"] == "choice":
        candidates = state.get("over", {})
        def score(key):
            value = candidates.get(key, {})
            if isinstance(value, dict) and "expression" in value:
                return (len(list(ast.walk(ast.parse(value["expression"], mode="eval")))), value["expression"])
            return (value.get("priority", 0) if isinstance(value, dict) else 0, str(value))
        keys = list(q["criteria"])
        pick = min(keys, key=score)
        return {"type": "choice", "choice": pick, "confidence": 0.99,
                "probabilities": {key: 0.99 if key == pick else 0.01 / max(1, len(keys) - 1) for key in keys}}
    if q["type"] == "score":
        expression = obj.get("expression", "x") if isinstance(obj, dict) else "x"
        nodes = len(list(ast.walk(ast.parse(expression, mode="eval"))))
        level = min(len(q["criteria"]) - 1, max(0, (nodes - 3) // 4))
        return {"type": "score", "score": float(level), "confidence": 0.99,
                "probabilities": {str(i): 0.99 if i == level else 0.01 / max(1, len(q["criteria"]) - 1)
                                  for i in range(len(q["criteria"]))}}
    return None


class FixtureClient(jv.FakeClient):
    def __init__(self, rule=fixed_rule):
        super().__init__(rule=rule, model_id="synthetic-composition-fixture-v1")

    def ask(self, state, questions):
        answers, tokens, _ = super().ask(state, questions)
        return answers, tokens, 0.0


def runtime(*, root=None, rule=fixed_rule, generator=None, passes=None):
    rt = jv.Runtime(FixtureClient(rule), profile={"model_version": "synthetic-composition-fixture-v1"},
                    root=str(root) if root else None, generator=generator, passes=passes)
    for key in KEYS:
        rt.calib.put(key, hi=0.65, lo=0.35, n=1, status="上岗", delta=0.0,
                     set_id="synthetic-mechanism-fixture", source="Synthetic fixture; not real model calibration")
    return rt
