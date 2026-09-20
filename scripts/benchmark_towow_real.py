"""Freeze local retrieval rankings before reading any JEV outputs."""
import argparse
from datetime import datetime, timezone
from hashlib import sha256
from importlib.metadata import version
import json
from pathlib import Path
from time import perf_counter


def main():
    p=argparse.ArgumentParser()
    p.add_argument('--data',type=Path,required=True)
    p.add_argument('--model',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    a=p.parse_args()
    import torch
    from sentence_transformers import SentenceTransformer
    from jpp.towow_population import lexical_ranking
    torch.set_num_threads(4)
    people=json.loads(a.data.read_text())['people']
    start=perf_counter()
    model=SentenceTransformer(str(a.model),device='cpu',local_files_only=True)
    loaded=perf_counter()-start
    start=perf_counter()
    vectors=model.encode([x['context'] for x in people],batch_size=16,normalize_embeddings=True,show_progress_bar=False)
    encoded=perf_counter()-start
    start=perf_counter()
    scores=vectors@vectors.T
    dense={}
    for i,person in enumerate(people):
        dense[person['id']]=[people[j]['id'] for j in sorted(range(len(people)),key=lambda j:(-float(scores[i,j]),people[j]['id'])) if j!=i]
    searched=perf_counter()-start
    start=perf_counter()
    bm25={x['id']:[v for v in lexical_ranking(people,{'query':x['context']}) if v!=x['id']] for x in people}
    lexical_seconds=perf_counter()-start
    fused={}
    for person in people:
        key=person['id']
        positions=[{value:i+1 for i,value in enumerate(method[key])} for method in (dense,bm25)]
        fused[key]=sorted(dense[key],key=lambda value:(-sum(1/(60+rank[value]) for rank in positions),value))
    report={'schema':1,'created_utc':datetime.now(timezone.utc).isoformat(),
            'data_sha256':sha256(a.data.read_bytes()).hexdigest(),
            'model':'paraphrase-multilingual-MiniLM-L12-v2','model_revision':a.model.name,
            'versions':{name:version(name) for name in ('torch','sentence-transformers','transformers')},
            'timing_seconds':{'model_load':loaded,'profile_encoding':encoded,'dense_search':searched,'bm25_all':lexical_seconds},
            'api_calls':0,'api_cost_usd':0,'methods':{'minilm':dense,'bm25':bm25,'rrf':fused}}
    a.out.parent.mkdir(parents=True,exist_ok=True)
    a.out.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(report['timing_seconds']))


if __name__=='__main__':
    main()
