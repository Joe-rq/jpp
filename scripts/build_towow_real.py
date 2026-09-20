"""Publish all real-source development comparisons, including regressions."""
import argparse
import gzip
import json
from hashlib import sha256
from pathlib import Path


def build(run_dir, public):
    data_path = public / 'data.json'
    data = json.loads(data_path.read_text())
    reports = {key: json.loads((run_dir / name).read_text()) for key, name in {
        'cut20': 'report-20.json', 'cut324': 'report-324.json',
        'order20': 'report-order-20.json',
    }.items()}
    digest = sha256(data_path.read_bytes()).hexdigest()
    assert all(r['data_sha256'] == digest for r in reports.values())
    methods = []
    specs = [('minilm', 'MiniLM 向量检索', 'cut20', 'minilm'),
             ('bm25', 'BM25 词面检索', 'cut20', 'bm25'),
             ('rrf', '本地融合检索 RRF', 'cut20', 'rrf'),
             ('cut20', 'J++ 等级排序 · 前 20', 'cut20', 'jpp'),
             ('cut324', 'J++ 等级排序 · 全部 324', 'cut324', 'jpp'),
             ('pair_cut20', '独立配对 · 接受等级排序', 'order20', 'jpp_acceptance'),
             ('order20', '独立配对 · J++ 相对排序', 'order20', 'jpp')]
    for key, label, report_key, method in specs:
        report = reports[report_key]
        entry = {'id': key, 'label': label, 'rankings': report['rankings'][method],
                 'metrics': report['metrics'][method]}
        if method.startswith('jpp'):
            entry['grades'] = report['grades']
        methods.append(entry)
    hits = {m['id']: m['metrics']['10']['undirected_hits'] for m in methods}
    lookup = {m['id']: m for m in methods}
    def found(method, edge):
        rank = lookup[method]['rankings']
        return edge['b'] in rank[edge['a']][:10] or edge['a'] in rank[edge['b']][:10]
    changes = {kind: [edge for edge in data['known_relations'] if
                     found('order20', edge) == improved and found('rrf', edge) != improved]
               for kind, improved in [('added', True), ('lost', False)]}
    records = [json.loads(line) for line in (run_dir / 'recording.jsonl').read_text().splitlines()]
    cost = sum(r['estimated_cost_usd'] for r in records)
    result = {'schema_version': 1, 'data_sha256': digest, 'levels': reports['cut20']['levels'],
              'methods': methods, 'default_method': 'order20', 'default_source': data['people'][0]['id'],
              'finding': f"每人前十候选：本地融合找到 {hits['rrf']} 条，J++ 前二十等级排序找到 {hits['cut20']} 条，全量等级排序找到 {hits['cut324']} 条；独立配对后的等级排序找到 {hits['pair_cut20']} 条，相对排序找到 {hits['order20']} 条。所有结果都保留，方法的价值以比较结果为准。",
              'cost_summary': f'本组真实来源实验累计录制 {len(records):,} 个后端请求，估算调用费用 ${cost:.6f}；加此前十人及合成案例约 ${cost + .04149542:.6f}。费用按录制中的 token 计价估算，非账单。网页回放不产生费用。计时与缓存口径见结果文档。',
              'estimated_cost_usd': cost, 'recorded_requests': len(records),
              'execution_source_commit': '5068a3c',
              'timing_notes': {'cut20': 'Recovered from an interrupted run using recorded answers; elapsed_seconds is not cold inference time.',
                               'cut324': 'End-to-end expansion, reusing the already judged top twenty; not a fully cold all-pairs run.',
                               'order20': 'Independent pair requests, first successful live run; includes local J++ execution and HTTP scheduling.'},
              'scope': reports['order20']['scope'],
              'partial_orders': reports['order20']['partial_orders'],
              'changes_at_10_vs_rrf': changes,
              'experiments': {key: {k: v for k, v in r.items() if k not in ('grades', 'rankings', 'metrics', 'partial_orders')} for key, r in reports.items()}}
    (public / 'results.json').write_text(json.dumps(result, ensure_ascii=False, separators=(',', ':')) + '\n')
    # mtime=0 makes the exported transcript reproducible.
    (public / 'recording.jsonl.gz').write_bytes(gzip.compress((run_dir / 'recording.jsonl').read_bytes(), mtime=0))
    (public / 'reports.json.gz').write_bytes(gzip.compress(json.dumps(reports, ensure_ascii=False).encode(), mtime=0))
    print(json.dumps({'hits_at_10': hits, 'estimated_cost_usd': cost, 'recorded_requests': len(records)}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('run_dir', type=Path)
    parser.add_argument('public', type=Path)
    args = parser.parse_args()
    build(args.run_dir, args.public)
