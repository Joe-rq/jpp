"""Trim paper abstracts from real-source academic profiles; no network access.

Nature's decision on docs/demos/towow/real/data.json (2026-09-25): keep the
325 profiles on their real public sources, do not treat the dataset as
de-identified, do not rewrite git history. Going forward, remove the paper
abstract text from the academic-source entries -- the long passage between
the paper title and the trailing OpenAlex concept tags -- and replace it
with a hand-written, one-sentence description of what that specific paper
does, read from scripts/towow_real_paper_descriptions.json (title -> up to
25-word English description, paraphrased, not copied from the abstract).
Institution, paper title and OpenAlex tags are kept verbatim; GitHub and
startup entries are untouched, since they were never built from a title
plus an abstract.

Idempotent: running this twice on an already-trimmed file is a no-op. If
docs/demos/towow/real/data.json is ever regenerated from
scripts/prepare_towow_real.py against the original source corpus, the
abstracts come back and this script must be rerun (with --check to verify)
before republishing.

2026-09-25 note: an earlier version of this script, and the first version of
data.json it produced, used a generic "The paper is about <field>." sentence
instead of a per-paper description. The coordinating session judged that
sentence redundant with the "Recent work in <field> includes" clause right
before it and asked for 40 hand-written one-liners instead, one per distinct
title. This version reads those from the checked-in mapping file so the
transform stays a single, reviewable source of truth instead of a second
one-off edit to data.json.
"""
import argparse
import json
from pathlib import Path
import re

DEFAULT_DESCRIPTIONS_PATH = Path(__file__).with_name("towow_real_paper_descriptions.json")
MAX_DESCRIPTION_WORDS = 25

LEAD_PATTERN = re.compile(r"^(?P<lead>Public author profile is associated with .+?\.)\s+")
TITLE_PATTERN = re.compile(r"^Recent work in (?P<field>.+?) includes '(?P<title>.+?)'\.\s*")
TAIL_PATTERN = re.compile(r"^.*?(?P<tail>OpenAlex concept tags include .+)$", re.S)


def load_descriptions(path=DEFAULT_DESCRIPTIONS_PATH):
    descriptions = json.loads(path.read_text(encoding="utf-8"))
    for title, description in descriptions.items():
        word_count = len(description.split())
        if word_count > MAX_DESCRIPTION_WORDS:
            raise ValueError(
                f"description for {title!r} is {word_count} words, over the "
                f"{MAX_DESCRIPTION_WORDS}-word limit: {description!r}")
    return descriptions


def trim_context(context, descriptions):
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
    if title not in descriptions:
        raise KeyError(
            f"no hand-written description for title {title!r}; add one to "
            f"{DEFAULT_DESCRIPTIONS_PATH.name} before trimming this entry")
    description = descriptions[title]
    return f"{lead}Recent work in {field} includes '{title}'. {description}{tail}"


def trim(data, descriptions):
    changed = 0
    for person in data["people"]:
        if person.get("source_type") == "academic":
            new_context = trim_context(person["context"], descriptions)
            if new_context != person["context"]:
                changed += 1
            person["context"] = new_context
    return changed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("data", type=Path, help="docs/demos/towow/real/data.json")
    parser.add_argument("--descriptions", type=Path, default=DEFAULT_DESCRIPTIONS_PATH,
                         help="title -> one-sentence description JSON mapping")
    parser.add_argument("--check", action="store_true",
                         help="fail if the file is not already trimmed, without writing")
    args = parser.parse_args()
    descriptions = load_descriptions(args.descriptions)
    data = json.loads(args.data.read_text(encoding="utf-8"))
    academic_total = sum(1 for p in data["people"] if p.get("source_type") == "academic")
    changed = trim(data, descriptions)
    if args.check:
        if changed:
            raise SystemExit(f"{changed} of {academic_total} academic entries still need trimming")
        print(f"already trimmed: {academic_total} academic entries checked, 0 changed")
        return
    args.data.write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"trimmed {changed} of {academic_total} academic entries")


if __name__ == "__main__":
    main()
