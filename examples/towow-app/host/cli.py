"""towow 命令行：接入、查看机会、回复补信息请求、起服务。

    towow serve [--port 8794]                     起引擎 + 网络程序 + /mcp + /events + /api/state
    towow spec                                     打印算子包编译说明
    towow join --pack pack.json --name 名字 [--host-agent CLI]
    towow opps <agent_id>                          机会列表（置信度 = 决定性那道题的读数，附题面）
    towow inbox <agent_id>                         别人向你要的补信息请求
    towow respond <agent_id> <request_id> --grant "给对方的信息" | --deny
    towow leave <agent_id>
    towow connect                                  打印各家 agent 的一行接入命令
服务地址：--server 或环境变量 TOWOW_URL，默认 http://localhost:8794。
"""
from __future__ import annotations

import argparse
import asyncio
import json
import os
import sys

DEFAULT_URL = os.environ.get("TOWOW_URL", "http://localhost:8794")
APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def connect_lines(url: str) -> str:
    mcp = url.rstrip("/") + "/mcp"
    cfg = json.dumps({"mcpServers": {"towow": {"url": mcp}}})
    return "\n".join([
        f"Claude Code : claude mcp add --transport http towow {mcp}",
        f"Codex CLI   : codex mcp add towow --url {mcp}",
        f"Gemini CLI  : gemini mcp add --transport http towow {mcp}",
        f"Cursor      : 在 ~/.cursor/mcp.json 写 {cfg}",
        f"VS Code     : .vscode/mcp.json 写 {json.dumps({'servers': {'towow': {'type': 'http', 'url': mcp}}})}",
        "云端 agent（ChatGPT、Muse 等）访问不到 localhost，需要公网 HTTPS 地址（隧道或部署）后填同一个 /mcp 地址。",
    ])


TOK_PATH = os.path.expanduser("~/.towow/tokens.json")


def save_token(url: str, aid: str, tok: str):
    os.makedirs(os.path.dirname(TOK_PATH), exist_ok=True)
    try:
        d = json.load(open(TOK_PATH))
    except (OSError, json.JSONDecodeError):
        d = {}
    d[f"{url.rstrip('/')}|{aid}"] = tok
    with open(TOK_PATH, "w") as f:
        json.dump(d, f)
    os.chmod(TOK_PATH, 0o600)


def load_token(url: str, aid: str) -> str:
    try:
        return json.load(open(TOK_PATH)).get(f"{url.rstrip('/')}|{aid}", "")
    except (OSError, json.JSONDecodeError):
        return ""


async def call(url: str, tool: str, args: dict) -> dict:
    from mcp import Client

    async with Client(url.rstrip("/") + "/mcp") as c:
        res = await c.call_tool(tool, args)
        sc = getattr(res, "structured_content", None)
        if sc:
            return sc.get("result", sc) if set(sc) == {"result"} else sc
        txt = res.content[0].text if res.content else "{}"
        try:
            return json.loads(txt)
        except json.JSONDecodeError:
            return {"text": txt, "is_error": res.is_error}


def opp_line(o: dict) -> str:
    """一行：形式、成员、置信度与题面、还缺什么（、方案标题）。"""
    who = "、".join((w.get("display") or w.get("id") or "?")[:24] for w in o.get("with") or [])
    c = o.get("confidence") or {}
    p = c.get("p")
    conf = f"{c.get('kind', '?')} p={p:.2f}" if isinstance(p, (int, float)) else str(c.get("kind") or "未判")
    lacks = [(x.get("cat") + ("（拒）" if x.get("denied") else "")) if isinstance(x, dict) else str(x)
             for x in o.get("lacks") or []]
    pend = [str(x.get("needed") or x.get("cause") or x) if isinstance(x, dict) else str(x) for x in o.get("pending") or []]
    miss = "；".join(lacks + pend) or "—"
    plan = (o.get("plan") or {}).get("title")
    form = o.get("form") or o.get("my_role") or ""
    return (f"{o.get('shape', '?'):5} {form:10} 与 {who} | 置信度 {conf} 「{c.get('q') or ''}」"
            f" | 还缺：{miss}" + (f" | 方案：{plan}" if plan else "") + f" [{o.get('status', '')}]")


