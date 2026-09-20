"""Candidate retrieval followed by composable, recorded context judgments.

Inputs are supplied explicitly. Known relations are read only by evaluate().
The model never receives relation labels, names, account handles or source URLs
from the prepared dataset. Shared professional content remains in profiles.
"""
from copy import deepcopy
from datetime import datetime, timezone
from hashlib import sha256
import json
from pathlib import Path
from threading import Lock
from time import perf_counter

from foundation import jv
from jev_compose import component, execute
from .towow import CallBudget, RecordedClient, fingerprint

LEVELS = ('没有具体关联', '只有泛泛的领域相似', '有具体共同方向或可用的互补能力', '有非常明确的共同工作内容或直接互补')


@component('judge candidate contexts', dict, dict, effects=('judge',))
def judge_candidates(payload):
    people={p['id']:p for p in payload['people']}
    pending=[]
    for source, candidates in payload['candidates'].items():
        if not candidates:
            continue
        contexts={f'c{i:02d}':people[key]['context'] for i,key in enumerate(candidates)}
        state=jv.state(jv.mat({'source':people[source]['context']}),ctx=[jv.mat({'candidates':contexts})])
        questions=[jv.measure(
            f'只比较 on.source 与 ctx[0].candidates.c{i:02d}：双方职业、研究、技能或资源存在多强的具体关联，值得进一步交流或协作？'
            '依据这两份材料中的实际内容，区分泛泛相似与具体共同方向、互补能力；不得编造经历、意愿或已发生的合作。'
            '同批其他候选是独立问题，不作为这个候选的证据。',
            scale=LEVELS,calib=jv.calib('towow.real.context-relation')) for i in range(len(candidates))]
        pending.append((source,candidates,jv.judge(state,*questions)))
    grades={}
    for source,candidates,readings in pending:
        grades[source]={}
        for target,reading in zip(candidates,readings):
            result=jv.cut(reading)
            if isinstance(result,jv.Unsure) and result.cause=='cold':
                result=jv.handle(result)
            if isinstance(result,jv.At):
                grades[source][target]={'level':result.level,'provisional':result.provisional}
            else:
                grades[source][target]={'level':None,'provisional':True,'status':result.kind if result is not None else 'unresolved'}
                if result is not None:
                    jv.consume([result],unsure=jv.drop)
    return {'grades':grades,'candidates':payload['candidates']}


def grade_key(level):
    # Preserve unresolved separately, between weak evidence and no evidence.
    return -(.5 if level is None else level)


@component('rank judged candidates', dict, dict)
def rank_judged(payload):
    rankings={source:sorted(candidates,key=lambda target:grade_key(payload['grades'][source][target]['level']))
              for source,candidates in payload['candidates'].items()}
    return {**payload,'rankings':rankings}


REAL_METHOD=judge_candidates.then(rank_judged,name='towow_real_relations')


@component('relative ordering of candidate contexts', dict, dict, effects=('judge',))
def order_candidates(payload):
    """Rank through the public partial-order operation; retain unsure exits.

    Unlike an accepted At(level), a ranking does not claim a certain relation.
    Each object receives the same question with the same fixed ordinal scale.
    """
    people={p['id']:p for p in payload['people']}
    question=jv.measure(
        '只比较 on.source 与 ctx[0].candidate：双方职业、研究、技能或资源存在多强的具体关联，值得进一步交流或协作？'
        '依据这两份材料中的实际内容，区分泛泛相似与具体共同方向、互补能力；不得编造经历、意愿或已发生的合作。',
        scale=LEVELS,calib=jv.calib('towow.real.relative-order'))
    pending=[]
    for source,candidates in payload['candidates'].items():
        on=jv.mat({'source':people[source]['context']})
        states=[jv.state(on,ctx=[jv.mat({'candidate':people[target]['context']})]) for target in candidates]
        pending.append((source,candidates,jv.judge(states,question)))
    rankings,grades,tiers={},{},{}
    for source,candidates,readings in pending:
        groups=readings.order()
        tiers[source]=[[candidates[i] for i in sorted(group)] for group in groups]
        rankings[source]=[target for group in tiers[source] for target in group]
        grades[source]={}
        for target,row in zip(candidates,readings):
            result=jv.cut(row[0])
            if isinstance(result,jv.Unsure) and result.cause=='cold':
                result=jv.handle(result)
            if isinstance(result,jv.At):
                grades[source][target]={'level':result.level,'provisional':result.provisional}
            else:
                grades[source][target]={'level':None,'provisional':True,'status':result.kind if result is not None else 'unresolved'}
                if result is not None:
                    jv.consume([result],unsure=jv.drop)
    return {'grades':grades,'rankings':rankings,'tiers':tiers}


def evaluate(data,rankings):
    """Fixed-K retrieval of proxy positives; absent labels are not negatives."""
    relations=data['known_relations']
    neighborhoods={p['id']:set() for p in data['people']}
    for row in relations:
        neighborhoods[row['a']].add(row['b'])
        neighborhoods[row['b']].add(row['a'])
    metrics={}
    for name,ranking in rankings.items():
        methods={}
        for k in (1,5,10,20):
            selected={source:set(targets[:k]) for source,targets in ranking.items()}
            found=lambda row: row['b'] in selected.get(row['a'],set()) or row['a'] in selected.get(row['b'],set())
            by_type={}
            for kind in sorted({r['type'] for r in relations}):
                rows=[r for r in relations if r['type']==kind]
                hits=sum(found(r) for r in rows)
                by_type[kind]={'hits':hits,'total':len(rows),'recall':hits/len(rows)}
            hits=sum(found(r) for r in relations)
            directed=sum(len(selected.get(source,set())&known) for source,known in neighborhoods.items())
            denominator=sum(map(len,neighborhoods.values()))
            source_backed=[r for r in relations if r['label_scope']=='source-backed proxy']
            methods[str(k)]={'undirected_hits':hits,'undirected_total':len(relations),
                             'undirected_recall':hits/len(relations) if relations else None,
                             'directed_hits':directed,'directed_total':denominator,
                             'directed_recall':directed/denominator if denominator else None,
                             'directed_nominations':sum(map(len,selected.values())),
                             'source_backed_hits':sum(found(r) for r in source_backed),
                             'source_backed_total':len(source_backed),'by_type':by_type}
        metrics[name]=methods
    return metrics


