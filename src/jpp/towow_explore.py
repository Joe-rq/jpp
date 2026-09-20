"""A bounded, source-diversity candidate-entry experiment for Towow.

This is deliberately an application experiment.  It changes which twenty
candidate contexts reach the existing relative-order component; it does not
change the question, J++ kernel, or use known relation labels for selection.
"""
from __future__ import annotations

from copy import deepcopy
from datetime import datetime, timezone
from hashlib import sha256
import argparse
import json
from pathlib import Path
from time import perf_counter

from foundation import jv
from jev_compose import execute

from .towow import CallBudget, fingerprint
from .towow_real import JournalClient, evaluate, order_candidates


POOL_SIZE = 20
SOURCE_EXPLORATION_SLOTS = 4


def exact_pair_fingerprint(state, question):
    """A state/question identity; executor-assigned qids are deliberately absent."""
    return fingerprint(state, {'question': question})


def source_exploration_pool(rankings, source_types, *, size=POOL_SIZE,
                            exploration=SOURCE_EXPLORATION_SLOTS):
    """Keep the RRF prefix, then admit different-source RRF candidates.

    The source type is only a fixed candidate-entry allocation.  Neither it
    nor known relation labels is passed to the J++ judgment or used to rank it.
    """
    if not 0 <= exploration <= size:
        raise ValueError('exploration must be between zero and candidate size')
    pools, origins = {}, {}
    for source, ranking in rankings.items():
        if source not in source_types:
            raise ValueError(f'missing source_type for {source}')
        prefix = ranking[:size - exploration]
        different = [target for target in ranking
                     if target != source and source_types.get(target) != source_types[source]]
        selected, selected_origins = [], {}
        def add(target, origin):
            if target != source and target not in selected and len(selected) < size:
                selected.append(target)
                selected_origins[target] = origin
        for target in prefix:
            add(target, 'rrf_prefix')
        for target in different[:exploration]:
            add(target, 'different_source')
        for target in ranking:
            add(target, 'rrf_backfill')
        pools[source], origins[source] = selected, selected_origins
    return pools, origins


def _edge_ids(data, pools):
    selected = {source: set(targets) for source, targets in pools.items()}
    return {i for i, edge in enumerate(data['known_relations'])
            if edge['b'] in selected.get(edge['a'], set())
            or edge['a'] in selected.get(edge['b'], set())}


def _candidate_stats(data, pools):
    edges = data['known_relations']
    found = _edge_ids(data, pools)
    # This is the literal public data scope, checked rather than inferred from
    # source_type.  It is evaluation-only and never consulted by pool building.
    cross = {i for i, edge in enumerate(edges) if edge.get('label_scope') == 'curated proxy'}
    return {'candidate_limit': POOL_SIZE,
            'directed_candidate_slots': sum(map(len, pools.values())),
            'covered_edges': len(found), 'total_edges': len(edges),
            'curated_cross_source_covered': len(found & cross),
            'curated_cross_source_total': len(cross)}


def proposal(data, published):
    """Create the zero-cost candidate proposal and its label-blind selection."""
    methods = {method['id']: method for method in published['methods']}
    if 'rrf' not in methods or 'order20' not in methods:
        raise ValueError('published results must contain rrf and order20 methods')
    rrf = methods['rrf']['rankings']
    source_types = {person['id']: person['source_type'] for person in data['people']}
    baseline = {source: ranking[:POOL_SIZE] for source, ranking in rrf.items()}
    candidates, origins = source_exploration_pool(rrf, source_types)
    base_edges, explore_edges = _edge_ids(data, baseline), _edge_ids(data, candidates)
    return {'baseline_candidates': baseline, 'candidates': candidates, 'origins': origins,
            'candidate_comparison': {'baseline': _candidate_stats(data, baseline),
                                     'explore_source4': _candidate_stats(data, candidates),
                                     'added': len(explore_edges-base_edges),
                                     'lost': len(base_edges-explore_edges)},
            'published_order20': deepcopy(methods['order20'])}