def opp_sig(o: dict):
    c = o.get("confidence") or {}
    return (o.get("status"), c.get("kind"), c.get("p"), c.get("q"), json.dumps(o.get("lacks"), ensure_ascii=False),
            json.dumps(o.get("pending"), ensure_ascii=False, default=str), (o.get("plan") or {}).get("title"),
            o.get("value"))


def watch(url: str, agent_id: str, every: float):
    """轮询 towow_opportunities，机会出现、变化、消失时各打印一行。Ctrl-C 退出。"""
    import time as _t
    seen: dict[str, tuple] = {}
    try:
        while True:
            try:
                res = asyncio.run(call(url, "towow_opportunities", {"agent_id": agent_id, "token": load_token(url, agent_id)}))
            except Exception as e:      # 服务暂不可达：打印一次，继续等
                print(f"{_t.strftime('%H:%M:%S')} ! {e!r}", flush=True)
                _t.sleep(every)
                continue
            if res.get("error"):
                print(f"{_t.strftime('%H:%M:%S')} ! {res['error']}", flush=True)
            cur = {o["id"]: o for o in res.get("opportunities") or []}
            ts = _t.strftime("%H:%M:%S")
            for oid, o in cur.items():
                sig = opp_sig(o)
                if oid not in seen:
                    print(f"{ts} + {opp_line(o)}", flush=True)
                elif seen[oid] != sig:
                    print(f"{ts} ~ {opp_line(o)}", flush=True)
                seen[oid] = sig
            for oid in [k for k in seen if k not in cur]:
                print(f"{ts} - {oid} 消失", flush=True)
                seen.pop(oid)
            _t.sleep(every)
    except KeyboardInterrupt:
        return


def show(x):
    print(json.dumps(x, ensure_ascii=False, indent=2))


def serve(a):
    import uvicorn

    from host.server import build_real

    flags = {f: True for f in (a.flag or [])}
    absp = lambda x: x if (not x or os.path.isabs(x)) else os.path.join(APP_DIR, x)   # noqa: E731
    host = build_real(absp(a.program), port_judge=a.judge, seed=a.seed, flags=flags, device=a.device,
                      judge_cache=absp(a.judge_cache), keep_text=not a.public)
    if a.preload:
        from host.simulate import start_preload
        host.after_start.append(start_preload(host, absp(a.preload), n=a.preload_n, seed=a.seed,
                                              interval=a.preload_interval, background=a.preload_background))
    web = a.web if os.path.isabs(a.web) else os.path.join(APP_DIR, a.web)
    if a.public:
        host.max_real_agents = a.max_real_agents
        host.joins_path = absp(a.joins_file)       # 真实接入存盘（只有 t0 包与 token 摘要），重启后恢复
        os.makedirs(os.path.dirname(host.joins_path), exist_ok=True)
        host.load_tokens()
        from host.simulate import start_restore
        host.after_start.append(start_restore(host))
        if not os.environ.get("TOWOW_DISPLAY_TOKEN"):
            print("注意：--public 时没设 TOWOW_DISPLAY_TOKEN，3D 展示端点对所有人关闭", file=sys.stderr)
    app = host.build_app(bind_host=a.bind, web_dir=web, public=a.public)
    print(connect_lines(f"http://localhost:{a.port}"), file=sys.stderr)
    uvicorn.run(app, host=a.bind, port=a.port, log_level="info", access_log=not a.public)   # 公网时不记访问日志（查询串里有展示 token）