class JournalClient(RecordedClient):
    """Append each completed paid response; resume/replay without resending it."""
    def __init__(self,path,*,live,budget):
        records=[json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
        super().__init__(live=live,recording={'requests':records},budget=budget)
        self.journal_path=path
        self.journal_lock=Lock()
        self.prior_cost=sum(r['estimated_cost_usd'] for r in records)
        if live:
            self.budget.spent=self.prior_cost

    def ask(self,state,questions):
        key=fingerprint(state,questions)
        if self.live and key in self.saved:
            return deepcopy(self.saved[key]['answers']),0,0.0
        response=super().ask(state,questions)
        if self.live:
            with self.lock:
                record=next(r for r in reversed(self.records) if r['key']==key)
            with self.journal_lock:
                with self.journal_path.open('a',encoding='utf-8') as out:
                    out.write(json.dumps(record,ensure_ascii=False,separators=(',',':'))+'\n')
                self.saved[key]=record
        return response


def run(data,retrieval,client,root,*,limit=20,batch_size=20,mode='cut'):
    people=data['people']
    ranking=retrieval['methods']['rrf']
    total_limit=min(limit,len(people)-1)
    grades={p['id']:{} for p in people}
    batches=[]
    ordered={}
    partial_orders={}
    if mode=='order' and total_limit>batch_size:
        raise ValueError('Relative-order experiment uses one common candidate set, limit must be <= batch_size')
    start=perf_counter()
    for offset in range(0,total_limit,batch_size):
        candidates={p['id']:ranking[p['id']][offset:min(offset+batch_size,total_limit)] for p in people}
        rt=jv.Runtime(client,root=str(root),max_workers=32 if mode=='order' else 8)
        result=execute(order_candidates if mode=='order' else REAL_METHOD,{'people':people,'candidates':candidates},rt,
                       budget=jv.Budget(calls=20000,cost=4.95))
        for source,values in result.value['grades'].items():
            grades[source].update(values)
        if mode=='order':
            ordered.update(result.value['rankings'])
            partial_orders.update(result.value['tiers'])
        batches.append(result.stats)
        print(json.dumps({'through_candidates':min(offset+batch_size,total_limit),
                          'calls':result.stats['calls'],'ledger_hits':result.stats['ledger_hits'],
                          'total_recorded_cost':client.budget.spent}),flush=True)
    jpp={source:sorted(targets[:total_limit],key=lambda target:grade_key(grades[source][target]['level']))
         for source,targets in ranking.items()}
    extra={}
    if mode=='order':
        extra['jpp_acceptance']=jpp
        jpp=ordered
    return {'created_utc':datetime.now(timezone.utc).isoformat(),'candidate_limit':total_limit,
            'grades':grades,'partial_orders':partial_orders,'ranking_mode':mode,'rankings':{**retrieval['methods'],**extra,'jpp':jpp},
            'metrics':evaluate(data,{**retrieval['methods'],**extra,'jpp':jpp}),
            'batches':batches,'elapsed_seconds':perf_counter()-start,
            'calls':sum(b['calls'] for b in batches),'ledger_hits':sum(b['ledger_hits'] for b in batches),
            'levels':LEVELS,'scope':'Recovery of existing proxy relations. Direct relationship signals remain in professional descriptions. Uncalibrated semantic levels; no precision or unseen-cooperation claim.'}


def main():
    import argparse
    p=argparse.ArgumentParser()
    p.add_argument('--data',type=Path,required=True)
    p.add_argument('--retrieval',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    p.add_argument('--live',action='store_true')
    p.add_argument('--limit',type=int,default=20)
    p.add_argument('--budget',type=float,default=4.95)
    p.add_argument('--mode',choices=('cut','order'),default='cut')
    a=p.parse_args()
    if a.limit<1 or a.budget<=0:
        p.error('positive limit and budget required')
    data=json.loads(a.data.read_text())
    retrieval=json.loads(a.retrieval.read_text())
    if retrieval['data_sha256']!=sha256(a.data.read_bytes()).hexdigest():
        p.error('retrieval and profile data do not match')
    a.out.mkdir(parents=True,exist_ok=True)
    budget=CallBudget(limit=a.budget)
    client=JournalClient(a.out/'recording.jsonl',live=a.live,budget=budget)
    report=run(data,retrieval,client,a.out/'ledger',limit=a.limit,mode=a.mode)
    report.update({'mode':'live JEV' if a.live else 'recorded JEV','data_sha256':retrieval['data_sha256'],
                   'model':client.model_id,'estimated_total_cost_usd':budget.spent if a.live else client.prior_cost,
                   'estimated_new_cost_usd':budget.spent-client.prior_cost if a.live else 0})
    suffix=f'{a.mode}-' if a.mode!='cut' else ''
    (a.out/f'report-{suffix}{a.limit}.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({'calls':report['calls'],'elapsed_seconds':report['elapsed_seconds'],
                      'estimated_total_cost_usd':report['estimated_total_cost_usd'],
                      'recall_at_10':{key:v['10']['undirected_recall'] for key,v in report['metrics'].items()}}))


if __name__=='__main__':
    main()