class ReusingJournalClient(JournalClient):
    """Journal client that safely replays individual unchanged state/questions.

    Old physical requests may contain many questions and duplicate profile text.
    Executor qids change when fusion changes, so the reusable key is the
    canonical state plus the exact question body (not its qid).  Conflicting
    historical answers are never reused.
    """
    def __init__(self, path, *, live, budget):
        super().__init__(path, live=live, budget=budget)
        self.recorded_questions, self.ambiguous_questions = {}, set()
        self.replayed_questions = 0
        self.live_questions = 0
        for record in self.saved.values():
            for qid, question in record['questions'].items():
                key = exact_pair_fingerprint(record['state'], question)
                answer = record['answers'][qid]
                if key in self.recorded_questions and self.recorded_questions[key] != answer:
                    self.ambiguous_questions.add(key)
                self.recorded_questions[key] = deepcopy(answer)

    def ask(self, state, questions):
        full_key = fingerprint(state, questions)
        # An exact physical transcript has priority.  It preserves its response
        # grouping and qids even if a historical single-question index is
        # ambiguous because duplicate profile text appeared elsewhere.
        if full_key in self.saved:
            self.replayed_questions += len(questions)
            return deepcopy(self.saved[full_key]['answers']), 0, 0.0
        keys = {qid: exact_pair_fingerprint(state, question) for qid, question in questions.items()}
        cached = {qid: deepcopy(self.recorded_questions[key]) for qid, key in keys.items()
                  if key in self.recorded_questions and key not in self.ambiguous_questions}
        missing = {qid: question for qid, question in questions.items() if qid not in cached}
        self.replayed_questions += len(cached)
        if not missing:
            return cached, 0, 0.0
        if not self.live:
            raise ValueError('Recorded response missing for exact state/question. Use --live only for missing candidates.')
        answers, tokens, cost = super().ask(state, missing)
        self.live_questions += len(missing)
        # JournalClient appended the completed physical request.  Index it so a
        # later independent request can reuse this exact pair in the same run.
        for qid, answer in answers.items():
            key = exact_pair_fingerprint(state, missing[qid])
            self.recorded_questions[key] = deepcopy(answer)
        return {**cached, **answers}, tokens, cost


