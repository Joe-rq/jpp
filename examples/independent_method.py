"""A third-party composition built from the README's public interface only.

All review answers in this file are explicitly synthetic fixtures.  No model
client is called, and the allocation result is an exact finite calculation.
"""

from __future__ import annotations

import json
import sys
from dataclasses import asdict, dataclass
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(PACKAGE_ROOT))
# Only present in the independent-use package layout, where the example runs
# against a frozen kernel snapshot; harmless no-op in a source checkout.
# See research/扩展/codex_composition/INDEPENDENT_USE.md.
sys.path.insert(0, str(PACKAGE_ROOT / "_kernel_snapshot"))

from foundation import jv
from jev_compose import (
    Component, Observation, Request, batch_observe, branch, component, execute,
    identity, product,
)
from jev_compose.fixtures import runtime


@dataclass(frozen=True)
class SyntheticReview:
    """Local test data, not a native/model-produced Observation."""

    answer: bool | None
    provenance: str = "synthetic_fixture_no_model_call"


@dataclass(frozen=True)
class NativeReview:
    """Read a native boolean exit while retaining its original observation."""

    observation: Observation
    provenance: str = "synthetic_fixture_native_judge_cut"

    @property
    def answer(self) -> bool | None:
        if not self.observation.resolved:
            return None
        if type(self.observation.value) is not bool:
            raise TypeError("budget review requires a boolean Observation")
        return self.observation.value


@dataclass(frozen=True)
class Candidate:
    name: str
    cost: int
    benefit: int
    reviews: tuple[SyntheticReview | NativeReview, ...]


@dataclass(frozen=True)
class Portfolio:
    candidates: tuple[Candidate, ...]
    budget: int
    minimum_support: int


@dataclass(frozen=True)
class ReviewResult:
    eligible: tuple[Candidate, ...]
    deferred: tuple[Candidate, ...]
    rejected: tuple[Candidate, ...]


@dataclass(frozen=True)
class AllocationInput:
    review: ReviewResult
    budget: int


@dataclass(frozen=True)
class Allocation:
    selected: tuple[str, ...]
    cost: int
    benefit: int
    deferred: tuple[str, ...]
    rejected: tuple[str, ...]
    eligible_not_funded: tuple[str, ...]
    budget: int


@dataclass(frozen=True)
class Report:
    allocation: Allocation
    requested_cost: int
    unresolved_review_count: int
    fixture_provenance: str


@dataclass(frozen=True)
class NativeCandidate:
    name: str
    cost: int
    benefit: int
    observations: tuple[Observation, ...]


@dataclass(frozen=True)
class NativePortfolio:
    candidates: tuple[NativeCandidate, ...]
    budget: int
    minimum_support: int


@dataclass(frozen=True)
class ObservationBackedReport:
    report: Report
    observations: tuple[Observation, ...]


def classify(portfolio: Portfolio) -> ReviewResult:
    if portfolio.budget < 0 or portfolio.minimum_support < 0:
        raise ValueError("budget and minimum_support must be nonnegative")
    names = [candidate.name for candidate in portfolio.candidates]
    if len(names) != len(set(names)):
        raise ValueError("candidate names must be unique")
    eligible, deferred, rejected = [], [], []
    for candidate in portfolio.candidates:
        if candidate.cost < 0 or candidate.benefit < 0:
            raise ValueError("cost and benefit must be nonnegative")
        lower = sum(review.answer is True for review in candidate.reviews)
        upper = lower + sum(review.answer is None for review in candidate.reviews)
        if lower >= portfolio.minimum_support:
            eligible.append(candidate)
        elif upper < portfolio.minimum_support:
            rejected.append(candidate)
        else:
            deferred.append(candidate)
    return ReviewResult(tuple(eligible), tuple(deferred), tuple(rejected))


@component("classify_synthetic_reviews", Portfolio, ReviewResult)
def review_candidates(portfolio):
    return classify(portfolio)


@component("attach_budget", tuple, AllocationInput)
def attach_budget(pair):
    review, original = pair
    return AllocationInput(review, original.budget)


@component("has_eligible_candidates", AllocationInput, bool)
def has_candidates(request):
    return bool(request.review.eligible)


def allocation_result(request, chosen):
    chosen_names = tuple(candidate.name for candidate in chosen)
    return Allocation(
        selected=chosen_names,
        cost=sum(candidate.cost for candidate in chosen),
        benefit=sum(candidate.benefit for candidate in chosen),
        deferred=tuple(candidate.name for candidate in request.review.deferred),
        rejected=tuple(candidate.name for candidate in request.review.rejected),
        eligible_not_funded=tuple(
            candidate.name for candidate in request.review.eligible
            if candidate.name not in chosen_names
        ),
        budget=request.budget,
    )


@component("empty_allocation", AllocationInput, Allocation)
def empty_allocation(request):
    return allocation_result(request, ())


