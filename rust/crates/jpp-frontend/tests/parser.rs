use jpp_frontend::{
    ast::{ExprKind, Statement, Type},
    parse,
};

#[test]
fn methods_can_be_parameters_and_returned_closures() {
    let source = r#"
fn compose(f: Fn(Int) -> Int, g: Fn(Int) -> Int) -> Fn(Int) -> Int !{} {
    fn(x: Int) -> Int { g(f(x)) }
}
let twice = compose(fn(x: Int) -> Int { x + 1 }, fn(y: Int) -> Int { y * 2 });
{value: twice(20), unresolved: []}
"#;
    let program = parse(source).unwrap();
    let Statement::Function { function, .. } = &program.body.statements[0] else {
        panic!()
    };
    assert!(matches!(
        function.parameters[0].annotation,
        Some(Type::Function(_, _))
    ));
    assert_eq!(function.effects, Some(vec![]));
    assert!(matches!(
        program.body.result.unwrap().kind,
        ExprKind::Record(_)
    ));
}

#[test]
fn precedence_and_postfix_are_preserved() {
    let parsed = parse("-f(2).values[0] + 3 * 4 == 10 && true").unwrap();
    let ExprKind::Binary { op, left, .. } = parsed.body.result.unwrap().kind else {
        panic!()
    };
    assert_eq!(op, "&&");
    let ExprKind::Binary { op, left, .. } = left.kind else {
        panic!()
    };
    assert_eq!(op, "==");
    let ExprKind::Binary { op, left, right } = left.kind else {
        panic!()
    };
    assert_eq!(op, "+");
    assert!(matches!(left.kind, ExprKind::Unary { .. }));
    assert!(matches!(right.kind, ExprKind::Binary { ref op, .. } if op == "*"));
}

#[test]
fn unicode_source_errors_point_to_the_user_line() {
    let source = "let 问题 = \"中文\";\nlet 结果 = [1, 2;";
    let error = parse(source).unwrap_err();
    assert_eq!(&source[error.span.start..error.span.end], ";");
    let rendered = error.render("用户.jpp", source);
    assert!(
        rendered.starts_with("用户.jpp:2:15: expected ','"),
        "{rendered}"
    );
}

#[test]
fn malformed_source_never_becomes_a_partial_program() {
    for source in [
        "let x = ;",
        "fn f(x: Int) {",
        "[1,",
        "f(1",
        "{a: 1, a: 2}",
        "\"unterminated",
        "1 2",
        "let if = 1;",
    ] {
        assert!(parse(source).is_err(), "accepted {source}");
    }
}

#[test]
fn comments_branches_and_effect_annotations_parse() {
    let source = "// retained work\nfn step(s: Record) -> Record !{observe, action} {\nif s.done { s } else if s.remaining == 0 { {done: true} } else { let next = s.remaining - 1; {done: false, remaining: next} }\n}\nstep({done: false, remaining: 2})";
    let p = parse(source).unwrap();
    let Statement::Function { function, .. } = &p.body.statements[0] else {
        panic!()
    };
    assert_eq!(
        function.effects,
        Some(vec!["observe".into(), "action".into()])
    );
    assert!(matches!(
        function.body.result.as_ref().unwrap().kind,
        ExprKind::If { .. }
    ));
}

#[test]
fn complete_migration_sources_parse() {
    for source in [
        include_str!("../../../examples/composition.jpp"),
        include_str!("../../../examples/adaptive.jpp"),
        include_str!("../../../examples/partial.jpp"),
    ] {
        if let Err(error) = parse(source) {
            panic!("{}", error.render("example.jpp", source));
        }
    }
}
