"""Published results must match the inputs and source visitors can inspect."""
from hashlib import sha256
import json
from pathlib import Path
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]


def test_embedding_record_has_same_pool_and_recomputable_metrics():
    source = ROOT/'src/jpp/data/towow-population.json'
    data = json.loads(source.read_text())
    report = json.loads((ROOT/'docs/demos/towow/population/embedding-report.json').read_text())
    assert report['dataset_sha256'] == sha256(source.read_bytes()).hexdigest()
    ids = {p['id'] for p in data['people']}
    for method in report['methods'].values():
        hits = 0
        assert len(method['rows']) == len(data['intents'])
        for row, intent in zip(method['rows'], data['intents']):
            assert row['intent_id'] == intent['id']
            ranking = row['ranking']
            assert len(ranking) == len(ids)
            assert {r['person'] for r in ranking} == ids
            assert ranking == sorted(ranking, key=lambda r: (-r['score'],r['person']))
            assert row['top10'] == [r['person'] for r in ranking[:10]]
            expected_hits = sorted(set(row['top10']) & set(intent['present_expected']))
            assert row['hits'] == expected_hits
            assert row['hit_count'] == len(expected_hits)
            hits += len(expected_hits)
        assert method['summary']['hits'] == hits
        assert method['summary']['known_positive_pairs'] == sum(len(q['present_expected']) for q in data['intents'])


def test_browser_bundle_matches_repository_sources():
    path = ROOT/'docs/demos/towow/lab'
    manifest = json.loads((path/'manifest.json').read_text())
    archive = path/manifest['archive']
    assert sha256(archive.read_bytes()).hexdigest() == manifest['sha256']
    with ZipFile(archive) as bundle:
        assert set(bundle.namelist()) == set(manifest['files'])
        for name, digest in manifest['files'].items():
            current = (ROOT/'src'/name).read_bytes()
            assert bundle.read(name) == current
            assert sha256(current).hexdigest() == digest
