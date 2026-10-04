"""预注册 08：用 claude -p（Sonnet）生成互不相同的背景人口，只写 t0 片段。
每批 25 人，按种子分配城市 × 行业 × 生活阶段；输出 runs/scale/genbg/g00001.json …（已有的批次跳过，可续跑）。
用法：.venv/bin/python -m host.gen_bg --n 9500 --jobs 6
"""
from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import os
import random
import re
import subprocess
import sys

APP_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(APP_DIR, "runs", "scale", "genbg")

CITIES = ["北京", "上海", "广州", "深圳", "成都", "重庆", "杭州", "武汉", "西安", "南京", "苏州", "天津", "长沙", "郑州",
          "青岛", "厦门", "昆明", "贵阳", "南宁", "哈尔滨", "沈阳", "大连", "济南", "合肥", "福州", "南昌", "太原", "兰州",
          "乌鲁木齐", "呼和浩特", "拉萨", "海口", "三亚", "温州", "宁波", "无锡", "佛山", "东莞", "珠海", "洛阳", "大理",
          "丽江", "桂林", "扬州", "绍兴", "泉州", "汕头", "潮州", "景德镇", "宜兴", "香港", "台北", "新加坡", "吉隆坡",
          "曼谷", "东京", "大阪", "首尔", "悉尼", "墨尔本", "伦敦", "曼彻斯特", "柏林", "巴黎", "阿姆斯特丹", "多伦多",
          "温哥华", "纽约", "旧金山", "洛杉矶", "芝加哥", "西雅图", "内罗毕", "拉各斯", "开普敦", "圣保罗", "墨西哥城",
          "孟买", "班加罗尔", "雅加达", "胡志明市", "马尼拉", "迪拜", "伊斯坦布尔", "县城", "乡镇", "海岛", "山区村庄"]
DOMAINS = ["农业与养殖", "餐饮", "零售小店", "手工艺", "制造与工厂", "物流运输", "建筑装修", "医疗护理", "养老", "教育培训",
           "幼儿与家庭", "心理与社工", "法律", "财务会计", "金融保险", "房地产", "软件开发", "硬件与电子", "数据与AI",
           "设计", "摄影影像", "音乐", "表演艺术", "文学写作", "新闻媒体", "自媒体", "电商", "跨境贸易", "旅游酒店",
           "体育健身", "户外运动", "宠物", "环保公益", "宗教文化", "非遗传承", "考古文博", "科研学术", "政府与社区",
           "汽车维修", "能源", "海洋渔业", "林业园艺", "美容美发", "时尚服装", "游戏", "动漫", "出版", "翻译",
           "家政服务", "安保", "航空", "航运", "铁路", "矿业", "化工", "纺织", "茶与咖啡", "酒类", "烘焙"]
STAGES = ["学生", "刚毕业", "职场新人", "资深从业者", "中年转行", "小老板", "自由职业", "全职父母", "退休",
          "创业早期", "病后休养", "刚搬到新城市", "副业起步", "失业找方向", "海外归来"]

PROMPT = """生成 {n} 个虚构的普通人，每人是一个 personal agent 替主人交给陌生人发现网络的「公开一句话」算子包。只写可以公开的内容（t0）。
每个人的主题由下面的种子决定（城市 / 行业 / 生活阶段），彼此必须明显不同；不要写名人，不要写真实可识别的个人，不要和其他人重复句子。
{seeds}

每个人输出一个 JSON 对象，字段：
- "display": 一句话自我介绍（具体：城市、做什么、当前处境），30–60 字
- "signals": 3 条，这个人最近真实的需要或困扰，具体到场景，每条 20–50 字
- "offers": 3 条，这个人能给别人的具体技能、资源或经验，每条 20–50 字
- "catchers": 2 条，每条 {{"hypo": 设想中会有人这样来找他（第一人称来信，20–50 字）, "can": 他能怎么接（20–40 字）, "confirm": 一个确认来信人是对口的人的问句}}
- "lang": "zh"（种子城市在海外时可用 "en" 并用英文写）

只输出一个 JSON 数组，不要任何解释或代码块标记。"""


def seeds_for(batch: int, n: int) -> list[str]:
    rng = random.Random(1000 + batch)
    return [f"{i + 1}. {rng.choice(CITIES)} / {rng.choice(DOMAINS)} / {rng.choice(STAGES)}" for i in range(n)]


def gen_batch(batch: int, n: int, model: str) -> str:
    path = os.path.join(OUT, f"batch{batch:04d}.json")
    if os.path.exists(path):
        return f"skip {batch}"
    prompt = PROMPT.format(n=n, seeds="\n".join(seeds_for(batch, n)))
    for attempt in range(2):
        try:
            p = subprocess.run(["claude", "-p", "--model", model, "--output-format", "text"], input=prompt.encode(),
                               capture_output=True, timeout=600)
        except subprocess.TimeoutExpired:
            continue
        txt = p.stdout.decode()
        m = re.search(r"\[.*\]", txt, re.S)
        if p.returncode == 0 and m:
            try:
                arr = json.loads(m.group(0))
                if isinstance(arr, list) and len(arr) >= n * 0.8:
                    json.dump(arr, open(path, "w", encoding="utf-8"), ensure_ascii=False)
                    return f"ok {batch} {len(arr)}"
            except json.JSONDecodeError:
                pass
    return f"fail {batch}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--n", type=int, default=9500)
    ap.add_argument("--per", type=int, default=25)
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument("--model", default="sonnet")
    a = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)
    nb = (a.n + a.per - 1) // a.per
    done = 0
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        for r in ex.map(lambda b: gen_batch(b, a.per, a.model), range(nb)):
            done += 1
            if done % 10 == 0 or r.startswith("fail"):
                print(done, nb, r, flush=True)
    print("done", flush=True)


if __name__ == "__main__":
    sys.exit(main())
