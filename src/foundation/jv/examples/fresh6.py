"""G6 零上下文读者（契约表之后）写的三条程序：邮件分流 / 代码评审排队 / 地址标准化。

只读了 README（含 §9 契约表）、six.py、seven.py、strength.py 与 __init__.py 的签名。
假客户端与假 S 函数照 six.py 的写法。跑：`python -m foundation.jv.examples.fresh6`。
猜点编号与 设计/G6-零上下文读者-契约表后.md 对应。
"""

from __future__ import annotations

import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))))

import foundation.jv as jv  # noqa: E402

# =============================================================== 1. 邮件分流
FOLDERS: dict[str, list] = {}                                   # 宿主的文件夹映射：文件夹名 → 邮件 id 列表


def _file(email, folder):
    FOLDERS.setdefault(folder.content, []).append(email.content["id"])
    return {"filed": email.content["id"], "to": folder.content}


# 猜 4：不可逆动作照 seven.py 的写法 reversible=False + reason=""；输出 taint 用 inherit
分流 = jv.register_action("file_email", fn=_file, taint_out="inherit", reversible=False, reason="", cost=0.0)


@jv.program(budget=jv.Budget(calls=50, cost=0.02, layers=2))
def 邮件分流(邮件, 文件夹):
    正常 = jv.test("这封邮件是正常的业务邮件（不是钓鱼或诈骗）吗？", calib=jv.calib("mail.正常"))   # 猜 2：守卫要 Act，题面反着问
    去哪 = jv.select("这封邮件应归入哪个文件夹？", calib=jv.calib("mail.去哪"))
    rs = jv.judge([jv.state(on=m, over=文件夹) for m in 邮件], 正常, 去哪)     # 猜 1：test 题的状态也带 over
    安, 夹 = jv.cut([r[0] for r in rs]), jv.cut([r[1] for r in rs])            # 猜 5：第二个 cut 不再多一层
    写, 交人 = [], []
    for i, (m, a, f) in enumerate(zip(邮件, 安, 夹)):
        match (a, f):
            case (jv.Act(), jv.Pick(k)): 写.append(jv.do(分流, m, 文件夹[k], iter_seq=i, guard=a))   # 猜 3：邮件是 lit 所以 trusted
            case _: 交人.append(m.content["id"])                                  # 可疑 / 拿不准 / 归类拿不准：都不分流
    jv.consume(安, unsure=jv.drop); jv.consume(夹, unsure=jv.drop)                # 猜 6：case _ 不消费 Unsure，用 consume 补
    return {"已分流": 写, "交人": jv.escalate(交人, note="可疑或拿不准，攒起来交人")}   # 猜 7：期物列表直接 return


# =============================================================== 2. 代码评审排队
def split_hunks(diff):
    return [h.strip() for h in diff.content.split("\n@@") if h.strip()]


def 合格(hunk_text: str, 意见: str) -> bool:
    return any(n in 意见 for n in re.findall(r"def (\w+)", hunk_text))


@jv.program(budget=jv.Budget(calls=50, cost=0.02, layers=2))
def 评审排队(diff):
    要看 = jv.test("这个 hunk 需要人工评审吗？", calib=jv.calib("review.要看"))
    风险 = jv.measure("这个 hunk 的风险等级是？", scale=("低", "中", "高"), calib=jv.calib("review.风险"))
    hunks = jv.transform(split_hunks, diff)
    states = [jv.state(on=h) for h in hunks]
    看, 险 = jv.judge(states, 要看), jv.judge(states, 风险)      # 猜 8：两次 judge 同状态同段也融合；猜 9：分开登记才能对 险 单独 .order()
    看出 = jv.cut(看)
    拿不准 = [i for i, x in enumerate(jv.consume(看出, unsure=jv.drop)) if x is None]
    顺序 = [i for tier in 险.order() for i in tier if isinstance(看出[i], jv.Act)]   # 猜 10：档高→低，组内并列按下标；猜 11：险 不 cut 只 order
    意见 = {}
    for j, i in enumerate(顺序[:2]):
        for 试 in range(2):                                        # 不合格重新生成一次
            句 = jv.gen("为这个 hunk 写一句评审意见，点名其中的函数", ctx=[hunks[i]], n=1, retry_seq=试)   # 猜 12：retry_seq 用内层循环变量
            if 句 and 合格(hunks[i].content, 句[0].content):       # 猜 13：宿主检查直接读 .content，不经 transform
                意见[i] = 句[0].content; break
    return {"顺序": 顺序, "拿不准": 拿不准, "意见": 意见}


