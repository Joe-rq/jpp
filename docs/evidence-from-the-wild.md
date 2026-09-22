# What we learned by reading everyone else's Jev code

[中文](evidence-from-the-wild.zh-CN.md) · [Why J++](why-jpp.md) · [Status](status.md)

Before writing more of J++, we went and read what people actually build with Jev.

We read the official documentation and all 22 cookbooks, both official SDKs, 35 named
patterns from community pattern repositories, roughly 150 real repositories found by
reverse-lookup on SDK dependencies, all 194 entries in the jevable.com gallery, and
26 write-ups where developers describe what they shipped. We analysed 57 production
programs line by line against a fixed template.

We expected to find that people were using Jev in ways we had not thought of.
That happened a little. What we mostly found was something else: **everyone writes
the same handful of mechanisms by hand, independently, in different languages,
and no two of them agree on the shape.**

This document is that evidence, and what we think it means for language design.
Where our own implementation falls short of what we describe, we say so.

## Nobody caches a judgment

A Jev judgment is a pure function of the material and the question. Same input,
same answer. Caching it is free money.

Across 57 production programs, **one** caches the judgment itself. The other 56 cache
tool outputs, ontology trees, cache keys, semantic fingerprints — anything but the
judgment.

The one who did it wrote about 15 lines: normalise the volatile fields (IPs, block IDs)
to placeholders, SHA-256 the result, keep it in a five-minute map. That turned
2,500 calls into 88, and 1.35 million tokens into 48,000. Roughly 96% of calls never
happened.

So why don't the other 56 do it? Not because it isn't worth it — the number above is
enormous. Because each of them would have to work out, from scratch, which fields to
normalise, how to build the key, and how long to hold it. Most people decide it isn't
worth the thinking, and they are individually right.

**This is what a language is for.** A judgment's identity — which material, which
question, which calibration line — is something the runtime already knows. J++ records
it in a ledger, and we verified replay on real traffic: the same program run three times
against the live model produces byte-identical ledger keys.

**Where we are short:** the ledger makes replay work within one program. A store that
survives across programs and sessions is designed (`12` §2.10, §5) and not built.
We are not claiming this one yet.

## Everyone invents their own "I don't know"

This is the one mechanism people do write. Of 26 GitHub samples, 18 handle low
confidence explicitly. It is also the mechanism nobody agrees on: among the samples
with real code, **we found eight different shapes and no two the same** — single
threshold, three-state interval, minimum across signals, returning a set instead of a
value, an exception that triggers a framework fallback, an explicit "none of these"
option in the candidate list, a two-threshold band with a global safety valve.

Thresholds ranged from 0.30 to 0.95.

One community playbook actually contains a well-written general routing function:

```python
def safe_route(probability, automatic, review="human_review"):
    if probability >= 0.70: return automatic
    if probability <= 0.30: return "no_action"
    return review
```

Nothing in that repository calls it. Four scenarios in the same repository each write
their own if-chain instead.

That is how libraries fail. A good abstraction exists, sits there, and nothing obliges
you to use it, so every call site rewrites it.

**In J++ the obligation is in the type system.** A judgment does not give you a boolean.
It gives you an exit with three states, and the unsure state is a value you must consume:
route it to a handler, escalate it, explicitly drop it, or return it to your caller.
A program that silently discards it does not compile. A catch-all arm cannot absorb it —
you have to write the unsure arm yourself.

This part is built and has been running since the first kernel package.

## The most common line of code in the ecosystem is one we made illegal

We saw this shape in four independent projects:

```javascript
if (result.noul("is_urgent").isYes(0.7)) { escalate(); }
```

The threshold lives in the calling code. When the authors explain why, they say the
same thing: so the threshold can be tuned and A/B tested independently of the model.

**That reason is exactly right.** A decision boundary should evolve on its own schedule,
measured against data, without touching program logic.

**And hardcoding it in the caller is the one way to guarantee that cannot happen.**
A number in source is changed by editing source and shipping a release. It is not
independent of the code; it *is* the code.

J++ forbids writing a line in a program at all. Lines come only from calibration
records, which are produced by measuring against a labelled set and carry their own
provenance, sample count, and certificate. The program says "cut this reading";
which line it cuts against is not the program's business.

We think this is our most restrictive rule and the one that will cause the most friction.
We also think the ecosystem's most common idiom is a workaround for its absence.

**Where we are short:** the loop that produces a line from a program's own runs is not
closed. Runtime evidence carries no ground truth yet, so calibration records still have
to be supplied from outside. This is the next thing we are building.

## The platform says batch; the tooling doesn't help you

The official parallel-questions cookbook measures it: putting every question into one
call is **12.2x cheaper and 10.0x faster** than asking them one at a time. The request
format has no limit on how many questions you attach.

Now look at what the SDK gives you. From the official Python SDK reference:

```python
RetryPolicy(max_retries=2, backoff_initial=0.5, backoff_max=5.0,
            backoff_jitter=0.25, http_statuses={408, 429, *range(500, 600)},
            respect_retry_after=True, timeout=30.0)
```

That is genuinely good, and it is the *only* seam the SDK owns. The async client
reference documents no concurrency limit, no semaphore, no throttle. The usage guide
mentions neither batching nor caching. Every call to `system_one()` is one HTTP POST.

So people build it themselves, and we watched them: `ThreadPoolExecutor(max_workers=8)`
in one project, `asyncio.Semaphore(16)` in another, deliberately serial batching in a
third — that last one commented as a hedge against rate limits. Three projects
independently tuned their concurrency down out of fear.