def run(data, published, client, root):
    """Run existing ReadingsVec.order on the new fixed twenty-item pools."""
    prepared = proposal(data, published)
    start = perf_counter()
    runtime = jv.Runtime(client, root=str(root), max_workers=32,
                         passes={'ledger': False} if not client.live else None)
    result = execute(order_candidates,
                     {'people': data['people'], 'candidates': prepared['candidates']}, runtime,
                     budget=jv.Budget(calls=20000, cost=4.95))
    if 'W-call-fail' in result.stats.get('warnings', []):
        raise RuntimeError('A model or replay request failed; do not evaluate the incomplete run.')
    explore = {'id': 'explore_source4',
               'label': 'RRF 前十六 + 四个不同资料来源候选 · J++ 相对排序',
               'rankings': result.value['rankings'], 'grades': result.value['grades'],
               'tiers': result.value['tiers'],
               'metrics': evaluate(data, {'explore_source4': result.value['rankings']})['explore_source4']}
    baseline = prepared['published_order20']
    baseline_metrics = baseline['metrics']['10']
    explore_metrics = explore['metrics']['10']
    baseline_top10 = {source: targets[:10] for source, targets in baseline['rankings'].items()}
    explore_top10 = {source: targets[:10] for source, targets in explore['rankings'].items()}
    baseline_edges, explore_edges = _edge_ids(data, baseline_top10), _edge_ids(data, explore_top10)
    added = [data['known_relations'][i] for i in sorted(explore_edges-baseline_edges)]
    lost = [data['known_relations'][i] for i in sorted(baseline_edges-explore_edges)]
    report = {'schema_version': 1, 'created_utc': datetime.now(timezone.utc).isoformat(),
              'methods': [baseline, explore], 'candidates': prepared['candidates'],
              'origins': prepared['origins'], 'candidate_comparison': prepared['candidate_comparison'],
              'comparison': {'baseline_method': 'order20', 'method': 'explore_source4',
                             'added': added, 'lost': lost,
                             'baseline_recall_at_10': baseline_metrics['undirected_recall'],
                             'recall_at_10': explore_metrics['undirected_recall'],
                             'delta_recall_at_10': explore_metrics['undirected_recall'] - baseline_metrics['undirected_recall']},
              'elapsed_seconds': perf_counter()-start, 'calls': len(client.records),
              'logical_requests': result.stats['calls'],
              'replayed_questions': client.replayed_questions,
              'live_questions': client.live_questions,
              'ledger_hits': result.stats['ledger_hits'],
              'replay_policy': 'Reuse only an exact canonical state plus qid and question body; conflicting historical answers are never reused.',
              'scope': 'Fixed four-slot source-diversity candidate-entry experiment. Labels are evaluation-only; source type is never supplied to the J++ judgment or used for final ordering.'}
    report['finding'] = (f"固定二十个名额下，候选池覆盖新增 {prepared['candidate_comparison']['added']} 条、失去 "
                         f"{prepared['candidate_comparison']['lost']} 条旧代理关系；相对排序前十相对已发布 order20 "
                         f"新增 {len(added)} 条、失去 {len(lost)} 条。它只检验候选入口，不能证明陌生合作成功。")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--results', type=Path, required=True,
                        help='published results.json containing rrf and order20')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--journal', type=Path, help='existing real-relation recording.jsonl; replays it when --live is absent')
    parser.add_argument('--proposal', action='store_true', help='write only the zero-cost candidate-pool proposal')
    parser.add_argument('--live', action='store_true', help='call only exact state/questions missing from journal')
    parser.add_argument('--budget-total', type=float, default=1.69770775,
                        help='cumulative journal cost ceiling, including the seeded journal')
    args = parser.parse_args()
    if args.live and not args.journal:
        parser.error('--live requires --journal so the cumulative cost is known')
    if args.budget_total <= 0:
        parser.error('--budget-total must be positive')
    data, published = json.loads(args.data.read_text()), json.loads(args.results.read_text())
    if published.get('data_sha256') != sha256(args.data.read_bytes()).hexdigest():
        parser.error('results and profile data do not match')
    args.out.mkdir(parents=True, exist_ok=True)
    prepared = proposal(data, published)
    if args.proposal or not args.journal:
        offline = {'schema_version': 1, 'mode': 'offline proposal; zero API calls',
                   'data_sha256': published['data_sha256'], **prepared,
                   'scope': 'Candidate coverage is an upper bound, not a final-ranking result.'}
        (args.out/'exploration-proposal.json').write_text(json.dumps(offline, ensure_ascii=False, indent=2)+'\n')
        print(json.dumps({'out': str(args.out/'exploration-proposal.json'), **prepared['candidate_comparison']}, ensure_ascii=False))
        return
    budget = CallBudget(limit=args.budget_total)
    client = ReusingJournalClient(args.journal, live=args.live, budget=budget)
    if args.live and client.prior_cost > args.budget_total:
        parser.error(f'journal prior cost {client.prior_cost:.8f} exceeds --budget-total')
    report = run(data, published, client, args.out/'ledger')
    total_cost = budget.spent if args.live else client.prior_cost
    new_cost = budget.spent-client.prior_cost if args.live else 0.0
    report.update({'mode': 'live JEV with exact journal replay' if args.live else 'recorded JEV with exact journal replay', 'model': client.model_id,
                   'data_sha256': published['data_sha256'],
                   'costs': {'journal_prior_cost_usd': client.prior_cost,
                             'budget_total_usd': args.budget_total if args.live else None,
                             'estimated_new_cost_usd': new_cost,
                             'estimated_total_cost_usd': total_cost},
                   'cost_summary': (f"复用日志累计估算 ${client.prior_cost:.8f}；本轮新增估算 ${new_cost:.8f}；"
                                    + (f"累计上限 ${args.budget_total:.8f}。" if args.live else '离线回放不发起 API 调用。'))})
    path = args.out/'exploration.json'
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n')
    print(json.dumps({'out': str(path), 'calls': report['calls'], 'ledger_hits': report['ledger_hits'],
                      **report['costs']}, ensure_ascii=False))


if __name__ == '__main__':
    main()
