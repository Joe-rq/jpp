# J++ native kernel (Rust) — status, 2026-09-21

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
| effects (judge / gen / do / ask / cut / state) | 1512 | five of six exercised by examples; `ask` has none |
| ledger (replay / resume) | 158 | replay verified: identical keys across three live runs |
| conformal / calibration | 288 | code complete, coverage verified; **no language surface** |

302 tests green. Five example programs, 180 lines, using 21 of the 53 builtins.

## The calibration loop

It closes as of `585b551`: readings written by `--calib-out` are read back by `--calib`, and a
second pass accumulates onto them. Seven builtins were then probed with and without a line in
service — six behave differently, and for two of them (`cut`, `line_source`) the program does
not complete at all when no line exists.

## Known gaps, stated plainly

- **`commission` — putting a line into service — is host-only.** The round trip works, but this
  segment of the loop is reachable only from Rust, not from `.jpp`. A `.jpp` author cannot walk
  the whole loop alone.
- **The conformal family has no language surface**: `commission`, `put`, `absorb`, `certify`,
  `drift`, `cost_line` are not builtins. This is deliberate — `J-03` forbids a program from
  writing a line — but it means those capabilities arrive through host plumbing, not the
  language.
- **`J-17` has zero implementation** anywhere in the tree.
- **`ask` has no example program.**
- 46 of the 53 builtins have not been probed against calibration state; they are believed
  line-independent, which is a judgement rather than a measurement.

## Layout

- `crates/jpp-core` — AST, checker, interpreter, effects, ledger, conformal
- `crates/jpp-frontend` — lexer, parser, lowering, source loader
- `crates/jpp-cli` — the `jpp` binary
- `examples/` — runnable `.jpp` programs with fixtures
- `crates/jpp-core/INTERFACE.md` — kernel interface, including what is *not* implemented
