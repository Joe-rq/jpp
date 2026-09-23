//! The same method composition expressed as source and as a direct core program.
use jpp_core::{
    ast::{Block, Budget, Expr, Program, Span, let_},
    effects::{CalibStore, NoCallClient},
    interp::ActionRegistry,
    ledger::Ledger,
};

fn execute(program: &Program) -> String {
    let mut client = NoCallClient;
    let mut ledger = Ledger::new();
    let calibrations = CalibStore::new();
    let actions = ActionRegistry::new();
    let outcome =
        jpp_core::run(program, &mut client, &calibrations, &actions, &mut ledger).unwrap();
    assert!(outcome.pending.is_empty());
    assert_eq!(outcome.cost.calls, 0);
    outcome.value_json().to_string()
}

#[test]
fn nested_method_composition_matches_direct_core_construction() {
    let source = jpp_frontend::parse(include_str!("../../../examples/composition.jpp")).unwrap();
    let lowered = jpp_frontend::lower(&source).unwrap();
    let s = Span::default();
    let name = |n| Expr::name(n, s);
    let call = |n, args| Expr::call_name(n, args, s);
    let returned_method = Expr::func(
        &["x"],
        None,
        Block::expr(call("g", vec![call("f", vec![name("x")])])),
        s,
    );
    let compose = Expr::func(&["f", "g"], None, Block::expr(returned_method), s);
    let increment = Expr::func(
        &["x"],
        None,
        Block::expr(Expr::binary("+", name("x"), Expr::int(1, s), s)),
        s,
    );
    let twice = Expr::func(
        &["x"],
        None,
        Block::expr(Expr::binary("*", name("x"), Expr::int(2, s), s)),
        s,
    );
    let direct = Program {
        budget: Some(Budget {
            calls: 0,
            cost: 0.0,
            depth: Some(256),
            escalate: None,
            unsure: None,
        }),
        span: s,
        body: Block::new(
            vec![
                let_("compose", compose, s),
                let_("increment", increment, s),
                let_("twice", twice, s),
                let_(
                    "method",
                    call("compose", vec![name("increment"), name("twice")]),
                    s,
                ),
                let_(
                    "larger",
                    call("compose", vec![name("method"), name("increment")]),
                    s,
                ),
            ],
            Some(Expr::record(
                vec![
                    ("result", call("larger", vec![Expr::int(20, s)])),
                    ("expected", Expr::int(43, s)),
                ],
                s,
            )),
            s,
        ),
    };
    let result = execute(&lowered);
    assert_eq!(result, execute(&direct));
    assert_eq!(result, r#"{"expected":43,"result":43}"#);
    let typed_source = include_str!("../../../examples/composition.jpp")
        .replace("Fn(Int) -> Int", "Fn(Int) -!{}-> Int");
    let typed = jpp_frontend::lower(&jpp_frontend::parse(&typed_source).unwrap()).unwrap();
    assert_eq!(execute(&typed), execute(&direct));
}
