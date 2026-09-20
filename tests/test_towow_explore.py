import json

import pytest

from jpp.towow import CallBudget, fingerprint
from jpp.towow_explore import ReusingJournalClient, exact_pair_fingerprint, source_exploration_pool


def test_source_pool_preserves_rrf_prefix_and_uses_four_different_source_entries():
    rankings = {'a': ['b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v']}
    types = {key: ('academic' if key in {'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p'} else 'open_source')
             for key in ['a', *rankings['a']]}
    pools, origins = source_exploration_pool(rankings, types)
    assert pools['a'][:16] == rankings['a'][:16]
    # q was already inside the fixed prefix; dedup therefore backfills u.
    assert pools['a'][16:] == ['r', 's', 't', 'u']
    assert all(origins['a'][key] == 'different_source' for key in ['r', 's', 't'])
    assert origins['a']['u'] == 'rrf_backfill'


def test_exact_pair_replay_reuses_fused_historical_request_live(tmp_path):
    state = {'on': 'same profile', 'ctx': ['one candidate']}
    questions = {'q0': {'instructions': 'same'}, 'q1': {'instructions': 'another'}}
    answers = {'q0': {'score': 1}, 'q1': {'score': 2}}
    journal = tmp_path/'recording.jsonl'
    journal.write_text(json.dumps({'key': fingerprint(state, questions), 'state': state,
                                   'questions': questions, 'answers': answers,
                                   'estimated_cost_usd': .02})+'\n')
    client = ReusingJournalClient(journal, live=True, budget=CallBudget(limit=.10))
    result, tokens, cost = client.ask(state, {'q1': questions['q1']})
    assert result == {'q1': answers['q1']} and tokens == cost == 0
    assert client.budget.spent == .02


def test_exact_pair_ignores_executor_qid_but_requires_same_body(tmp_path):
    state, question = {'on': 'same profile'}, {'instructions': 'same'}
    journal = tmp_path/'recording.jsonl'
    journal.write_text(json.dumps({'key': fingerprint(state, {'old_qid': question}), 'state': state,
                                   'questions': {'old_qid': question}, 'answers': {'old_qid': {'score': 1}},
                                   'estimated_cost_usd': .01})+'\n')
    client = ReusingJournalClient(journal, live=False, budget=CallBudget())
    assert client.ask(state, {'new_qid': question})[0] == {'new_qid': {'score': 1}}
    assert exact_pair_fingerprint(state, question) != exact_pair_fingerprint(state, {'instructions': 'changed'})


def test_exact_full_transcript_wins_over_ambiguous_single_pair(tmp_path):
    state, question = {'on': 'same profile'}, {'instructions': 'same'}
    journal = tmp_path/'recording.jsonl'
    first = {'key': fingerprint(state, {'q0': question}), 'state': state,
             'questions': {'q0': question}, 'answers': {'q0': {'score': 1}}, 'estimated_cost_usd': .01}
    second = {**first, 'answers': {'q0': {'score': 2}}}
    journal.write_text(json.dumps(first)+'\n'+json.dumps(second)+'\n')
    client = ReusingJournalClient(journal, live=False, budget=CallBudget())
    # The exact full key is recorded, so its saved transcript remains replayable.
    assert client.ask(state, {'q0': question})[0] == {'q0': {'score': 2}}


def test_exact_pair_replay_refuses_changed_question_offline(tmp_path):
    state, question = {'on': 'same profile'}, {'instructions': 'original'}
    journal = tmp_path/'recording.jsonl'
    journal.write_text(json.dumps({'key': fingerprint(state, {'q0': question}), 'state': state,
                                   'questions': {'q0': question}, 'answers': {'q0': {'score': 1}},
                                   'estimated_cost_usd': .02})+'\n')
    client = ReusingJournalClient(journal, live=False, budget=CallBudget())
    with pytest.raises(ValueError, match='exact state/question'):
        client.ask(state, {'q0': {'instructions': 'changed'}})
