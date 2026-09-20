"""The fields-vs-types comparison: routing policy, not taxonomy, is the active factor."""

import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "examples"))

_spec = importlib.util.spec_from_file_location(
    "compare_primitive_schemes", ROOT / "scripts" / "compare_primitive_schemes.py")
compare = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(compare)


def test_capability_routing_is_stable_under_both_taxonomies():
    summary = compare.main()
    # All four cells route the truth correctly.
    for router in ("capability", "tag"):
        for taxonomy in ("seven", "five"):
            assert summary[router][taxonomy]["truth_hits"] == summary["cases"]
    # Capability routing never flips, whatever the taxonomy.
    assert summary["capability"]["seven"]["flips"] == 0
    assert summary["capability"]["five"]["flips"] == 0
    # Type-tag routing flips under both taxonomies: taxonomy is not the
    # active ingredient — routing policy is.
    assert summary["tag"]["seven"]["flips"] == 4
    assert summary["tag"]["five"]["flips"] == 4


def test_capability_rows_run_the_real_method(capsys):
    compare.main()
    report = json.loads(capsys.readouterr().out)
    for row in report["rows"]:
        for taxonomy in ("seven", "five"):
            capability = row[taxonomy]["capability"]
            tag = row[taxonomy]["tag"]
            # The confusions that flip tag routing are exactly the merged
            # pairs; entity/resource stays put in every cell.
            if row["pair"] == "entity/resource":
                assert not capability["flip"] and not tag["flip"]
            else:
                assert tag["flip"] and not capability["flip"]
            # The entity/resource pair is harmless because both members
            # route to reflection under every router and taxonomy.
            if row["pair"] == "entity/resource":
                assert capability["truth"] == tag["truth"] == "reflection"
