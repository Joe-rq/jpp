"""G4 零上下文读者的 24 条猜点与 5 处报错 → 库与 README 的修补（ir-impl-3）。全部 FakeClient，$0。"""

from __future__ import annotations

import warnings

import pytest

import foundation.jv as jv


def _rt(rule=None, **kw):
    rt = jv.Runtime(client=jv.FakeClient(rule=rule), **kw)
    rt.calib.put("k.test", hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")
    rt.calib.put("k.sel", hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")
    rt.calib.put("k.m", hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")
    return rt


# ---------------------------------------------------------------- G4 报错 1 / 3 / 5：返回体校验（不再静默）
def test_g4_err1_missing_probabilities_is_jverror_with_fix():
    def rule(text, qid, q):
        if q["type"] == "score":
            return {"type": "score", "score": 1.0}                       # 漏 probabilities（G4 报错 1：KeyError）
    with _rt(rule) as rt:
        @jv.program()
        def f(s):
            m = jv.measure("严重程度？", scale=("低", "中", "高"), calib=jv.calib("k.m"))
            return jv.cut(jv.judge(jv.state(on=s), m)[0])
        with pytest.raises(jv.JvError) as ei:
            f(jv.lit("WARN x"))
        assert "probabilities" in str(ei.value) and "修法" in str(ei.value)


def test_g4_err3_score_label_instead_of_index():
    def rule(text, qid, q):
        if q["type"] == "score":
            return {"type": "score", "score": "低", "probabilities": {"0": 0.9}}   # G4 报错 3：ValueError '低'
    with _rt(rule):
        @jv.program()
        def f(s):
            m = jv.measure("严重程度？", scale=("低", "中", "高"), calib=jv.calib("k.m"))
            return jv.cut(jv.judge(jv.state(on=s), m)[0])
        with pytest.raises(jv.JvError) as ei:
            f(jv.lit("x"))
        assert "下标" in str(ei.value) and "标签" in str(ei.value)


def test_g4_err5_probabilities_keyed_by_label_not_silent_unsure():
    def rule(text, qid, q):
        if q["type"] == "score":
            return {"type": "score", "score": 2.0, "probabilities": {"低": 0.05, "中": 0.05, "高": 0.9}}  # G4 报错 5
    with _rt(rule):
        @jv.program()
        def f(s):
            m = jv.measure("严重程度？", scale=("低", "中", "高"), calib=jv.calib("k.m"))
            return jv.cut(jv.judge(jv.state(on=s), m)[0])
        with pytest.raises(jv.JvError) as ei:
            f(jv.lit("FATAL"))
        assert "档位下标" in str(ei.value) and "标签" in str(ei.value)


def test_validate_answers_normalizes_int_keys_and_choice_labels():
    qs = {"a": {"type": "score", "instructions": "?", "criteria": ["低", "中"]},
          "b": {"type": "choice", "instructions": "?", "criteria": {"c0": "x", "c1": "y"}},
          "c": {"type": "noul", "instructions": "?"}}
    out = jv.validate_answers(qs, {"a": {"type": "score", "score": 1, "probabilities": {0: 0.2, 1: 0.8}},
                                   "b": {"type": "choice", "probabilities": {"c0": 0.3, "c1": 0.7}},
                                   "c": {"type": "noul", "noul": 0.5}})
    assert out["a"]["probabilities"] == {"0": 0.2, "1": 0.8} and out["a"]["score"] == 1.0
    assert out["b"]["choice"] == "c1"
    with pytest.raises(jv.JvError):
        jv.validate_answers(qs, {"a": {"type": "score", "score": 1, "probabilities": {"0": 1}},
                                 "b": {"type": "choice", "probabilities": {"x": 1.0}}, "c": {"type": "noul", "noul": 0.5}})
    with pytest.raises(jv.JvError):
        jv.validate_answers({"c": qs["c"]}, {"c": {"type": "noul", "noul": 1.5}})


# ---------------------------------------------------------------- G4 报错 2：J-06 报文带修法
def test_g4_err2_loop_without_variant_message_has_fix():
    with _rt():
        @jv.program()
        def f(s):
            for it in jv.loop(bound=3):
                pass
            return 0
        with pytest.raises(jv.JvError) as ei:
            f(jv.lit("x"))
        assert "J-06" in str(ei.value) and "variant=jv.decreasing" in str(ei.value)


# ---------------------------------------------------------------- G4 报错 4：`"x" in Mat`
def test_g4_err4_in_on_mat_is_typed_error_with_fix():
    m = jv.lit("提交 按钮")
    with pytest.raises(jv.JvTypeError) as ei:
        "提交" in m
    assert ".content" in str(ei.value)
    with pytest.raises(jv.JvTypeError):
        list(m)
    with pytest.raises(jv.JvTypeError):
        m == "提交 按钮"
    assert "提交" in m.content


# ---------------------------------------------------------------- 猜 4：Mat 相等性与哈希（按内容）
def test_mat_equality_by_content_hash_and_set_membership():
    a, b = jv.lit("张三"), jv.lit("张三")
    c = jv.lit("李四")
    assert a == b and a != c and len({a, b, c}) == 2 and a in [b]
    with _rt():
        @jv.program()
        def f(x):
            xs = jv.transform(lambda m: [m.content, "李四"], x)
            return [m for m in xs if m not in {jv.lit("李四")}]      # transform 输出与字面量按内容相等
        out = f(jv.lit("张三"))
        assert [m.content for m in out] == ["张三"]


# ---------------------------------------------------------------- 猜 21：期物直接 return → Mat
def test_future_return_is_materialized_to_mat():
    act = jv.register_action("echo", fn=lambda m: {"echo": m.content}, taint_out="inherit")
    with _rt():
        @jv.program()
        def f(x):
            return jv.do(act, x, iter_seq=0)                      # 不写 .content
        out = f(jv.lit("hi"))
        assert isinstance(out, jv.Mat) and out.content == {"echo": "hi"}

        @jv.program()
        def g(x):
            return {"a": [jv.do(act, x, iter_seq=0)], "b": 1}
        out = g(jv.lit("hi"))
        assert isinstance(out["a"][0], jv.Mat) and out["b"] == 1


# ---------------------------------------------------------------- 猜 16 / W-self-trusted / register_action
def test_register_action_trusted_requires_reason():
    with pytest.raises(jv.JvError):
        jv.register_action("r1", fn=lambda: 1, taint_out="trusted")
    a = jv.register_action("r2", fn=lambda: 1, taint_out="trusted", reason="确定性函数")
    assert a.registered and jv.ACTIONS["r2"] is a


读页_自封 = jv.Action("read_page_self", fn=lambda p: "表单 姓名:张三 按钮:提交", taint_out="trusted")
提交_不可逆 = jv.Action("submit_x", fn=lambda p: {"ok": 1}, taint_out="inherit", reversible=False)


def test_self_trusted_action_warns_static_and_runtime():
    with _rt() as rt:
        @jv.program()
        def f(page):
            全填 = jv.test("所有字段都已填好了吗？", calib=jv.calib("k.test"))
            观 = jv.do(读页_自封, page, iter_seq=0)
            好 = jv.cut(jv.judge(jv.state(on=观), 全填)[0])
            if isinstance(好, jv.Act):
                return jv.do(提交_不可逆, page, iter_seq=0, guard=好)
            jv.consume([好], unsure=jv.drop)
            return None
        rep = jv.check(f.__jv_fn__)
        assert any("W-self-trusted" in w and "read_page_self" in w for w in rep.warnings)
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            f(jv.lit("page"))
        assert sum("W-self-trusted" in w for w in rt.stats["warnings"]) >= 1


# ---------------------------------------------------------------- Action.cost 进预算
def test_action_cost_counts_toward_frame_budget():
    pricey = jv.register_action("pricey", fn=lambda m: {"r": 1}, taint_out="inherit", cost=0.01)
    with _rt() as rt:
        @jv.program(budget=jv.Budget(cost=0.015))
        def f(x):
            t = jv.test("好吗？", calib=jv.calib("k.test"))
            a = jv.do(pricey, x, iter_seq=0); b = jv.do(pricey, x, iter_seq=1)     # 0.02 > 0.015
            es = jv.cut(jv.judge([jv.state(on=a), jv.state(on=b)], t))
            jv.consume(es, unsure=jv.drop)
            return es
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            es = f(jv.lit("x"))
        assert rt.stats["do_cost"] == pytest.approx(0.02)
        assert all(isinstance(e, jv.Unsure) and e.cause == "budget" for e in es)   # 层边界核：do 的钱已超，判断层停
        assert any("W-budget" in w for w in rt.stats["warnings"])


# ---------------------------------------------------------------- 猜 3 / 5 / 6：ask → Pending → jv.answer → 重跑（无 root 也行）
def test_ask_answer_replay_without_root():
    def rule(text, qid, q):
        return {"type": "noul", "noul": 0.5}
    with _rt(rule) as rt:
        @jv.program(budget=jv.Budget(escalate=2))
        def f(a, b):
            同人 = jv.test("同一个人吗？", calib=jv.calib("k.test"))
            e = jv.cut(jv.judge(jv.state(on=a, ctx=[b]), 同人)[0])
            match e:
                case jv.Unsure(c):
                    答 = jv.ask(jv.state(on=a, ctx=[b]), 同人)
                    return "同" if isinstance(答, jv.Act) else "异"
                case _:
                    return "?"
        with pytest.raises(jv.Pending) as ei:
            f(jv.lit("张三"), jv.lit("张三 华为"))
        with pytest.raises(jv.JvError):
            jv.answer(ei.value.key, "yes")                            # kind 只能 act|ignore|pick|at
        jv.answer(ei.value.key, "act")
        assert f(jv.lit("张三"), jv.lit("张三 华为")) == "同"
        assert rt.stats["ledger_hits"] >= 1                           # 判断也重放了


# ---------------------------------------------------------------- 猜 22 / 23：守卫失败后的消费幂等；Ignore 不必显式消费
def test_match_guard_fallthrough_and_ignore_need_no_consume():
    def rule(text, qid, q):
        if q["type"] == "choice":
            return {"type": "choice", "choice": "c1", "probabilities": {"c0": 0.1, "c1": 0.9}}
        return {"type": "noul", "noul": 0.05}
    with _rt(rule):
        @jv.program()
        def f(x, cands):
            sel = jv.select("哪个？", calib=jv.calib("k.sel"))
            t = jv.test("好吗？", calib=jv.calib("k.test"))
            e = jv.cut(jv.judge(jv.state(on=x, over=cands), sel))
            g = jv.cut(jv.judge(jv.state(on=x), t)[0])          # Ignore：不消费也不报错
            match e:
                case jv.Pick(k) if k == 0: return "零"
                case jv.Pick(k): return f"k={k}"
                case jv.Unsure(c): jv.handle(c, keep=None)
            return None
        assert f(jv.lit("x"), [jv.lit("a"), jv.lit("b")]) == "k=1"


# ---------------------------------------------------------------- 猜 20：escalate 返回值不抛
def test_escalate_returns_value_and_counts():
    with _rt() as rt:
        @jv.program(budget=jv.Budget(escalate=1))
        def f(x):
            return jv.escalate(x, note="人看")
        out = f(jv.lit("x"))
        assert isinstance(out, jv.Escalated) and out.note == "人看" and rt.stats["escalated"]


# ---------------------------------------------------------------- 第七条示例（measure）跑通且两题同层融合
def test_seven_measure_example_runs_and_fuses():
    from foundation.jv.examples import seven
    r = seven.run_seven()
    out = r["out"]
    assert [m.content for m in out["告警"]] == [{"sent": "FATAL OOM killed worker"}]
    assert out["汇总"] == ["WARN 连接池 retry 3 次"]
    assert r["层数"] == 1 and r["调用"] == 4 and r["题"] == 8          # 4 段 × 2 题，一层 4 调用
    assert r["钱"] >= 0.001                                               # 告警的 Action.cost 计入
