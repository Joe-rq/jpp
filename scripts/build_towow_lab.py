"""Bundle the current Python sources for browser execution; no alternate runtime."""
from hashlib import sha256
import json
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

ROOT = Path(__file__).resolve().parents[1]
DEST = ROOT / "docs/demos/towow/lab"


def build():
    DEST.mkdir(parents=True, exist_ok=True)
    entries = {}
    for package in ("foundation", "jev_compose", "jpp"):
        for path in sorted((ROOT / "src" / package).rglob("*")):
            if path.suffix in (".py", ".json") and "__pycache__" not in path.parts:
                entries[path.relative_to(ROOT / "src").as_posix()] = path.read_bytes()
    with ZipFile(DEST / "jpp-source.zip", "w", compression=ZIP_DEFLATED) as archive:
        for name, content in sorted(entries.items()):
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            archive.writestr(info, content)
    manifest = {"archive": "jpp-source.zip", "sha256": sha256((DEST / "jpp-source.zip").read_bytes()).hexdigest(),
                "files": {name: sha256(data).hexdigest() for name, data in sorted(entries.items())}}
    (DEST / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    population = ROOT / "src/jpp/data/towow-population.json"
    if population.exists():
        public_data = DEST.parent / "population/data.json"
        public_data.parent.mkdir(parents=True, exist_ok=True)
        public_data.write_bytes(population.read_bytes())
    print(f"Browser bundle: {len(entries)} source/data files, {(DEST / 'jpp-source.zip').stat().st_size} bytes")


if __name__ == "__main__":
    build()
