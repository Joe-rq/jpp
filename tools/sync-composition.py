"""Publish only the reviewed composition modules; never copy research wholesale."""
from pathlib import Path
import shutil
import sys


def sync(workspace, repo):
    source = Path(workspace) / "扩展/codex_composition/jev_compose"
    target = Path(repo) / "src/jev_compose"
    files = (Path(repo) / "tools/composition-files.txt").read_text().splitlines()
    target.mkdir(parents=True, exist_ok=True)
    for name in files:
        if not name or Path(name).name != name or not name.endswith(".py"):
            raise ValueError(f"Invalid delivery filename: {name!r}")
        shutil.copy2(source / name, target / name)
    # These unreviewed research modules may be left by the previous broad sync.
    # Remove only their release copies; never change research files.
    for name in ("semantic_functions.py", "semantic_example.py", "cli.py"):
        (target / name).unlink(missing_ok=True)


if __name__ == "__main__":
    sync(sys.argv[1], Path(__file__).resolve().parents[1])
