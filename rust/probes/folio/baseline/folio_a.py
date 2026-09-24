import json

from jev_interface import judge


def run_single_path(root, tree, document, threshold):
    hi = threshold["hi"]
    delta = threshold["delta"]

    current = root
    path = []
    stopped = "depth"

    for _ in range(6):
        children = tree.get(current)
        if not children:
            stopped = "leaf"
            break

        answers = judge(
            document,
            [{"type": "choice", "instructions": "这份法律文书最应归入下列哪一类？"}],
            over=children,
        )
        answer = answers[0]
        probs = answer["probabilities"]
        best_key = max(probs, key=lambda k: probs[k])
        best_p = probs[best_key]
        mode_share = answer.get("mode_share")
        selected = mode_share is not None and mode_share == 1.0 and best_p >= hi + delta

        if not selected:
            stopped = "unsure"
            break

        idx = int(best_key[1:])
        current = children[idx]
        path.append(current)
    else:
        stopped = "depth"

    return {"leaf": current, "path": path, "stopped": stopped}


def run_multi_path(root, tree, document, threshold):
    hi = threshold["hi"]
    lo = threshold["lo"]
    delta = threshold["delta"]

    frontier = [root]
    leaves = []
    dead_ends = []
    layers = 0
    undecided = 0

    for _ in range(6):
        internal = [c for c in frontier if tree.get(c)]
        round_leaves = [c for c in frontier if not tree.get(c)]

        if not internal:
            leaves.extend(round_leaves)
            break

        next_frontier = []
        for category in internal:
            any_yes = False
            for child in tree[category]:
                answers = judge(
                    document,
                    [{
                        "type": "noul",
                        "instructions": f"这段话的内容是否与「{child}」这个话题相关？",
                    }],
                )
                p = answers[0]["noul"]
                if p >= hi + delta:
                    next_frontier.append(child)
                    any_yes = True
                elif p <= lo - delta:
                    pass
                else:
                    undecided += 1
            if not any_yes:
                dead_ends.append(category)

        leaves.extend(round_leaves)
        layers += 1
        frontier = next_frontier

    return {
        "leaves": leaves,
        "dead_ends": dead_ends,
        "layers": layers,
        "undecided": undecided,
    }


def main():
    with open("materials.json", encoding="utf-8") as fh:
        materials = json.load(fh)

    root = materials["root"]
    tree = materials["tree"]
    document = materials["document"]
    thresholds = materials["thresholds"]

    single_path = run_single_path(root, tree, document, thresholds["folio-level"])
    multi_path = run_multi_path(root, tree, document, thresholds["form-topic"])

    print(json.dumps({"single_path": single_path, "multi_path": multi_path}, ensure_ascii=False))


if __name__ == "__main__":
    main()
