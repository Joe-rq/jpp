"""#81 文档与函数签名不一致（noul）。真值：人工改签名 / 改 docstring 的参数表。签名由 AST 抽取（transform）。"""

from __future__ import annotations

import ast
import random

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_noul, q_noul, register

_FUNCS = [
    ("def fetch(url, timeout, retries):", ["url", "timeout", "retries"], "抓取 url。"),
    ("def render(template, context, escape):", ["template", "context", "escape"], "渲染模板。"),
    ("def split_batch(items, size, drop_last):", ["items", "size", "drop_last"], "按 size 切批。"),
    ("def score(pred, truth, weights):", ["pred", "truth", "weights"], "计算加权得分。"),
    ("def connect(host, port, ssl):", ["host", "port", "ssl"], "建立连接。"),
    ("def save(path, data, overwrite):", ["path", "data", "overwrite"], "保存数据。"),
]
_DESC = {"url": "地址", "timeout": "超时秒数", "retries": "重试次数", "template": "模板字符串", "context": "变量字典",
         "escape": "是否转义", "items": "元素列表", "size": "每批大小", "drop_last": "是否丢弃不满一批", "pred": "预测值",
         "truth": "真值", "weights": "权重", "host": "主机", "port": "端口", "ssl": "是否启用 TLS", "path": "文件路径",
         "data": "要写的内容", "overwrite": "已存在时是否覆盖", "verbose": "是否打印日志", "limit": "上限"}


def _src(sig: str, doc_params: list[str], summary: str) -> str:
    lines = [sig, f'    """{summary}', "", "    Args:"]
    lines += [f"        {p}: {_DESC.get(p, '参数')}" for p in doc_params]
    lines += ['    """', "    ..."]
    return "\n".join(lines)


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        sig, params, summary = _FUNCS[i % len(_FUNCS)]
        sig = sig.replace("(", f"{i // len(_FUNCS)}(", 1) if i >= len(_FUNCS) else sig      # 24 条互不相同（避免同状态去重）
        pos = i % 2 == 0
        doc = list(params)
        if not pos:
            mode = (i // 2) % 3
            if mode == 0:                                   # 签名改名
                j = rnd.randrange(len(params)); newp = params[j] + "_s"
                sig = sig.replace(params[j], newp)
            elif mode == 1:                                 # 签名多一个参数，文档没写
                sig = sig.replace("):", ", verbose):")
            else:                                           # 文档多写一个不存在的参数
                doc = doc + ["limit"]
        out.append(Sample(id=f"p81-{i:02d}", truth=pos, mats={"src": _src(sig, doc, summary)}))
    return out


def _signature(src):
    tree = ast.parse(src.content)
    fn = next(n for n in tree.body if isinstance(n, ast.FunctionDef))
    return "签名参数: " + ", ".join(a.arg for a in fn.args.args)


一致 = jv.test("docstring 里 Args 描述的参数与函数签名的参数一致吗？", calib=jv.calib("probe81.一致"))


@jv.program(budget=jv.Budget(calls=40, cost=0.05, layers=1))
def 核对(srcs, keys):
    states = [jv.state(on=s, ctx=[jv.transform(_signature, s)]) for s in srcs]
    exits = jv.cut(jv.judge(states, 一致))
    return [exit_to_result(sid, truth, e, "noul") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    return 核对([jv.lit(s.mats["src"]) for s in samples], [(s.id, s.truth) for s in samples])


def bare(samples, client, tally):
    out = []
    for s in samples:
        sig = _signature(jv.lit(s.mats["src"]))
        a = bare_ask(client, bare_state(on=s.mats["src"], ctx=[sig]), {"q0": q_noul(一致.text)}, tally)
        pred, p = parse_noul(a["q0"])
        out.append(Result(s.id, s.truth, pred, p, "bare"))
    return out


def baseline(samples):
    """启发式：Args 段的名字集合 == 签名名字集合。"""
    out = []
    for s in samples:
        src = s.mats["src"]
        sig = set(_signature(jv.lit(src)).split(": ")[1].split(", "))
        doc = {l.strip().split(":")[0] for l in src.splitlines() if l.startswith("        ") and ":" in l}
        same = sig == doc
        out.append(Result(s.id, s.truth, same, 1.0 if same else 0.0, "nameset"))
    return out


register(Probe("p81", "文档与函数签名不一致", "noul", make_samples, builder, bare, baseline))