def optimal_subset(request):
    """Exact dynamic programming over reachable costs for a finite portfolio."""
    # Each cost keeps its highest benefit; ties retain the earlier input choice.
    states = {0: (0, ())}
    for candidate in request.review.eligible:
        next_states = dict(states)
        for spent, (benefit, selected) in states.items():
            next_cost = spent + candidate.cost
            next_benefit = benefit + candidate.benefit
            if next_cost <= request.budget:
                previous = next_states.get(next_cost)
                if previous is None or next_benefit > previous[0]:
                    next_states[next_cost] = (next_benefit, selected + (candidate,))
        states = next_states
    best_cost = max(states, key=lambda cost: (states[cost][0], -cost))
    return allocation_result(request, states[best_cost][1])


@component("exact_budget_allocation", AllocationInput, Allocation)
def exact_allocator(request):
    return optimal_subset(request)


@component("cheapest_first_allocation", AllocationInput, Allocation)
def cheapest_first_allocator(request):
    selected = []
    spent = 0
    for candidate in sorted(request.review.eligible, key=lambda item: item.cost):
        if spent + candidate.cost <= request.budget:
            selected.append(candidate)
            spent += candidate.cost
    return allocation_result(request, tuple(selected))


def build_reviewed_allocator(allocator: Component) -> Component:
    """Accept a method and return a reusable method without changing its leaves."""
    prepare = product(review_candidates, identity(Portfolio)).then(attach_budget)
    allocate = branch(has_candidates, allocator, empty_allocation)
    return prepare.then(allocate)


@component("requested_cost", Portfolio, int)
def requested_cost(portfolio):
    return sum(candidate.cost for candidate in portfolio.candidates)


@component("unresolved_fixture_reviews", Portfolio, int)
def unresolved_reviews(portfolio):
    return sum(
        review.answer is None
        for candidate in portfolio.candidates
        for review in candidate.reviews
    )


@component("package_allocation_report", tuple, Report)
def package_report(parts):
    allocation, requested, unresolved = parts
    return Report(allocation, requested, unresolved, "synthetic_fixture_no_model_call")


def build_report(allocator: Component = exact_allocator) -> Component:
    reviewed_allocator = build_reviewed_allocator(allocator)
    return product(reviewed_allocator, requested_cost, unresolved_reviews).then(package_report)


@component("adapt_native_boolean_reviews", NativePortfolio, Portfolio)
def adapt_native_reviews(portfolio):
    candidates = tuple(
        Candidate(
            candidate.name, candidate.cost, candidate.benefit,
            tuple(NativeReview(observation) for observation in candidate.observations),
        )
        for candidate in portfolio.candidates
    )
    return Portfolio(candidates, portfolio.budget, portfolio.minimum_support)


@component("retain_native_observations", NativePortfolio, tuple)
def retain_native_observations(portfolio):
    return tuple(
        observation
        for candidate in portfolio.candidates
        for observation in candidate.observations
    )


@component("package_observation_backed_report", tuple, ObservationBackedReport)
def package_observation_backed_report(parts):
    report, observations = parts
    sourced_report = Report(
        report.allocation, report.requested_cost, report.unresolved_review_count,
        "synthetic_fixture_native_judge_cut",
    )
    return ObservationBackedReport(sourced_report, observations)


def build_native_report(allocator: Component = exact_allocator) -> Component:
    """Adapt native observations and reuse the complete original method."""
    calculate = adapt_native_reviews.then(build_report(allocator))
    return product(calculate, retain_native_observations).then(package_observation_backed_report)


def sample_portfolio() -> Portfolio:
    def answers(*values):
        return tuple(SyntheticReview(value) for value in values)

    return Portfolio(
        candidates=(
            Candidate("A", 6, 13, answers(True, True, None)),
            Candidate("B", 5, 11, answers(True, True, False)),
            Candidate("C", 5, 10, answers(True, True, True)),
            Candidate("D", 2, 100, answers(True, None, False)),
            Candidate("E", 1, 99, answers(False, False, None)),
        ),
        budget=10,
        minimum_support=2,
    )


def sample_native_portfolio() -> NativePortfolio:
    """Obtain native Observation objects using the README's zero-cost fixture."""
    original = sample_portfolio()
    question = jv.test("flag 成立吗？", calib=jv.calib("demo.flag"))
    requests = []
    for candidate in original.candidates:
        for index, review in enumerate(candidate.reviews):
            record = {"candidate": candidate.name, "review": index}
            if review.answer is None:
                record["unknown"] = True
            else:
                record["flag"] = review.answer
            requests.append(Request(
                jv.state(on=jv.mat(record)), question,
                label=f"{candidate.name}/review/{index}", tag=(candidate.name, index),
            ))
    observations = execute(batch_observe, requests, runtime()).value
    candidates = []
    cursor = 0
    for candidate in original.candidates:
        end = cursor + len(candidate.reviews)
        candidates.append(NativeCandidate(
            candidate.name, candidate.cost, candidate.benefit,
            tuple(observations[cursor:end]),
        ))
        cursor = end
    return NativePortfolio(tuple(candidates), original.budget, original.minimum_support)


def native_observation_demo() -> dict:
    """CLI-friendly native exits → unchanged allocation → nested report demo."""
    portfolio = sample_native_portfolio()
    result = execute(build_native_report(), portfolio, runtime()).value
    return {
        "report": asdict(result.report),
        "observations": [observation.to_dict() for observation in result.observations],
    }


def main():
    method = build_report()
    result = execute(method, sample_portfolio(), runtime())
    print(json.dumps(asdict(result.value), ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
