"""Trim paper abstracts from real-source academic profiles; no network access.

Nature's decision on docs/demos/towow/real/data.json (2026-09-25): keep the
325 profiles on their real public sources, do not treat the dataset as
de-identified, do not rewrite git history. Going forward, remove the paper
abstract text from the academic-source entries -- the long passage between
the paper title and the trailing OpenAlex concept tags -- and replace it
with one short sentence naming the field already recorded on that entry.
Institution, paper title and OpenAlex tags are kept verbatim; GitHub and
startup entries are untouched, since they were never built from a title
plus an abstract.

Idempotent: running this twice on an already-trimmed file is a no-op. If
docs/demos/towow/real/data.json is ever regenerated from
scripts/prepare_towow_real.py against the original source corpus, the
abstracts come back and this script must be rerun before republishing.
"""
import argparse
import json
from pathlib import Path
import re

LEAD_PATTERN = re.compile(r"^(?P<lead>Public author profile is associated with .+?\.)\s+")
TITLE_PATTERN = re.compile(r"^Recent work in (?P<field>.+?) includes '(?P<title>.+?)'\.\s*")
TAIL_PATTERN = re.compile(r"^.*?(?P<tail>OpenAlex concept tags include .+)$", re.S)


def trim_context(context):
    lead_match = LEAD_PATTERN.match(context)
    lead = lead_match.group("lead") + " " if lead_match else ""
    rest = context[lead_match.end():] if lead_match else context
    title_match = TITLE_PATTERN.match(rest)
    if not title_match:
        raise ValueError(f"unrecognized academic context shape: {context[:120]!r}")
    field, title = title_match.group("field"), title_match.group("title")
    after_title = rest[title_match.end():]
    tail_match = TAIL_PATTERN.match(after_title)
    tail = (" " + tail_match.group("tail")) if tail_match else ""
    return f"{lead}Recent work in {field} includes '{title}'. The paper is about {field}.{tail}"


def trim(data):
    changed = 0
    for person in data["people"]:
        if person.get("source_type") == "academic":
            new_context = trim_context(person["context"])
            if new_context != person["context"]:
                changed += 1
            person["context"] = new_context
    return changed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("data", type=Path, help="docs/demos/towow/real/data.json")
    parser.add_argument("--check", action="store_true",
                         help="fail if the file is not already trimmed, without writing")
    args = parser.parse_args()
    data = json.loads(args.data.read_text(encoding="utf-8"))
    academic_total = sum(1 for p in data["people"] if p.get("source_type") == "academic")
    changed = trim(data)
    if args.check:
        if changed:
            raise SystemExit(f"{changed} of {academic_total} academic entries still need trimming")
        print(f"already trimmed: {academic_total} academic entries checked, 0 changed")
        return
    args.data.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"trimmed {changed} of {academic_total} academic entries")


if __name__ == "__main__":
    main()
