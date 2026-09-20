"""Animate the recorded discovery trace without making new model calls."""
from importlib.resources import files
import json
from pathlib import Path


def write_view(report, path):
    template = files("jpp").joinpath("data/towow-animation.html").read_text(encoding="utf-8")
    data = json.dumps(report, ensure_ascii=False).replace("<", "\\u003c")
    Path(path).write_text(template.replace("__REPORT__", data), encoding="utf-8")
