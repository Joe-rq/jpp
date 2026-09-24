"""步 20e 验证：为 align-link（打分题，三档）造带计算真值的标注。

真值由构造给出（source: computed）：
- 档 2「同一款产品」：同一条记录的改写（名字加酒厂前缀或去掉风格后缀、酒厂写全称或简称、风格写近义）。
- 档 0「两种不同的产品」：不同酒厂的不同产品。
档 1（变体、特别版）真值不可计算，不造。读数来自真实 JEV（一次调用一对），标注行写
p = 胜出档位概率、pick = argmax、label = 真值档位（B63）。
"""
import concurrent.futures as cf
import json
import pathlib
import random
import time
import urllib.request

random.seed(20260924)
KEY = (pathlib.Path.home() / ".typesafe-key").read_text().strip()
URL = "https://api.typesafe.ai/v1/systemone"
HERE = pathlib.Path(__file__).parent

SCALE = [
    "它们描述的是两种不同的产品。",
    "它们描述的是密切相关、可能是也可能不是同一款的产品：变体、特别版，或名字两种理解都说得通。",
    "它们描述的是同一款产品。",
]
TEXT = "两条实体描述作为产品是什么关系？"

BEERS = [
    ("Hazy Little Thing IPA", "Sierra Nevada", "Sierra Nevada Brewing Co.", "New England IPA", "Hazy IPA", 6.7),
    ("Guinness Draught", "Guinness", "Guinness Brewery", "Irish Dry Stout", "Dry Stout", 4.2),
    ("Two Hearted Ale", "Bell's", "Bell's Brewery", "American IPA", "IPA", 7.0),
    ("Pliny the Elder", "Russian River", "Russian River Brewing", "Double IPA", "Imperial IPA", 8.0),
    ("Heady Topper", "The Alchemist", "The Alchemist Brewery", "Double IPA", "DIPA", 8.0),
    ("Duvel", "Duvel Moortgat", "Brouwerij Duvel Moortgat", "Belgian Strong Golden Ale", "Golden Strong Ale", 8.5),
    ("Chimay Blue", "Chimay", "Bières de Chimay", "Belgian Strong Dark Ale", "Dark Strong Ale", 9.0),
    ("Weihenstephaner Hefeweissbier", "Weihenstephan", "Bayerische Staatsbrauerei Weihenstephan", "Hefeweizen", "Wheat Beer", 5.4),
    ("Pilsner Urquell", "Plzeňský Prazdroj", "Pilsner Urquell Brewery", "Czech Pilsner", "Bohemian Pilsner", 4.4),
    ("Samuel Adams Boston Lager", "Boston Beer Company", "The Boston Beer Co.", "Vienna Lager", "Amber Lager", 5.0),
    ("Blue Moon Belgian White", "Blue Moon Brewing", "Blue Moon Brewing Company", "Witbier", "Belgian White", 5.4),
    ("Fat Tire Amber Ale", "New Belgium", "New Belgium Brewing", "Amber Ale", "American Amber", 5.2),
    ("Stone IPA", "Stone Brewing", "Stone Brewing Co.", "West Coast IPA", "American IPA", 6.9),
    ("Lagunitas IPA", "Lagunitas", "Lagunitas Brewing Company", "American IPA", "IPA", 6.2),
    ("Founders Breakfast Stout", "Founders", "Founders Brewing Co.", "Imperial Stout", "Oatmeal Stout", 8.3),
    ("Westmalle Tripel", "Westmalle", "Brouwerij der Trappisten van Westmalle", "Tripel", "Belgian Tripel", 9.5),
    ("Orval", "Orval", "Brasserie d'Orval", "Belgian Pale Ale", "Trappist Pale Ale", 6.2),
    ("La Fin du Monde", "Unibroue", "Unibroue Brewery", "Tripel", "Belgian Tripel", 9.0),
    ("Dogfish Head 60 Minute IPA", "Dogfish Head", "Dogfish Head Craft Brewery", "American IPA", "IPA", 6.0),
    ("Allagash White", "Allagash", "Allagash Brewing Company", "Witbier", "Belgian Wheat", 5.2),
]


def rec(name, brewery, style, abv):
    return {"name": name, "brewery": brewery, "style": style, "abv": abv}


def same_pair(b):
    name, br, br_full, st, st_alt, abv = b
    a = rec(name, br, st, abv)
    variants = [
        rec(f"{br} {name}", br_full, st_alt, abv),
        rec(name, br_full, st, abv),
        rec(name.replace(" Ale", "").replace(" IPA", ""), br, st_alt, abv),
    ]
    return a, random.choice(variants)


def diff_pair(b1, b2):
    return rec(b1[0], b1[1], b1[3], b1[5]), rec(b2[0], b2[1], b2[3], b2[5])


def call(on):
    body = {"state": {"on": on}, "model": "jev-1.13.0",
            "questions": {"q0": {"type": "score", "instructions": TEXT, "criteria": SCALE}}}
    for a in range(5):
        try:
            req = urllib.request.Request(URL, data=json.dumps(body, ensure_ascii=False).encode(),
                                         headers={"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"})
            r = json.load(urllib.request.urlopen(req, timeout=60))
            pr = r["answers"]["q0"]["probabilities"]
            v = [float(pr.get(str(k), 0.0)) for k in range(len(SCALE))]
            return v, r.get("usage", {}).get("input_tokens", 0)
        except Exception:
            time.sleep(2 ** a)
    return None, 0


def main():
    items = []
    for i in range(130):
        a, b = same_pair(BEERS[i % len(BEERS)])
        items.append((f"same-{i}", {"a": a, "b": b}, 2))
    for i in range(130):
        b1, b2 = random.sample(BEERS, 2)
        items.append((f"diff-{i}", {"a": rec(b1[0], b1[1], b1[3], b1[5]), "b": rec(b2[0], b2[1], b2[3], b2[5])}, 0))
    out = []
    tokens = 0
    with cf.ThreadPoolExecutor(12) as ex:
        for (item, on, truth), (v, t) in zip(items, ex.map(lambda x: call(x[1]), items)):
            tokens += t
            if v is None:
                continue
            k = max(range(len(v)), key=lambda j: v[j])
            out.append({"key": "align-link", "op": "measure", "item": item, "p": v[k], "pick": k,
                        "label": truth, "source": "computed", "readings": v})
    with open(HERE / "labels.jsonl", "w") as f:
        for r in out:
            f.write(json.dumps({k: r[k] for k in ["key", "op", "item", "p", "pick", "label", "source"]}, ensure_ascii=False) + "\n")
    with open(HERE / "readings.jsonl", "w") as f:
        for r in out:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(json.dumps({"rows": len(out), "tokens": tokens, "usd_est": round(tokens * 4.2e-8, 5),
                      "correct": sum(r["pick"] == r["label"] for r in out)}))


if __name__ == "__main__":
    main()
