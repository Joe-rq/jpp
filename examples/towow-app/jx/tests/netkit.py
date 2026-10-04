"""离线跑 app/net.jpx 的小工具：host 的索引与图算法动作 + 哈希编码器 + fixture 端口 + 手写的小算子包。"""
from __future__ import annotations

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

from jx.engine import Engine, VirtualClock  # noqa: E402
from jx.sched import FixturePort, HashEncoder  # noqa: E402


def pack(i, display, signals, offers, catchers, t1=(), projects=()):
    return {"id": f"n{i:02d}", "display": display, "lang": "zh",
            "signals": [{"text": s, "tier": "t0", "src": "note"} for s in signals] +
                       [{"text": s, "tier": "t1", "src": "note"} for s in t1],
            "offers": [{"text": s, "tier": "t0", "src": "skill"} for s in offers],
            "catchers": [{"hypo": hy, "can": can, "confirm": cf, "tier": "t0", "src": "bio"} for hy, can, cf in catchers],
            "forbids": [], "projects": [{"name": p, "text": p, "roles_needed": ["设计", "开发"], "tier": "t0"} for p in projects],
            "policy": {"t1": "有合作苗头时愿意说具体情况", "t2": "把握高时可以给联系方式", "never": "家庭财务"}}


PACKS = [
    pack(1, "独立插画师", ["童书系列卡在故事节奏上"], ["儿童插画", "品牌插画"], [("我想把亲子课程做成图文册子", "画插图", "对方是否需要插画？")], t1=["这个月收入很少"]),
    pack(2, "儿童心理咨询师", ["想把工作坊做成图文小册子，不会画画"], ["儿童心理咨询", "亲子工作坊"], [("童书故事节奏不对，需要懂儿童的人看看", "审读儿童故事", "对方的书是否面向儿童？")]),
    pack(3, "咖啡馆老板", ["店庆想办小展览，缺作品"], ["闲置小展厅", "咖啡馆客流"], [("我有作品想找地方展", "提供展厅", "对方是否有可展出的实物作品？")]),
    pack(4, "陶艺人", ["作品多但没有固定展示地方"], ["手作陶器", "陶艺体验课"], [("店里缺有手感的器物", "供货寄售", "对方是否是实体店？")]),
    pack(5, "有机菜园主", ["散户卖不完菜"], ["当季有机蔬菜", "周配送"], [("餐厅想要稳定的本地菜", "直供", "对方是否是餐饮？")]),
    pack(6, "粤菜私厨", ["食材供应不稳定"], ["私宴", "粤菜"], [("想办一场小型私宴", "上门做菜", "对方是否要办宴？")]),
    pack(7, "独立开发者", ["记账小程序增长停了"], ["小程序开发"], [("想做个小程序", "开发", "对方是否需要开发？")], projects=["记账小程序 2.0"]),
    pack(8, "自由运营", ["在找按效果付费的增长项目"], ["增长运营", "社群"], [("产品没人用", "做增长", "对方是否有产品？")]),
]


def make_engine(flags=None, seed=0, judge=None, src_path=None, **kw):
    from host.graph import make_do_actions as graph_actions
    from host.index import FragmentIndex, make_do_actions as index_actions
    enc = HashEncoder()
    eng = Engine.load(src_path or os.path.join(ROOT, "app", "net.jpx"), ports={"judge": judge or FixturePort(skew=4.0), "enc": enc},
                      flags=flags or {}, seed=seed, clock=VirtualClock(), **kw)
    ix = FragmentIndex(enc, cap=1024, threads=1)
    for name, fn in index_actions(ix).items():
        eng.register_action(name, fn, transparent=(name != "index_put"))   # 与宿主一致：route 不挂 node 族依赖（主会话裁定）
    for name, fn in graph_actions().items():
        eng.register_action(name, fn, transparent=True)
    eng.index = ix
    return eng
