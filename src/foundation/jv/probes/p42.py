"""#42 日志异常行分类（choice）。公开标注集离线不可得，用模板合成四类（网络 / 磁盘 / 认证 / 正常），真值按模板。"""

from __future__ import annotations

import random

import foundation.jv as jv

from ._common import Probe, Result, Sample, bare_ask, bare_state, exit_to_result, parse_choice, q_choice, register

_CATS = ["网络故障", "磁盘或存储故障", "认证或权限失败", "正常运行"]
_LINES = {
    0: ["2026-09-20 10:{m}:01 ERROR upstream connect timeout after 30s host=api-{h}",
        "2026-09-20 10:{m}:02 WARN dns lookup failed for cache-{h}.internal: NXDOMAIN",
        "2026-09-20 10:{m}:03 ERROR connection reset by peer while reading response from {h}",
        "2026-09-20 10:{m}:04 ERROR socket: no route to host 10.0.{m}.{h}",
        "2026-09-20 10:{m}:05 WARN retrying request to {h}, attempt 3/3 (EHOSTUNREACH)",
        "2026-09-20 10:{m}:06 ERROR tls handshake with {h} timed out"],
    1: ["2026-09-20 10:{m}:11 ERROR write /var/data/{h}.log: no space left on device",
        "2026-09-20 10:{m}:12 ERROR I/O error on sda{m}: read-only file system",
        "2026-09-20 10:{m}:13 WARN inode usage 98% on /data (host {h})",
        "2026-09-20 10:{m}:14 ERROR fsync failed for journal-{h}: Input/output error",
        "2026-09-20 10:{m}:15 ERROR cannot open /mnt/store/{h}: stale NFS file handle",
        "2026-09-20 10:{m}:16 WARN disk latency p99 2400ms on volume {h}"],
    2: ["2026-09-20 10:{m}:21 ERROR 401 unauthorized: token expired for user u{h}",
        "2026-09-20 10:{m}:22 WARN permission denied: u{h} tried DELETE /admin/{m}",
        "2026-09-20 10:{m}:23 ERROR invalid signature on request from client {h}",
        "2026-09-20 10:{m}:24 ERROR ldap bind failed for svc-{h}: invalid credentials",
        "2026-09-20 10:{m}:25 WARN api key {h}**** revoked, request rejected",
        "2026-09-20 10:{m}:26 ERROR 403 forbidden: role viewer cannot access /billing/{m}"],
    3: ["2026-09-20 10:{m}:31 INFO request GET /health 200 in 3ms (host {h})",
        "2026-09-20 10:{m}:32 INFO scheduled job nightly-{h} finished, 0 errors",
        "2026-09-20 10:{m}:33 INFO cache warmed: {m} keys from {h}",
        "2026-09-20 10:{m}:34 INFO worker {h} started, pool size {m}",
        "2026-09-20 10:{m}:35 INFO rotated log file app-{h}.log",
        "2026-09-20 10:{m}:36 INFO deploy {h} completed, version 1.{m}.0"],
}


def make_samples(n: int = 24, seed: int = 0) -> list[Sample]:
    rnd = random.Random(seed)
    out = []
    for i in range(n):
        c = i % 4
        tpl = _LINES[c][(i // 4) % 6]
        line = tpl.format(m=rnd.randint(10, 59), h=rnd.choice(["a7", "b3", "c9", "d1"]))
        out.append(Sample(id=f"p42-{i:02d}", truth=c, mats={"line": line}))
    return out


类别 = jv.select("这行日志属于哪一类？", calib=jv.calib("probe42.类别"))


@jv.program(budget=jv.Budget(calls=60, cost=0.05, layers=1))
def 分类(lines, cats, keys):
    exits = jv.cut(jv.judge([jv.state(on=l, over=cats) for l in lines], 类别))
    return [exit_to_result(sid, truth, e, "choice") for (sid, truth), e in zip(keys, exits)]


def builder(samples, rt):
    cats = [jv.lit(c) for c in _CATS]
    return 分类([jv.lit(s.mats["line"]) for s in samples], cats, [(s.id, s.truth) for s in samples])


def bare(samples, client, tally):
    out = []
    for s in samples:
        a = bare_ask(client, bare_state(on=s.mats["line"], over=_CATS), {"q0": q_choice(类别.text, _CATS)}, tally)
        k, p = parse_choice(a["q0"])
        out.append(Result(s.id, s.truth, k, p, "bare"))
    return out


_KW = {0: ["timeout", "dns", "connect", "route", "socket", "tls", "EHOST"], 1: ["space", "I/O", "inode", "fsync", "NFS", "disk", "file system"],
       2: ["401", "403", "unauthorized", "permission", "signature", "credentials", "revoked", "forbidden"]}


def baseline(samples):
    out = []
    for s in samples:
        line = s.mats["line"]
        k = 3
        for c, kws in _KW.items():
            if any(w.lower() in line.lower() for w in kws):
                k = c
                break
        out.append(Result(s.id, s.truth, k, None, "keywords"))
    return out


register(Probe("p42", "日志异常行分类", "choice", make_samples, builder, bare, baseline))
