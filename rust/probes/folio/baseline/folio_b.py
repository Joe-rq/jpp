import json

from jev_interface import judge


def load_materials():
    with open("materials.json", encoding="utf-8") as fh:
        return json.load(fh)


def decide_choice(answer, threshold):
    probs = answer["probabilities"]
    n = len(probs)
    best_idx = 0
    best_p = probs["c0"]
    for i in range(1, n):
        p = probs[f"c{i}"]
        if p > best_p:
            best_p = p
            best_idx = i
    mode_share = answer.get("mode_share")
    if mode_share is not None and mode_share == 1.0 and best_p >= threshold["hi"] + threshold["delta"]:
        return best_idx
    return None


def decide_noul(answer, threshold):
    p = answer["noul"]
    if p >= threshold["hi"] + threshold["delta"]:
        return True
    if p <= threshold["lo"] - threshold["delta"]:
        return False
    return None


def run_single_path(root, tree, document, threshold):
    current = root
    path = []
    stopped = "depth"
    for _ in range(6):
        if current not in tree:
            stopped = "leaf"
            break
        children = tree[current]
        answer = judge(document, [{
            "type": "choice",
            "instructions": "这份法律文书最应归入下列哪一类？",
        }], over=children)[0]
        idx = decide_choice(answer, threshold)
        if idx is None:
            stopped = "unsure"
            break
        current = children[idx]
        path.append(current)
    else:
        stopped = "depth"
    return {"leaf": current, "path": path, "stopped": stopped}


def run_multi_path(root, tree, document, threshold):
    frontier = [root]
    leaves = []
    dead_ends = []
    layers = 0
    undecided = 0
    for _ in range(6):
        reached = [c for c in frontier if c not in tree]
        internal = [c for c in frontier if c in tree]
        if not internal:
            leaves.extend(reached)
            break
        next_frontier = []
        for category in internal:
            children = tree[category]
            any_yes = False
            for child in children:
                answer = judge(document, [{
                    "type": "noul",
                    "instructions": f"这段话的内容是否与「{child}」这个话题相关？",
                }])[0]
                decision = decide_noul(answer, threshold)
                if decision is True:
                    next_frontier.append(child)
                    any_yes = True
                elif decision is None:
                    undecided += 1
            if not any_yes:
                dead_ends.append(category)
        leaves.extend(reached)
        layers += 1
        frontier = next_frontier
    return {
        "leaves": leaves,
        "dead_ends": dead_ends,
        "layers": layers,
        "undecided": undecided,
    }


def main():
    materials = load_materials()
    root = materials["root"]
    tree = materials["tree"]
    document = materials["document"]
    thresholds = materials["thresholds"]

    single_path = run_single_path(root, tree, document, thresholds["folio-level"])
    multi_path = run_multi_path(root, tree, document, thresholds["form-topic"])

    result = {"single_path": single_path, "multi_path": multi_path}
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
