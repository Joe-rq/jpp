"""Rerun the historical MiniLM family on the exact published population.

Optional environment: sentence-transformers. All inference is local/offline.
Run with --model pointing at a complete cached MiniLM snapshot directory.
Both flat and field-bundled variants are specified before this run's results.
"""
import argparse
from datetime import datetime, timezone
from hashlib import sha256
from importlib.metadata import version
import json
from pathlib import Path
from time import perf_counter


ROOT = Path(__file__).resolve().parents[1]


def profile_chunks(context):
    chunks = []
    for text in context.split('\n\n'):
        profile = json.loads(text)
        for key, value in profile.items():
            values = value if isinstance(value, list) else [value]
            chunks.extend(f'{key}: {item}' for item in values if str(item).strip())
    return chunks


def summarize(rows):
    labeled = [r for r in rows if r['expected_count']]
    return {
        'hits': sum(r['hit_count'] for r in rows),
        'known_positive_pairs': sum(r['expected_count'] for r in rows),
        'micro_known_recall_at_10': sum(r['hit_count'] for r in rows) / sum(r['expected_count'] for r in rows),
        'macro_known_recall_at_10': sum(r['hit_count']/r['expected_count'] for r in labeled)/len(labeled),
        'evaluable_intents': len(labeled),
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--out', type=Path, default=ROOT/'docs/demos/towow/population/embedding-report.json')
    args = parser.parse_args()
    import numpy as np
    import torch
    from sentence_transformers import SentenceTransformer
    torch.set_num_threads(4)
    torch.manual_seed(0)
    source = ROOT/'src/jpp/data/towow-population.json'
    data = json.loads(source.read_text())
    start = perf_counter()
    model = SentenceTransformer(str(args.model), device='cpu', local_files_only=True)
    loading = perf_counter()-start
    encode = lambda texts: model.encode(texts, batch_size=16, normalize_embeddings=True, show_progress_bar=False)
    # Exactly the same query and additional context available to J++/BM25.
    queries = [q['query']+' '+(q.get('context') or '') for q in data['intents']]
    start = perf_counter()
    query_vectors = encode(queries)
    query_seconds = perf_counter()-start
    methods = {}
    for strategy in ('flat', 'field_bundle'):
        start = perf_counter()
        if strategy == 'flat':
            vectors = encode([p['context'] for p in data['people']])
        else:
            chunks = [profile_chunks(p['context']) for p in data['people']]
            encoded = encode([chunk for profile in chunks for chunk in profile])
            vectors, cursor = [], 0
            for profile in chunks:
                vector = encoded[cursor:cursor+len(profile)].mean(axis=0)
                cursor += len(profile)
                vectors.append(vector / np.linalg.norm(vector))
            vectors = np.array(vectors)
        index_seconds = perf_counter()-start
        start = perf_counter()
        scores = query_vectors @ vectors.T
        search_seconds = perf_counter()-start
        rows = []
        for q, values in zip(data['intents'], scores):
            order = sorted(range(len(values)), key=lambda i: (-float(values[i]),data['people'][i]['id']))
            ranked = [{'person':data['people'][i]['id'],'score':float(values[i])} for i in order]
            top10 = [r['person'] for r in ranked[:10]]
            hits = sorted(set(top10)&set(q['present_expected']))
            rows.append({'intent_id':q['id'],'query':q['query'],'level':q['level'],
                         'top10':top10,'hits':hits,'hit_count':len(hits),
                         'expected_count':len(q['present_expected']),'ranking':ranked})
        methods[strategy] = {'rows':rows,'summary':summarize(rows),
                             'timing_seconds':{'profile_index':index_seconds,'query_encoding':query_seconds,
                                               'all_queries_search':search_seconds}}
        print(strategy, json.dumps(methods[strategy]['summary']), flush=True)
    report = {'created_utc':datetime.now(timezone.utc).isoformat(),
              'model':'sentence-transformers/paraphrase-multilingual-MiniLM-L12-v2',
              'model_revision':args.model.name,'backend':'PyTorch CPU, 4 threads',
              'versions':{name:version(name) for name in ('sentence-transformers','torch','transformers','numpy')},
              'max_seq_length':model.max_seq_length,'model_load_seconds':loading,
              'dataset_sha256':sha256(source.read_bytes()).hexdigest(),
              'subjects':len(data['people']),'intents':len(queries),'api_calls':0,'api_cost_usd':0,
              'scope':'Same 216 merged profiles and 20 intents including context. Historical MiniLM cosine/bundling family rerun; not the original 447-person benchmark. Field bundling covers every current profile field.',
              'methods':methods}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')


if __name__ == '__main__':
    main()
