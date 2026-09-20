"""Locate discovery bottlenecks using published results, with zero API calls.

Candidate coverage is a ceiling for a later reranker, not its measured quality.
The source-diversity probe is exploratory and uses no relation labels to select.
"""
import argparse
from collections import Counter
from hashlib import sha256
import json
from pathlib import Path


def candidate_pool(rankings, source_types, size, explore=0):
    out = {}
    for source, ranking in rankings.items():
        different = [target for target in ranking if source_types[target] != source_types[source]]
        picked = ranking[:size-explore] + different[:explore] + ranking
        out[source] = list(dict.fromkeys(picked))[:size]
    return out


def hit_indices(edges, pool):
    selected = {source: set(targets) for source, targets in pool.items()}
    return {i for i, e in enumerate(edges)
            if e['b'] in selected[e['a']] or e['a'] in selected[e['b']]}


def diagnose(data, results):
    methods = {m['id']: m for m in results['methods']}
    rankings = methods['rrf']['rankings']
    edges = data['known_relations']
    types = {p['id']: p['source_type'] for p in data['people']}
    positions = {s: {t: i+1 for i, t in enumerate(ts)} for s, ts in rankings.items()}
    cross_ids = {i for i, e in enumerate(edges) if e['label_scope'] == 'curated proxy'}
    def summarize(pool):
        found = hit_indices(edges, pool)
        return {'covered_edges': len(found), 'total_edges': len(edges),
                'curated_cross_source_covered': len(found & cross_ids),
                'curated_cross_source_total': len(cross_ids),
                'directed_candidate_slots': sum(map(len, pool.values())),
                'by_type': dict(Counter(edges[i]['type'] for i in found))}
    coverage = {str(k): summarize(candidate_pool(rankings, types, k))
                for k in (10, 20, 40, 80, 160, 324)}
    base = hit_indices(edges, candidate_pool(rankings, types, 20))
    probes = []
    for exploration in (4, 8):
        pool = candidate_pool(rankings, types, 20, exploration)
        found = hit_indices(edges, pool)
        probes.append({'candidate_limit': 20, 'different_source_slots': exploration,
                       **summarize(pool), 'added_vs_rrf20': len(found-base),
                       'lost_vs_rrf20': len(base-found)})
    final_pool = {s: ranking[:10] for s, ranking in methods['order20']['rankings'].items()}
    final = hit_indices(edges, final_pool)
    assert final <= base
    return {'scope': 'Offline development diagnosis; no training, new model calls, or final-ranking claims for candidate probes.',
            'profiles': len(data['people']),
            'distinct_profile_texts': len({p['context'] for p in data['people']}),
            'candidate_coverage_ceiling': coverage,
            'miss_decomposition': {'known_edges': len(edges), 'found_at_10': len(final),
                                   'outside_both_top20_pools': len(edges)-len(base),
                                   'inside_pool_but_not_found_at_10': len(base-final)},
            'curated_cross_source_best_endpoint_ranks': sorted(
                min(positions[edges[i]['a']][edges[i]['b']], positions[edges[i]['b']][edges[i]['a']])
                for i in cross_ids),
            'same_budget_source_diversity_probes': probes,
            'api_calls': 0, 'api_cost_usd': 0}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--case', type=Path, default=Path('docs/demos/towow/real'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    data_path, result_path = args.case/'data.json', args.case/'results.json'
    results = json.loads(result_path.read_text())
    assert results['data_sha256'] == sha256(data_path.read_bytes()).hexdigest()
    report = diagnose(json.loads(data_path.read_text()), results)
    report['data_sha256'] = results['data_sha256']
    report['results_sha256'] = sha256(result_path.read_bytes()).hexdigest()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
    print(json.dumps(report['miss_decomposition']))
    print(json.dumps(report['same_budget_source_diversity_probes']))


if __name__ == '__main__':
    main()
