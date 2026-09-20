# J++ source frontend — initial grammar

Status: source parsing and lowering target the shared core AST. Three source
programs have executed through the common checker and interpreter. Known literal
argument type errors, missing budgets and source syntax errors are tested; this is
a defined static-check subset, with remaining dynamic rules enforced at runtime.

```ebnf
program    = [ "budget", record, ";" ], { statement }, [ expression ];
statement  = "let", identifier, [ ":", type ], "=", expression, ";"
           | "fn", identifier, function-tail, [ ";" ]
           | expression, ";";
block      = "{", { statement }, [ expression ], "}";
function-tail = "(", [ parameter, { ",", parameter } ], ")",
                [ "->", type ], [ "!", "{", [ effect, { ",", effect } ], "}" ], block;
parameter  = identifier, [ ":", type ];
type       = identifier, [ "<", type, { ",", type }, ">" ]
           | "Fn", "(", [ type, { ",", type } ], ")", "->", type;
expression = literal | identifier | list | record | block
           | "fn", function-tail
           | "if", expression, block, "else", (block | expression)
           | expression, "(", [ expression, { ",", expression } ], ")"
           | expression, ".", identifier | expression, "[", expression, "]"
           | unary-op, expression | expression, binary-op, expression;
```

Names support Unicode letters; `//` starts a comment. Text uses double quotes,
with `\n`, `\r`, `\t`, `\"`, `\\`. Literals include integers, decimals, booleans,
`unit`/`()`, lists `[1, 2]` and records `{value: 1, pending: []}`. Record keys may
also be quoted. A record is distinguished by its first `name:`; otherwise braces
are a block. `{}` in expression position is an empty record. Trailing commas
are allowed in parameters, calls, lists and records.

Postfix calls/fields/indexes bind tightest, followed by unary `! -`, then `* / %`,
`+ -`, comparisons, equality, `&&`, `||`. Binary operators associate left.
Blocks return their final expression; semicolon expressions do not return a value.
Functions and local bindings are immutable; bounded control will use the shared
core's generic higher-order operations. The parser accepts type/effect annotations;
the common checker decides their meaning and validates effects.

Frontend API: `parse(&str) -> Result<ast::Program, Diagnostic>`. All expressions,
bindings and parameters carry byte spans. `Diagnostic::render(filename, source)`
prints file/line/column plus the relevant source line. The source AST is frontend
owned; executable values/environments and the common program representation stay
core owned. `lower(&Program)` maps to `jpp_core::ast::Program`, preserving spans
and annotations. Source programs declare literal limits, for example
`budget {calls: 10, cost: 0, depth: 256};`. The first two fields are required in a
present budget; depth and escalate are optional. An absent budget remains absent
for the common checker to diagnose. The frontend does not invent a default budget.

Migration behavior: `examples/composition.jpp` passes and returns methods. The
other source programs implement adaptive inquiry and partial validation/combination/
continuation using generic operations rather than hidden domain solvers.

## Shared core operations used by the source examples

The examples use these shared-core signatures and have passed joint execution.

`map(list, fn)`, `filter(list, fn)`, `fold(list, initial, fn)` and
`loop(bound, initial, fn(accumulator, index))` supply generic control. `stop(value)`
ends a bounded loop early. The search and candidate algorithms live in `.jpp`.

`mat(value)` creates material; `state(material)` prepares a judgment state.
`test(text, calibration_key)` creates a question value. `judge(state, question)`
produces a reading, `cut(reading)` creates an exit, and `handle(exit, branches)`
consumes that exit with explicit branches. The example functions `observe`,
`resolved`, and `answer` are written in J++, not extra runtime primitives.

`transform(fn, material)` derives new material with provenance.
`do("record_check", [value], 0)` calls the registered local action. That action
records and returns a check already computed by the source program; it contains
no candidate-selection or constraint-solving algorithm.

The JSON fixtures enumerate exact materials, questions, and readings. They do
not implement a search oracle in the CLI. Their calibration records are synthetic
test data, not measurements of model accuracy. Replay uses the core ledger and a
client that rejects fresh requests; it does not serialize captured language closures.
