"""Independent author tests; no existing examples or implementation imports."""

import itertools
import sys
import unittest
from dataclasses import replace
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PACKAGE_ROOT))
sys.path.insert(0, str(PACKAGE_ROOT / "src"))

from foundation import jv
from jev_compose import component, execute
from jev_compose.fixtures import runtime
from independent_method import (
    Allocation, AllocationInput, Candidate, Portfolio, Report, SyntheticReview,
    build_report, build_reviewed_allocator, cheapest_first_allocator,
    exact_allocator, sample_portfolio,
)


class IndependentMethodTests(unittest.TestCase):
    def run_component(self, method, value):
        return execute(method, value, runtime()).value

    def test_exact_plan_preserves_local_unknown_and_determined_answers(self):
        report = self.run_component(build_report(), sample_portfolio())
        self.assertIsInstance(report, Report)
        self.assertEqual(report.allocation.selected, ("B", "C"))
        self.assertEqual((report.allocation.cost, report.allocation.benefit), (10, 21))
        self.assertEqual(report.allocation.deferred, ("D",))
        self.assertEqual(report.allocation.rejected, ("E",))
        self.assertEqual(report.allocation.eligible_not_funded, ("A",))
        self.assertEqual(report.requested_cost, 19)
        self.assertEqual(report.unresolved_review_count, 3)
        self.assertEqual(report.fixture_provenance, "synthetic_fixture_no_model_call")

    def test_same_method_accepts_another_allocation_method(self):
        portfolio = replace(sample_portfolio(), budget=6)
        exact = self.run_component(build_report(exact_allocator), portfolio)
        cheap = self.run_component(build_report(cheapest_first_allocator), portfolio)
        self.assertEqual(exact.allocation.selected, ("A",))
        self.assertEqual(exact.allocation.benefit, 13)
        self.assertEqual(cheap.allocation.selected, ("B",))
        self.assertEqual(cheap.allocation.benefit, 11)
        self.assertEqual(exact.allocation.deferred, cheap.allocation.deferred)

    def test_exact_computation_against_independent_exhaustive_oracle(self):
        candidates = sample_portfolio().candidates[:3]
        for budget in range(18):
            portfolio = Portfolio(candidates, budget, 2)
            plan = self.run_component(build_reviewed_allocator(exact_allocator), portfolio)
            feasible = []
            for count in range(len(candidates) + 1):
                for subset in itertools.combinations(candidates, count):
                    cost = sum(candidate.cost for candidate in subset)
                    if cost <= budget:
                        feasible.append((sum(candidate.benefit for candidate in subset), -cost))
            self.assertEqual((plan.benefit, -plan.cost), max(feasible), budget)

    def test_empty_branch_never_executes_supplied_allocator(self):
        @component("must_not_run", AllocationInput, Allocation)
        def must_not_run(request):
            raise AssertionError("empty portfolio should select the other branch")

        sample = sample_portfolio()
        portfolio = replace(sample, candidates=sample.candidates[3:])
        plan = self.run_component(build_reviewed_allocator(must_not_run), portfolio)
        self.assertEqual(plan.selected, ())
        self.assertEqual(plan.deferred, ("D",))
        self.assertEqual(plan.rejected, ("E",))

    def test_returned_method_can_run_as_a_nested_component_call(self):
        returned_method = build_report()

        @component("outer_report_consumer", Portfolio, int)
        def outer(portfolio):
            return returned_method(portfolio).allocation.benefit

        self.assertEqual(self.run_component(outer, sample_portfolio()), 21)


if __name__ == "__main__":
    unittest.main()