# =============================================================== 3. 地址标准化
def 校验(addr: str) -> bool:
    return addr.endswith("号") and "区" in addr and (addr.startswith("北京市") or addr.startswith("上海市"))


def 过校验(cands):                                                  # 猜 14：transform 收 list[Mat]、返 list[str]
    return [c.content for c in cands if 校验(c.content)]


@jv.program(budget=jv.Budget(calls=50, cost=0.02, layers=2))
def 地址标准化(地址们):
    忠实 = jv.select("哪个标准化结果最忠实于原文地址？", calib=jv.calib("addr.忠实"))
    待判, 结果, 失败 = [], {}, []
    for i, a in enumerate(地址们):
        通过 = jv.transform(过校验, jv.gen("把这条地址改写成「省市+区+路+号」的标准格式", ctx=[a], n=3, retry_seq=i))
        if not 通过: 失败.append(i); continue                       # 猜 15：transform 返回的列表可直接判空
        待判.append((i, a, 通过))
    exits = jv.cut(jv.judge([jv.state(on=a, over=c) for _, a, c in 待判], 忠实))   # 一层；猜 16：over 只剩 1 个也合法
    for (i, a, c), e in zip(待判, exits):
        match e:
            case jv.Pick(k): 结果[i] = c[k].content
            case jv.Unsure(): 失败.append(i)                        # 猜 17：Unsure 直接算失败，不 handle
    return {"结果": 结果, "失败": 失败, "成功率": round(len(结果) / len(地址们), 3)}


# =============================================================== 假客户端、假生成器、校准
def fake_rule(text, qid, q):
    st = json.loads(text) if text.startswith("{") else {}
    on = st.get("on", "")
    ins = q["instructions"]
    if q["type"] == "noul":
        if "钓鱼" in ins:
            body = json.dumps(on, ensure_ascii=False)
            return {"type": "noul", "noul": 0.05 if ("验证您的账户" in body or "点击链接" in body) else (0.5 if "拿不准" in body else 0.95)}
        if "hunk" in ins:
            return {"type": "noul", "noul": 0.9 if any(w in str(on) for w in ("delete", "drop", "auth")) else (0.5 if "config" in str(on) else 0.1)}
        return {"type": "noul", "noul": 0.5}
    if q["type"] == "score":
        s = str(on)
        idx = 2 if ("delete" in s or "drop" in s) else (1 if "auth" in s else 0)
        p = 0.9 if "delete" in s else 0.8
        n = len(q["criteria"])
        return {"type": "score", "score": float(idx), "probabilities": {str(i): (p if i == idx else (1 - p) / (n - 1)) for i in range(n)}}
    if q["type"] == "choice":
        opts = list(q["criteria"]); over = st.get("over", {})
        key = json.dumps(on, ensure_ascii=False)
        best = None
        for o in sorted(opts):
            v = str(over.get(o, ""))
            if v and v in key: best = o; break                                   # 文件夹名出现在邮件里
        if best is None and isinstance(on, str):                                  # 地址：与原文完全一致的最忠实
            for o in sorted(opts):
                if str(over.get(o, "")) == on: best = o; break
        if best is None:
            return {"type": "choice", "choice": opts[0], "probabilities": {k: 1 / len(opts) for k in opts}}
        return {"type": "choice", "choice": best, "probabilities": {k: (0.9 if k == best else 0.1 / max(1, len(opts) - 1)) for k in opts}}
    return None


