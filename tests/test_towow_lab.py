"""Intervene on actual components using published model answers, not scripted labels."""
import pytest

from jpp.towow_lab import LabSession, compare


def test_recorded_component_interventions_and_reuse(monkeypatch):
    from foundation import jv
    def forbidden(*args, **kwargs):
        raise AssertionError("The experiment must not create a live model client")
    monkeypatch.setattr(jv, "JevClient", forbidden)
    report = compare()
    rows = {r["id"]: r for r in report["cases"]}
    assert rows["one_round"]["outcome"]["support"] == ["mei"]
    assert rows["referral"]["outcome"]["support"] == ["mei", "zhou"]
    assert not rows["referral"]["outcome"]["extensions"]
    assert rows["full"]["outcome"]["extensions"] == ["an", "he", "tang"]
    assert rows["no_contact"]["outcome"] == rows["one_round"]["outcome"]
    assert rows["available"]["outcome"]["discuss"] == ["an", "he", "tang"]
    assert "qiao" in rows["available"]["outcome"]["unresolved"]
    assert [rows[k]["recording_lookups"] for k in ("one_round", "referral", "full")] == [8, 9, 16]
    reuse = report["reuse"]
    assert reuse["repeat"]["outcome"] == reuse["first"]["outcome"]
    assert reuse["repeat"]["recording_lookups"] == 0
    assert reuse["updated"]["recording_lookups"] == 8
    assert reuse["updated"]["run"]["stats"]["ledger_hits"] == 16
    assert reuse["disabled"]["recording_lookups"] == 16
    assert reuse["disabled"]["outcome"] == reuse["updated"]["outcome"]


def test_switches_do_not_fabricate_a_route():
    session = LabSession()
    try:
        result = session.run(referral=False, composition=True, available=True)
        assert result["outcome"]["support"] == ["mei"]
        assert not result["outcome"]["combinations"]
        assert not result["outcome"]["extensions"]
        assert all(r["state"]["ctx"][0]["person"] != "zhou" for r in result["requests"])
        with pytest.raises(ValueError):
            session.run(contact="false")
    finally:
        session.close()
