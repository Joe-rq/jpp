import importlib.util
import json
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"


def _load(name):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_regenerating_real_data_never_restores_paper_abstracts(tmp_path):
    descriptions = json.loads((SCRIPTS / "towow_real_paper_descriptions.json").read_text(encoding="utf-8"))
    title = next(iter(descriptions))
    abstract = "We propose a verbatim abstract sentence that must never be republished in the demo corpus."
    root = tmp_path / "corpus"
    (root / "profiles/normalized").mkdir(parents=True)
    (root / "relations").mkdir()
    (root / "profiles/normalized/a.json").write_text(json.dumps({
        "profile_id": "a", "name": "Ada Example", "source_type": "academic",
        "normalized_text": (f"Public author profile is associated with Example University. "
                            f"Recent work in statistics includes '{title}'. {abstract} "
                            f"OpenAlex concept tags include Statistics, Clustering."),
    }))
    (root / "profiles/normalized/b.json").write_text(json.dumps({
        "profile_id": "b", "name": "Bob Builder", "source_type": "github",
        "normalized_text": "Maintains a data orchestration library.",
    }))
    (root / "relations/ground_truth.json").write_text(json.dumps({"relations": []}))
    destination = tmp_path / "data.json"

    _load("prepare_towow_real").prepare(root, destination)

    people = {p["id"]: p for p in json.loads(destination.read_text(encoding="utf-8"))["people"]}
    academic = people["p001"]["context"]
    assert abstract not in academic
    assert f"'{title}'" in academic and descriptions[title] in academic
    assert academic.endswith("OpenAlex concept tags include Statistics, Clustering.")
    assert people["p002"]["context"] == "Maintains a data orchestration library."
