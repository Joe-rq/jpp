# J++ native kernel (Rust) — reviewed status, 2026-09-23

`.jpp` source → lex → parse → lower → check → interpret. The Python tree under
`src/foundation/jv/` is the frozen reference implementation, kept as a behavioural oracle. It is
not deprecated: cross-checking the two found a failure-open bug in `cut` this week that 285
green tests could not see.

## What runs today

```
jpp parse <file.jpp> [--ast]
jpp check <file.jpp>
jpp run   <file.jpp> --fixtures <f.json> [--output <r.json>] [--ledger-out <l.json>]
          [--replay <l.json> | --resume <l.json>]
          [--profile <p.json>] [--calib <dir>] [--calib-out <dir>]
```

| Component | Lines | State |
|---|---:|---|
| lexer + parser + lower + loader | 977 | complete for the current surface |
| static checker | 1851 | 17 of 18 typing rules; `J-17` not implemented |
| interpreter | 3111 | 53 builtins |
| effects (judge / gen / do / ask / cut / state) | 1512 | `lifecycle.jpp` exercises escalation to human response with fixtures |
| ledger (replay / resume) | 158 | replay verified: identical keys across three live runs |
| conformal / calibration | — | experimental host implementation; **no general selected-threshold risk guarantee** |

The review checkout reports 304 passing Rust tests and 3 explicitly ignored tests/snippets.
Live model calls were not run. Some historical research-data probes return early when
private records are absent; this count does not establish new experiment results.
Source-line and builtin counts above are the original inventory, not a generated census.

## The calibration loop

The observation persistence round trip works: readings written by `--calib-out` are read back by `--calib`, and a
second pass accumulates onto them. Seven builtins were then probed with and without a line in
service — six behave differently, and for two of them (`cut`, `line_source`) the program does
not complete at all when no line exists.

This is not the complete labeled-data → commissioning workflow. `certify` scans
thresholds on the same samples used for pointwise binomial bounds; selection correction
or independent validation remains unimplemented. The costed path checks distinct dataset
IDs but still reads the same stored samples. A `Cert` is an experimental host-policy
record, not proof of general risk control. The synthetic Monte Carlo test covers one
distribution only. PR review fixed large-sample binomial underflow and preserved the
reject-all threshold so scores equal to 1 are not accidentally accepted.

## Known gaps, stated plainly

- **`commission` — putting a line into service — is host-only.** The round trip works, but this
  segment of the loop is reachable only from Rust, not from `.jpp`. A `.jpp` author cannot walk
  the whole loop alone.
- **The conformal family has no language surface**: `commission`, `put`, `absorb`, `certify`,
  `drift`, `cost_line` are not builtins. This is deliberate — `J-03` forbids a program from
  writing a line — but it means those capabilities arrive through host plumbing, not the
  language.
- **`J-17` has zero implementation** anywhere in the tree.
- **Human response is fixture-driven in the CLI.** See `examples/lifecycle.jpp`; a live interaction channel remains separate work.
- 46 of the 53 builtins have not been probed against calibration state; they are believed
  line-independent, which is a judgement rather than a measurement.

## Layout

- `crates/jpp-core` — AST, checker, interpreter, effects, ledger, conformal
- `crates/jpp-frontend` — lexer, parser, lowering, source loader
- `crates/jpp-cli` — the `jpp` binary
- `examples/` — runnable `.jpp` programs with fixtures
- `crates/jpp-core/INTERFACE.md` — kernel interface, including what is *not* implemented
