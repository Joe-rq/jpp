import os

import pytest

from conftest import ROOT, make, run
from jx import ast as A
from jx.check import check_program
from jx.lexer import JxSyntaxError
from jx.parser import load, parse


def test_parse_examples():
    for p in ["app/net.jpx", "jx/examples/mini-net.jpx"]:
        prog, libs = load(os.path.join(ROOT, p))
        assert prog.budget is not None
        assert any(isinstance(s, A.ResidentDecl) for s in prog.stmts)


def test_fable_a_constructs():
    src = '''budget {calls: 1, cost: 0};
cell inbox[b] reducer union; cell reply[b, a, cat] reducer single; cell seat[s] reducer claim;
cell by[k] reducer by_key(id); cell o[k] reducer override;
resident R(b) on [change(inbox[b]), timer(5), event(join), settled(reply[b, b, "x"])] {
  let r = ev; let p = peek inbox[b]; let s = settled reply[b, b, "x"];
  put reply[b, b, "x"] <- {ok: true}; let c = claim seat["s1"] <- b;
  spawn R(b) budget {calls: 2} deadline 10
}'''
    p = parse(src)
    r = [s for s in p.stmts if isinstance(s, A.ResidentDecl)][0]
    assert [s.kind for s in r.sources] == ["change", "timer", "event", "settled"]
    kinds = []
    A.walk(r.body, lambda n: kinds.append(type(n).__name__))
    for k in ("Ev", "Peek", "Settled", "Put", "Claim", "Spawn"):
        assert k in kinds
    cells = {s.name: s for s in p.stmts if isinstance(s, A.CellDecl)}
    assert cells["by"].reducer == "by_key" and cells["by"].reducer_arg == "id"


def test_error_positions():
    with pytest.raises(JxSyntaxError) as e:
        parse("let x = 1\nlet y = 2;", "t.jpx")
    assert (e.value.line, e.value.col) == (2, 1)
    assert "t.jpx:2:1" in str(e.value)
    with pytest.raises(JxSyntaxError) as e:
        parse('let s = "abc;', "t.jpx")
    assert e.value.line == 1 and e.value.col == 9
    with pytest.raises(JxSyntaxError) as e:
        parse("cell c reducer bogus;")
    assert "归约器" in e.value.msg


def test_expressions_eval():
    src = '''budget {calls: 1, cost: 0};
let a = 1 + 2 * 3 == 7 && true;
let b = {x: {y: [1, 2, 3]}}.x.y[2];
let c = if 2 < -1 { "no" } else if 3 >= 3 { "yes" } else { "x" };
let d = {k: 1}.missing ?? "默认";
fn fact(n) { if n <= 1 { 1 } else { n * fact(n - 1) } }
let f = fold([1, 2, 3], 0, fn(acc, x) { acc + x });
// 注释
{a: a, b: b, c: c, d: d, e: fact(5), f: f, g: map(range(3), fn(i) { str(i) + "!" }), h: {}, u: unit}'''

    async def go():
        eng = make(src)
        await eng.start()
        return await eng.report()
    r = run(go())
    assert r == {"a": True, "b": 3, "c": "yes", "d": "默认", "e": 120, "f": 6, "g": ["0!", "1!", "2!"], "h": {}, "u": None}


def test_less_than_minus():
    p = parse("budget {calls: 1, cost: 0}; let a = 2; a<-1")
    assert isinstance(p.final, A.Binary) and p.final.op == "<"


def test_check_reports_undefined(tmp_path):
    f = tmp_path / "x.jpx"
    f.write_text('budget {calls: 1, cost: 0};\nlet y = zz + 1;\ny')
    diags = check_program(str(f))
    assert any(":2:9: 错误：未定义的名字 `zz`" in d for d in diags)


def test_check_net_passes():
    diags = check_program(os.path.join(ROOT, "app/net.jpx"))
    assert not [d for d in diags if "错误" in d]


def test_runtime_error_has_location():
    src = 'budget {calls: 1, cost: 0};\nlet r = {a: 1};\nr.b'

    async def go():
        eng = make(src)
        await eng.start()
        await eng.report()
    with pytest.raises(Exception) as e:
        run(go())
    assert "<test>:3:2" in str(e.value) and "没有字段 `b`" in str(e.value)
