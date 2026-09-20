import json

from foundation import jv
from jev_compose import execute
from jpp.towow_real import REAL_METHOD, evaluate, order_candidates, JournalClient
from jpp.towow import CallBudget, fingerprint


def test_real_relation_labels_never_enter_judgment_inputs(tmp_path):
    data={'people':[{'id':'a','context':'Builds audio instruments'},
                    {'id':'b','context':'Creates interactive stage visuals'},
                    {'id':'c','context':'Repairs bicycles'}],
          'candidates':{'a':['b','c'],'b':['a','c'],'c':['a','b']},
          'known_relations':[{'a':'a','b':'b','type':'SECRET_LABEL'}]}
    client=jv.FakeClient(rule=lambda s,qid,q:{'type':'score','score':2.0,
                          'probabilities':{'0':.01,'1':.01,'2':.97,'3':.01},'confidence':.97})
    rt=jv.Runtime(client,root=str(tmp_path))
    result=execute(REAL_METHOD,data,rt)
    assert result.stats['calls']==3
    assert client.questions_asked==6
    assert all(g['level']==2 for row in result.value['grades'].values() for g in row.values())
    assert 'SECRET_LABEL' not in json.dumps(client.log)
    rt=jv.Runtime(client,root=str(tmp_path))
    repeat=execute(REAL_METHOD,data,rt)
    assert repeat.stats['calls']==0
    assert repeat.stats['ledger_hits']==6


def test_undirected_and_directed_recall_have_explicit_denominators():
    data={'people':[{'id':x} for x in 'abc'],
          'known_relations':[{'a':'a','b':'b','type':'coauthor','label_scope':'source-backed proxy'}]}
    report=evaluate(data,{'example':{'a':['c','b'],'b':['a','c'],'c':['b','a']}})
    metric=report['example']['1']
    assert metric['undirected_hits']==metric['undirected_total']==1
    assert metric['directed_hits']==1 and metric['directed_total']==2
    assert metric['directed_nominations']==3
    assert metric['source_backed_total']==1


def test_ambiguous_scores_remain_unresolved(tmp_path):
    client=jv.FakeClient(rule=lambda s,qid,q:{'type':'score','score':1.0,
                          'probabilities':{'0':.25,'1':.25,'2':.25,'3':.25},'confidence':.25})
    rt=jv.Runtime(client,root=str(tmp_path))
    payload={'people':[{'id':'a','context':'one'},{'id':'b','context':'two'}],
             'candidates':{'a':['b']}}
    result=execute(REAL_METHOD,payload,rt)
    assert result.value['grades']['a']['b']['level'] is None


def test_relative_order_does_not_require_accepting_uncertain_relations(tmp_path):
    def rule(text,qid,q):
        level=3 if 'STRONG' in text else 0
        return {'type':'score','score':float(level),'probabilities':{str(i):.4 if i==level else .2 for i in range(4)},'confidence':.4}
    rt=jv.Runtime(jv.FakeClient(rule=rule),root=str(tmp_path))
    payload={'people':[{'id':'a','context':'anchor'},{'id':'b','context':'STRONG'},{'id':'c','context':'WEAK'}],
             'candidates':{'a':['c','b']}}
    result=execute(order_candidates,payload,rt)
    assert result.value['rankings']['a']==['b','c']
    assert all(g['level'] is None for g in result.value['grades']['a'].values())


def test_replay_handles_a_cached_subset_without_changing_questions(tmp_path):
    state={'on':'same exact profile'}
    questions={'q0':{'type':'score','instructions':'first'},'q1':{'type':'score','instructions':'second'}}
    answers={'q0':{'score':0},'q1':{'score':2}}
    path=tmp_path/'recording.jsonl'
    path.write_text(json.dumps({'key':fingerprint(state,questions),'state':state,'questions':questions,
                                'answers':answers,'estimated_cost_usd':.01})+'\n')
    client=JournalClient(path,live=False,budget=CallBudget())
    result,tokens,cost=client.ask(state,{'q1':questions['q1']})
    assert result=={'q1':answers['q1']} and tokens==cost==0
    import pytest
    with pytest.raises(ValueError,match='Recorded response missing'):
        client.ask(state,{'q1':{'type':'score','instructions':'changed'}})