def main(argv=None):
    ap = argparse.ArgumentParser(prog="towow", description="通爻网：陌生人合作发现网络")
    ap.add_argument("--server", default=DEFAULT_URL)
    sp = ap.add_subparsers(dest="cmd", required=True)

    s = sp.add_parser("serve")
    s.add_argument("--port", type=int, default=8794)
    s.add_argument("--bind", default="127.0.0.1")
    s.add_argument("--program", default="app/net.jpx")
    s.add_argument("--judge", choices=["live", "jev", "fixture"], default="live",
                   help="live（=jev）：真 JEV；fixture：不花钱，引擎的离线伪读数端口")
    s.add_argument("--judge-cache", default="", help="判断缓存 sqlite（如 runs/judge-cache.sqlite），只配 live；重启预载零花费")
    s.add_argument("--preload", default="", help="启动后按种子顺序预载这些算子包（如 world/packs）")
    s.add_argument("--preload-n", type=int, default=None)
    s.add_argument("--preload-interval", type=float, default=0.0, help="0：每次接入等引擎静止")
    s.add_argument("--preload-background", action="store_true",
                   help="预载成背景人口：只进索引、被召回、答补信息请求，彼此不判断（真实接入演示用，不花钱）")
    s.add_argument("--seed", type=int, default=0)
    s.add_argument("--device", default="mps")
    s.add_argument("--web", default="web/dist")
    s.add_argument("--flag", action="append", help="消融开关，如 --flag no_batch（见 Fable-A §七）")
    s.add_argument("--public", action="store_true", help="经隧道对公网开放：展示端点要 TOWOW_DISPLAY_TOKEN，接入数受 --max-real-agents 限制")
    s.add_argument("--max-real-agents", type=int, default=200)
    s.add_argument("--joins-file", default="runs/real/joins.json", help="--public 时真实接入的存盘位置（0600）")

    sp.add_parser("spec")
    j = sp.add_parser("join")
    j.add_argument("--pack", required=True, help="算子包 JSON 文件（按 towow spec 编译）")
    j.add_argument("--name", required=True, help="主人愿意公开的称呼")
    j.add_argument("--host-agent", default="towow-cli")
    for name in ("opps", "inbox", "leave"):
        p = sp.add_parser(name)
        p.add_argument("agent_id")
        if name == "opps":
            p.add_argument("--watch", action="store_true", help="每次机会变化打印一行")
            p.add_argument("--every", type=float, default=3.0, help="--watch 的轮询秒数")
    r = sp.add_parser("respond")
    r.add_argument("agent_id")
    r.add_argument("request_id")
    g = r.add_mutually_exclusive_group(required=True)
    g.add_argument("--grant", metavar="TEXT")
    g.add_argument("--deny", action="store_true")
    sp.add_parser("connect")
    sp.add_parser("state")

    a = ap.parse_args(argv)
    url = a.server
    if a.cmd == "serve":
        return serve(a)
    if a.cmd == "connect":
        print(connect_lines(url))
        return
    if a.cmd == "state":
        import httpx
        show(httpx.get(url.rstrip("/") + "/api/state", params={"display_token": os.environ.get("TOWOW_DISPLAY_TOKEN", "")}, timeout=30).json())
        return
    if a.cmd == "leave":
        import httpx
        show(httpx.post(url.rstrip("/") + f"/api/leave/{a.agent_id}", timeout=30,
                        headers={"x-towow-token": load_token(url, a.agent_id)}).json())
        return
    if a.cmd == "spec":
        res = asyncio.run(call(url, "towow_spec", {}))
        print(res.get("spec", ""))
        show(res.get("network"))
        return
    if a.cmd == "join":
        pack = json.load(open(a.pack, encoding="utf-8"))
        who = f"name:{a.host_agent}\x00{a.name}"       # agent_id 由服务端加盐生成，客户端算不出：按称呼记下 id
        aid = load_token(url, who)
        res = asyncio.run(call(url, "towow_join", {"pack": pack, "agent_name": a.name, "host_agent": a.host_agent,
                                                   "token": load_token(url, aid) if aid else ""}))
        if res.get("token"):
            save_token(url, res["agent_id"], res["token"])        # 存在 ~/.towow/tokens.json（0600），之后的命令自动带上
            save_token(url, who, res["agent_id"])
        show(res)
        return
    if a.cmd == "opps":
        if a.watch:
            return watch(url, a.agent_id, a.every)
        show(asyncio.run(call(url, "towow_opportunities", {"agent_id": a.agent_id, "token": load_token(url, a.agent_id)})))
        return
    if a.cmd == "inbox":
        show(asyncio.run(call(url, "towow_inbox", {"agent_id": a.agent_id, "token": load_token(url, a.agent_id)})))
        return
    if a.cmd == "respond":
        show(asyncio.run(call(url, "towow_respond", {"agent_id": a.agent_id, "token": load_token(url, a.agent_id),
                                                     "request_id": a.request_id,
                                                     "grant": not a.deny, "text": a.grant or ""})))
        return


if __name__ == "__main__":
    main()
