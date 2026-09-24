"""Build tool-output chunks whose truth for 'contains an error/failure' is known by construction.

Each item is assembled from a neutral body plus, for positives, one inserted error line.
Truth is computed from how the item was built (source: computed), not from reading it.
"""
import json, random, sys
N = int(sys.argv[1]) if len(sys.argv) > 1 else 120
random.seed(20260924)
neutral = [
    "   Compiling {c} v{v}\n   Compiling {d} v{w}",
    "test {m}::tests::{t} ... ok\ntest {m}::tests::{u} ... ok",
    "src/{m}.rs:{n}:    let {t} = {u}.clone();",
    "Downloaded {c} v{v}\nDownloaded {d} v{w}",
    "drwxr-xr-x  5 dev staff  160 Sep 24 10:{n} {m}\n-rw-r--r--  1 dev staff 2048 Sep 24 10:{n} {t}.rs",
    "    Finished `dev` profile [unoptimized + debuginfo] target(s) in {n}.{n}s",
    "On branch main\nYour branch is up to date with 'origin/main'.\nmodified:   src/{m}.rs",
    "{m}.py:{n}: def {t}(self, {u}):\n{m}.py:{n}:     return self.{u}",
]
errors = [
    "error[E0308]: mismatched types\n  --> src/{m}.rs:{n}:9",
    "thread 'main' panicked at src/{m}.rs:{n}:5:\ncalled `Result::unwrap()` on an `Err` value",
    "test {m}::tests::{t} ... FAILED",
    "Traceback (most recent call last):\n  File \"{m}.py\", line {n}, in {t}\nKeyError: '{u}'",
    "npm ERR! code ELIFECYCLE\nnpm ERR! errno 1",
    "fatal: not a git repository (or any of the parent directories): .git",
    "ModuleNotFoundError: No module named '{u}'",
    "Segmentation fault (core dumped)",
]
words = ["login", "session", "metrics", "config", "router", "cache", "billing", "auth", "parser", "worker"]
crates = ["serde", "tokio", "hyper", "regex", "rand", "clap", "tracing", "axum"]
def fill(t):
    return t.format(c=random.choice(crates), d=random.choice(crates), v="1.%d.%d" % (random.randint(0, 40), random.randint(0, 9)),
                    w="0.%d.%d" % (random.randint(1, 20), random.randint(0, 9)), m=random.choice(words),
                    t=random.choice(words) + "_" + random.choice(["ok", "flow", "check", "init"]),
                    u=random.choice(words), n=random.randint(10, 99))
items = []
for i in range(N):
    body = [fill(random.choice(neutral)) for _ in range(random.randint(1, 2))]
    label = i % 2 == 0
    if label:
        body.insert(random.randint(0, len(body)), fill(random.choice(errors)))
    items.append({"id": "e%03d" % i, "text": "\n".join(body), "label": label})
json.dump(items, open("items.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
json.dump({"chunks": [x["text"] for x in items]}, open("chunks.json", "w", encoding="utf-8"), ensure_ascii=False)
print(len(items))