**In J++, questions on the same material are supposed to fuse into one call by
construction.** You write the natural thing and the compiler puts them in one layer.

**Where we are short, and this one is embarrassing:** while writing this document we
measured our own implementation and found it fragile. Fourteen checks on one document:

```
map(questions, check)              → 1 call
map(questions, fn(q) { check(q) })  → 14 calls
```

Identical results. Zero warnings. The difference is an eta-expansion — wrapping a
function in a lambda that immediately calls it, a transformation that is the identity
in any functional language.

The cause is structural, and it is the useful part of this finding. Our kernel design
says the filter operator takes *a set and a question*. Our implementation made it take
*a list and a closure*, like an ordinary list operation with the same name. Once there
is a closure, there is something to wrap, and the fusion pass stops recognising the
shape. With the designed operator there is no closure and fusion is not a pass at all —
it is what the operator means.

We are fixing the operator, not the pass.

## Four programs loop; all four use three brakes

Multi-round programs are rare — Jev is a single judgment, so most uses are one-shot.
We found four real ones: a Pokémon battle agent, a computer-use agent, an end-to-end
test driver, and a bar-by-bar music generator.

All four stop on three independent conditions: a hard step limit, a no-progress
detector, and the model's own sense of completion. **Not one of them lets the model's
judgment terminate the loop on its own.**

That is a good instinct and they all had it separately. A step limit catches runaway
loops but not a program that takes legal steps in a circle. A no-progress detector
catches circles but not a model that thinks it isn't finished. And a judgment can be
confidently wrong, so it cannot be the only brake.

The cleanest of the four is worth describing. An outer `while (!battle.ended)` drives a
deterministic battle engine. Each turn, the structured state — HP, types, moves, weather —
is rendered into a question, one multiple-choice call picks a move, the engine applies it
and advances. Termination comes from the engine deciding the battle and from a hard
decision cap. The author's README notes they do not script around low-confidence answers.

**That is the architecture we think is right**: a deterministic engine drives the world;
judgment picks one move per step.

**Where we are short:** our bounded loop takes one integer bound. The no-progress line is
half-built and has no surface syntax. The completion judgment cannot drive it at all,
because the score primitive's level is not wired to loop progress. One brake of three.

## Nobody does second order, and that is the interesting part

We looked specifically for programs that use Jev's output to decide what to ask Jev next.

Across 26 community write-ups, **we found none.**

The official flagship cookbook comes closest and is worth being precise about: each round,
*a separate general-purpose reasoning model* proposes new questions, Jev answers them
across the rows, a gradient-boosted model refits, and code keeps the questions that lowered
cross-validated error. The round count is hardcoded at five, and the documentation says
plainly that adaptive termination is not implemented. So the thing deciding what to ask
is an expensive text model; Jev is the execution layer.

Two official cookbooks do construct the next round from the previous one, and both do it
**deterministically**. A skill router ranks 182 candidates, slices the top three, and builds
the second round's option set and per-candidate questions directly from that slice — and the
second round can still reject all three. A function-calling cookbook reads a function's type
signature and generates 54 questions per command with no model in the loop at all.

We think the deterministic route is the better one and it is almost unexplored. It is
cheaper, it is reproducible, and the structure it needs — a question that is a value you
can build, inspect and pass around — is something a language can provide and an SDK cannot.

This is the part of J++ we have not built. Questions are not yet first-class values in our
implementation; you cannot even read a question's text back. It is the largest gap between
our design and our code, and after this survey it is also the one we are most confident is
worth closing.

## What we did not cover

Honest limits, because the numbers above are only as good as the sample.

The jevable gallery lists 194 projects; we enumerated 177 and confirmed source for 20
repositories. Gallery entries link to X posts, which we could not fetch — the pages render
client-side with no usable metadata — so most source hunting went through name and author
reverse-lookup on GitHub, which fails whenever someone's GitHub login differs from their
social handle. Three of the four repositories we did confirm this way were found by real
name, not handle.

The weakest spot is games: 39 in the gallery, source confirmed for one. Games are where
multi-round depth lives, so our evidence for the termination section rests on four programs,
only one of them a game. **That is "we could not find it", not "it does not exist."**

We also did not sweep the 50 developer-tool entries. We stopped collecting when three
consecutive batches confirmed the existing findings without changing any of them.

## The short version

Nine mechanisms get hand-written over and over: capability-absent degradation (seven
independent implementations across five languages), deterministic pre-filtering so the model
only makes narrow judgments (eight or more), conservative aggregation that takes the worst
signal rather than the average (six), three- and four-state result enums instead of
try/catch (four), triple-brake termination (four), fit-and-holdout acceptance for tuned
question wording (two independent reinventions of the same protocol), retry logic (two
near-identical structures in Rust and JavaScript), beam-style path search (two), and the
select-then-fill two-call shape that tool routing converges on universally.

When we mapped those nine back onto our kernel — two types, one bridge, one question,
three values, five operators, one ledger — **seven of them already had a place in the
design.** People are hand-writing things our design already accounts for and our
implementation has not caught up with.

Exactly one had no place at all: what a program should do when the judgment capability
itself is absent. Seven people invented the same answer in five languages, and we did not
have the question.

That is the most useful thing this survey gave us, and it is why we did it before writing
more code.

---

*Raw samples and per-sample analyses are kept in our working repository. If you wrote one
of the programs we read and we have described it wrongly, please open an issue — we would
rather fix it than be quoted inaccurately.*
