from jev_compose import execute
from jev_compose.fixtures import runtime
from jev_compose.partial_example import CHECKED, requests
from priority_resume import method


def test_independent_strategy_keeps_usable_result_and_changes_requirement():
    CHECKED.clear()
    result = execute(method, requests(), runtime())
    before, (after_one, completed) = result.value
    assert before.value.cheapest["cost"] == after_one.value.cheapest["cost"] == 9
    assert [o.request.tag for o in before.pending] == ["C", "D"]
    assert [o.request.tag for o in after_one.pending] == ["C"]
    assert completed.value.cheapest["cost"] == 2 and not completed.pending
    assert CHECKED == ["A", "B", "C"]
    assert result.stats["calls_per_layer"] == [4, 1, 1]
    assert result.stats["effect_requests"]["transform"] == 2