def fake_generator(prompt, ctx, n, retry_seq):
    src = ctx[0].content if ctx else ""
    if "评审意见" in prompt:
        names = re.findall(r"def (\w+)", src)
        if "drop_table" in src and retry_seq == 0:
            return ["这段改动去掉了保护逻辑，请谨慎。"]                          # 第一次故意不提函数名
        return [f"{names[0]} 的改动影响面较大，建议补测试。" if names else "看不出改了哪个函数。"]
    if "地址" in prompt:
        if "老地方" in src:
            return ["老地方", "老地方见", "老地方 见"][:n]                      # 全部不过校验
        return [src, src.replace("市", " ").replace("区", " "), src + "（标准化）"][:n]
    return []


def calib_all(rt):
    for k in ("mail.正常", "mail.去哪", "review.要看", "review.风险", "addr.忠实"):
        rt.calib.put(k, hi=0.6, lo=0.3, n=50, status="上岗", set_id="fake")


def _row(rt, name, out):
    layers = rt.stats["layers"]; st = rt.stats_report()
    return {"程序": name, "结果": out, "层数": len(layers), "每层题数": [l["questions"] for l in layers],
            "每层调用": [l["calls"] for l in layers], "题": rt.stats["questions"], "调用": rt.stats["calls"],
            "融合率": st["fusion_rate"], "警告": rt.stats["warnings"]}


def run_all() -> list[dict]:
    rows = []
    cases = [
        ("邮件分流", lambda: 邮件分流(
            [jv.lit({"id": "m1", "from": "cfo@corp.com", "subject": "三季度财务报表", "body": "请查收附件的财务数据。\n谢谢。"}),
             jv.lit({"id": "m2", "from": "it@corp.com", "subject": "服务器技术维护通知", "body": "今晚 22 点技术停机。\n请知悉。"}),
             jv.lit({"id": "m3", "from": "x@evil.io", "subject": "紧急：验证您的账户", "body": "点击链接立即验证。\n否则冻结。"}),
             jv.lit({"id": "m4", "from": "hr@corp.com", "subject": "拿不准的一封", "body": "内容含糊。\n无关键字。"})],
            [jv.lit("财务"), jv.lit("技术"), jv.lit("人事")])),
        ("评审排队", lambda: 评审排队(jv.lit(
            "@@ -1,2 +1,2 @@\n def parse(x):\n-    return x\n+    return x.strip()\n"
            "@@ -10,3 +10,2 @@\n def delete_user(uid):\n-    check(uid)\n     db.remove(uid)\n"
            "@@ -20,2 +20,3 @@\n def check_auth(token):\n+    return True\n"
            "@@ -30,1 +30,1 @@\n def load_config(p):\n-    pass\n+    return p\n"
            "@@ -40,2 +40,1 @@\n def drop_table(name):\n-    confirm()\n     db.drop(name)\n"))),
        ("地址标准化", lambda: 地址标准化(
            [jv.lit("北京市海淀区中关村大街1号"), jv.lit("上海市浦东新区世纪大道100号"), jv.lit("老地方见"), jv.lit("北京市朝阳区建国路8号")])),
    ]
    for name, fn in cases:
        with jv.Runtime(client=jv.FakeClient(rule=fake_rule), generator=fake_generator) as rt:
            calib_all(rt)
            try:
                out = fn()
            except Exception as e:                                   # 跑不通就原样记录
                out = f"{type(e).__name__}: {e}"
            rows.append(_row(rt, name, out))
    return rows


if __name__ == "__main__":
    for r in run_all():
        print(json.dumps(r, ensure_ascii=False, default=repr))
    print("FOLDERS =", FOLDERS)
