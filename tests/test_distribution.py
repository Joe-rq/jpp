"""The release command and the explicit source allowlist stay aligned."""
import importlib.util
import json
from pathlib import Path

from jpp.cli import main


def test_methods_command_writes_requested_report(tmp_path, capsys):
    target = tmp_path / "report.json"
    main(["methods", "--output", str(target)])
    report = json.loads(target.read_text())
    assert report["cases"][1]["output"][1] == "x if x >= 0 else -x"
    assert len(report["cases"][1]["dynamic"]) == 4
    assert report["replacement"]["changed"]["calls"] == 0


def test_sync_excludes_unreviewed_modules_and_preserves_research(tmp_path):
    repo = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location("sync_composition", repo / "tools/sync-composition.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    workspace, release = tmp_path / "research", tmp_path / "release"
    source = workspace / "扩展/codex_composition/jev_compose"
    source.mkdir(parents=True)
    (release / "tools").mkdir(parents=True)
    manifest = (repo / "tools/composition-files.txt").read_text()
    (release / "tools/composition-files.txt").write_text(manifest)
    for name in manifest.splitlines() + ["semantic_functions.py", "semantic_example.py", "cli.py"]:
        (source / name).write_text("# sentinel\n")
    target = release / "src/jev_compose"
    target.mkdir(parents=True)
    (target / "semantic_functions.py").write_text("# old broad sync\n")
    module.sync(workspace, release)
    assert {p.name for p in target.glob("*.py")} == set(manifest.splitlines())
    assert (source / "semantic_functions.py").exists()


def test_published_shell_probe_edits_config_portably():
    from foundation.jv.probes.p77 import _GOALS
    from foundation.jv.probes._common import run_sh
    _, setup, commands, classify = _GOALS[5]
    result = run_sh(commands[0], setup)
    assert result["exit"] == 0
    assert result["files"]["cfg.ini"] == "debug=true\nport=1\n"
    assert classify(result["files"], result["stdout"]) == 2
