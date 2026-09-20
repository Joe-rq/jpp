//! 部分候选检验 → 精确组合 → 续解。
//!
//! A(4, web)、B(5, db) 一问即收；C(2, web+db)、D(20, web) 落在线中间是未决。
//! 第一轮用 A+B 交付成本 9；给 C 补材料（新材料 = 新观察身份）后只补 C 得成本 2，D 仍未决；
//! 再换一种问法（分档题 + 另一套校准）处理 D。共 6 次固定观察、3 次本地检查，旧检查不重做。
//!
//! 组合是源码里写的：幂集用 fold 展开，约束用 filter，最省用 fold；内核没有候选求解命令。
//! 「接着做」是语言里的方法值：`packet` 把算法状态和下一步策略封在一个记录里返回。

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use jpp_core::effects::{CalibStore, FixedClient, NoCallClient};
use jpp_core::interp::{ActionRegistry, TaintOut};
use jpp_core::ledger::{Entry, Ledger};
use jpp_core::value::{Answer, Mat, Op, Question, State};
use jpp_core::{Program, run};
use serde_json::{Value as Json, json};

const ASK: &str = "这个候选现在可用吗？";
const GRADE: &str = "这个候选的把握有多大？";
const ACCEPT: &str = "accept";
const CONF: &str = "confidence";
const LEVELS: [&str; 3] = ["low", "mid", "high"];

fn candidate(name: &str, cost: i64, skills: &[&str]) -> Json {
    json!({"name": name, "cost": cost, "skills": skills})
}

fn seed_test(client: &mut FixedClient, on: Json, p: f64) {
    let state = State::new(vec![Mat::literal(on)], vec![], vec![], vec![], false);
    client.observe(
        &state,
        &Question::new(Op::Test, ASK, ACCEPT, vec![]),
        Answer::Noul(p),
    );
}

fn fixed_client() -> FixedClient {
    let mut client = FixedClient::new();
    // 第一轮：四个候选各一次观察。A、B 过上线；C、D 落在线中间 → Unsure(band)
    seed_test(&mut client, candidate("A", 4, &["web"]), 0.95);
    seed_test(&mut client, candidate("B", 5, &["db"]), 0.92);
    seed_test(&mut client, candidate("C", 2, &["web", "db"]), 0.50);
    seed_test(&mut client, candidate("D", 20, &["web"]), 0.50);
    // 第二轮：C 有了补充材料，是另一个观察身份
    seed_test(
        &mut client,
        json!({"name": "C", "cost": 2, "skills": ["web", "db"], "evidence": "supplement"}),
        0.97,
    );
    // 第三轮：D 换成分档题 + 另一套校准。同一份材料、换一道题，仍是新的观察身份
    let state = State::new(
        vec![Mat::literal(candidate("D", 20, &["web"]))],
        vec![],
        vec![],
        vec![],
        false,
    );
    let question = Question::new(
        Op::Measure,
        GRADE,
        CONF,
        LEVELS.iter().map(|s| s.to_string()).collect(),
    );
    client.observe(&state, &question, Answer::Score(vec![0.85, 0.10, 0.05]));
    client
}

fn calibrations() -> CalibStore {
    let mut calib = CalibStore::new();
    calib
        .put(ACCEPT, 0.8, 0.2, 150, "上岗")
        .expect("校准记录合法");
    calib.put(CONF, 0.7, 0.3, 90, "上岗").expect("校准记录合法");
    calib
}

/// 登记 `do` 能触发的本地检查动作：它只记录并回传源码已经算好的值，不含任何候选算法。
fn actions(log: Rc<RefCell<Vec<Json>>>) -> ActionRegistry {
    let mut actions = ActionRegistry::new();
    actions.register("record_check", 0.0, true, TaintOut::Inherit, move |args| {
        if args.len() != 1 {
            return Err("record_check 收一条源码已算好的检查记录".into());
        }
        log.borrow_mut().push(args[0].to_json());
        Ok(args[0].clone())
    });
    actions
}

/// ```text
/// budget {calls: 6, cost: 0, depth: 64};
/// fn look(m, q) -> Record !{judge} {
///     let e = cut(judge(state(m), q));
///     handle(e, {act: fn() {{status: "accepted"}}, ignore: fn() {{status: "rejected"}},
///                unsure: fn(cause) {{status: "pending", cause: cause}}})
/// }
/// fn screen(cands, q) -> List !{judge} { map(cands, fn(c) {{cand: c, status: look(c, q).status}}) }
/// fn pick(items, want) { map(filter(items, fn(x) { x.status == want }), fn(x) { x.cand }) }
/// fn note(c, seq) -> Record !{do} {
///     let ok = c.cost > 0 && len(c.skills) > 0;
///     content(do("record_check", [{name: c.name, cost: c.cost, ok: ok}], seq))
/// }
/// fn record_all(cs, from) -> List !{do} { map(range(0, len(cs)), fn(i) { note(cs[i], from + i) }) }
/// fn subsets(cs) { fold(cs, [[]], fn(gs, c) { concat(gs, map(gs, fn(g) { append(g, c) })) }) }
/// fn total(g) { fold(g, 0, fn(t, c) { t + c.cost }) }
/// fn covers(g, s) { fold(g, false, fn(f, c) { f || contains(c.skills, s) }) }
/// fn usable(g) { covers(g, "web") && covers(g, "db") }
/// fn best(cs) { fold(filter(subsets(cs), usable), {members: [], cost: 999},
///                    fn(b, g) { if total(g) < b.cost { {members: map(g, fn(c) { c.name }), cost: total(g)} } else { b } }) }
/// fn packet(acc, und, checks) {
///     {plan: best(acc), pending: map(und, fn(c) { c.name }), checks: checks,
///      more: fn(strategy) { strategy(acc, und, checks) }}
/// }
/// fn first_round(cands) -> Record !{judge, do} { … }
/// fn supplement(acc, und, checks) -> Record !{judge, do} { … }   // 策略一：补材料再问一次
/// fn grade(acc, und, checks) -> Record !{judge} { … }            // 策略二：换分档题
/// let first = first_round([A, B, C, D]);
/// let second = first.more(supplement);
/// let third = second.more(grade);
/// ```
fn partial_program() -> Program {
    let cand_of = || lambda(&["x"], body(vec![], field(name("x"), "cand")));
    let names_of = || lambda(&["c"], body(vec![], field(name("c"), "name")));

    program(
        Some(budget(6, 64)),
        vec![
            // 一次观察：判断 → 切出口 → 穷尽处理。读数不出现在返回值里
            func_eff(
                "look",
                &["m", "q"],
                &["judge"],
                body(
                    vec![bind("e", call("cut", vec![call("judge", vec![call("state", vec![name("m")]), name("q")])]))],
                    call(
                        "handle",
                        vec![
                            name("e"),
                            rec(vec![
                                ("act", lambda(&[], body(vec![], rec(vec![("status", text("accepted"))])))),
                                ("ignore", lambda(&[], body(vec![], rec(vec![("status", text("rejected"))])))),
                                (
                                    "unsure",
                                    lambda(&["cause"], body(vec![], rec(vec![("status", text("pending")), ("cause", name("cause"))]))),
                                ),
                            ]),
                        ],
                    ),
                ),
            ),
            func_eff(
                "screen",
                &["cands", "q"],
                &["judge"],
                body(
                    vec![],
                    call(
                        "map",
                        vec![
                            name("cands"),
                            lambda(
                                &["c"],
                                body(
                                    vec![],
                                    rec(vec![
                                        ("cand", name("c")),
                                        ("status", field(call("look", vec![name("c"), name("q")]), "status")),
                                    ]),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            func(
                "pick",
                &["items", "want"],
                body(
                    vec![],
                    call(
                        "map",
                        vec![
                            call("filter", vec![name("items"), lambda(&["x"], body(vec![], bin("==", field(name("x"), "status"), name("want"))))]),
                            cand_of(),
                        ],
                    ),
                ),
            ),
            // 本地检查：有效性由源码算，动作只记录并回传
            func_eff(
                "note",
                &["c", "seq"],
                &["do"],
                body(
                    vec![bind(
                        "ok",
                        bin(
                            "&&",
                            bin(">", field(name("c"), "cost"), int(0)),
                            bin(">", call("len", vec![field(name("c"), "skills")]), int(0)),
                        ),
                    )],
                    call(
                        "content",
                        vec![call(
                            "do",
                            vec![
                                text("record_check"),
                                list(vec![rec(vec![
                                    ("name", field(name("c"), "name")),
                                    ("cost", field(name("c"), "cost")),
                                    ("ok", name("ok")),
                                ])]),
                                name("seq"),
                            ],
                        )],
                    ),
                ),
            ),
            func_eff(
                "record_all",
                &["cs", "from"],
                &["do"],
                body(
                    vec![],
                    call(
                        "map",
                        vec![
                            call("range", vec![int(0), call("len", vec![name("cs")])]),
                            lambda(
                                &["i"],
                                body(vec![], call("note", vec![index(name("cs"), name("i")), bin("+", name("from"), name("i"))])),
                            ),
                        ],
                    ),
                ),
            ),
            // —— 精确组合：幂集、约束、最省，全写在语言里
            func(
                "subsets",
                &["cs"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![
                            name("cs"),
                            list(vec![list(vec![])]),
                            lambda(
                                &["gs", "c"],
                                body(
                                    vec![],
                                    call(
                                        "concat",
                                        vec![
                                            name("gs"),
                                            call(
                                                "map",
                                                vec![name("gs"), lambda(&["g"], body(vec![], call("append", vec![name("g"), name("c")])))],
                                            ),
                                        ],
                                    ),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            func(
                "total",
                &["g"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![name("g"), int(0), lambda(&["t", "c"], body(vec![], bin("+", name("t"), field(name("c"), "cost"))))],
                    ),
                ),
            ),
            func(
                "covers",
                &["g", "s"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![
                            name("g"),
                            boolean(false),
                            lambda(
                                &["f", "c"],
                                body(vec![], bin("||", name("f"), call("contains", vec![field(name("c"), "skills"), name("s")]))),
                            ),
                        ],
                    ),
                ),
            ),
            func(
                "usable",
                &["g"],
                body(
                    vec![],
                    bin(
                        "&&",
                        call("covers", vec![name("g"), text("web")]),
                        call("covers", vec![name("g"), text("db")]),
                    ),
                ),
            ),
            func(
                "best",
                &["cs"],
                body(
                    vec![],
                    call(
                        "fold",
                        vec![
                            call("filter", vec![call("subsets", vec![name("cs")]), name("usable")]),
                            rec(vec![("members", list(vec![])), ("cost", int(999))]),
                            lambda(
                                &["b", "g"],
                                body(
                                    vec![],
                                    if_(
                                        bin("<", call("total", vec![name("g")]), field(name("b"), "cost")),
                                        rec(vec![
                                            ("members", call("map", vec![name("g"), names_of()])),
                                            ("cost", call("total", vec![name("g")])),
                                        ]),
                                        name("b"),
                                    ),
                                ),
                            ),
                        ],
                    ),
                ),
            ),
            // 部分结果：可用方案 + 未决项 + 一个「接着做」的方法值
            func(
                "packet",
                &["acc", "und", "checks"],
                body(
                    vec![],
                    rec(vec![
                        ("plan", call("best", vec![name("acc")])),
                        ("pending", call("map", vec![name("und"), names_of()])),
                        ("checks", name("checks")),
                        (
                            "more",
                            lambda(
                                &["strategy"],
                                body(vec![], call_of(name("strategy"), vec![name("acc"), name("und"), name("checks")])),
                            ),
                        ),
                    ]),
                ),
            ),
            func_eff(
                "first_round",
                &["cands"],
                &["judge", "do"],
                body(
                    vec![
                        bind("seen", call("screen", vec![name("cands"), call("test", vec![text(ASK), text(ACCEPT)])])),
                        bind("acc", call("pick", vec![name("seen"), text("accepted")])),
                        bind("checks", call("record_all", vec![name("acc"), int(0)])),
                    ],
                    call(
                        "packet",
                        vec![name("acc"), call("pick", vec![name("seen"), text("pending")]), call("len", vec![name("checks")])],
                    ),
                ),
            ),
            // 策略一：给便宜的未决项补材料，再问同一道题——新材料就是新的观察身份
            func_eff(
                "supplement",
                &["acc", "und", "checks"],
                &["judge", "do"],
                body(
                    vec![
                        bind("q", call("test", vec![text(ASK), text(ACCEPT)])),
                        bind(
                            "seen",
                            call(
                                "map",
                                vec![
                                    name("und"),
                                    lambda(
                                        &["c"],
                                        body(
                                            vec![],
                                            if_(
                                                bin("<=", field(name("c"), "cost"), int(2)),
                                                rec(vec![
                                                    ("cand", name("c")),
                                                    (
                                                        "status",
                                                        field(
                                                            call(
                                                                "look",
                                                                vec![
                                                                    call(
                                                                        "transform",
                                                                        vec![
                                                                            lambda(
                                                                                &["old"],
                                                                                body(
                                                                                    vec![],
                                                                                    call(
                                                                                        "with",
                                                                                        vec![
                                                                                            call("content", vec![name("old")]),
                                                                                            text("evidence"),
                                                                                            text("supplement"),
                                                                                        ],
                                                                                    ),
                                                                                ),
                                                                            ),
                                                                            name("c"),
                                                                        ],
                                                                    ),
                                                                    name("q"),
                                                                ],
                                                            ),
                                                            "status",
                                                        ),
                                                    ),
                                                ]),
                                                rec(vec![("cand", name("c")), ("status", text("pending"))]),
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                        ),
                        bind("more", call("pick", vec![name("seen"), text("accepted")])),
                    ],
                    call(
                        "packet",
                        vec![
                            call("concat", vec![name("acc"), name("more")]),
                            call("pick", vec![name("seen"), text("pending")]),
                            bin(
                                "+",
                                name("checks"),
                                call("len", vec![call("record_all", vec![name("more"), name("checks")])]),
                            ),
                        ],
                    ),
                ),
            ),
            // 策略二：换一道分档题、换一套校准，处理剩下的未决项
            func_eff(
                "grade",
                &["acc", "und", "checks"],
                &["judge"],
                body(
                    vec![
                        bind(
                            "q",
                            call(
                                "measure",
                                vec![text(GRADE), list(LEVELS.iter().map(|s| text(s)).collect()), text(CONF)],
                            ),
                        ),
                        bind(
                            "seen",
                            call(
                                "map",
                                vec![
                                    name("und"),
                                    lambda(
                                        &["c"],
                                        body(
                                            vec![],
                                            call(
                                                "handle",
                                                vec![
                                                    call("cut", vec![call("judge", vec![call("state", vec![name("c")]), name("q")])]),
                                                    rec(vec![
                                                        (
                                                            "at",
                                                            lambda(
                                                                &["level"],
                                                                body(
                                                                    vec![],
                                                                    rec(vec![
                                                                        ("cand", name("c")),
                                                                        (
                                                                            "status",
                                                                            if_(
                                                                                bin(">=", name("level"), int(2)),
                                                                                text("accepted"),
                                                                                text("rejected"),
                                                                            ),
                                                                        ),
                                                                    ]),
                                                                ),
                                                            ),
                                                        ),
                                                        (
                                                            "unsure",
                                                            lambda(
                                                                &["cause"],
                                                                body(
                                                                    vec![],
                                                                    rec(vec![("cand", name("c")), ("status", text("pending"))]),
                                                                ),
                                                            ),
                                                        ),
                                                    ]),
                                                ],
                                            ),
                                        ),
                                    ),
                                ],
                            ),
                        ),
                    ],
                    call(
                        "packet",
                        vec![
                            call("concat", vec![name("acc"), call("pick", vec![name("seen"), text("accepted")])]),
                            call("pick", vec![name("seen"), text("pending")]),
                            name("checks"),
                        ],
                    ),
                ),
            ),
            bind(
                "first",
                call(
                    "first_round",
                    vec![list(vec![
                        rec(vec![("name", text("A")), ("cost", int(4)), ("skills", list(vec![text("web")]))]),
                        rec(vec![("name", text("B")), ("cost", int(5)), ("skills", list(vec![text("db")]))]),
                        rec(vec![("name", text("C")), ("cost", int(2)), ("skills", list(vec![text("web"), text("db")]))]),
                        rec(vec![("name", text("D")), ("cost", int(20)), ("skills", list(vec![text("web")]))]),
                    ])],
                ),
            ),
            bind("second", call_of(field(name("first"), "more"), vec![name("supplement")])),
            bind("third", call_of(field(name("second"), "more"), vec![name("grade")])),
        ],
        rec(vec![
            ("first", snapshot("first")),
            ("second", snapshot("second")),
            ("third", snapshot("third")),
            ("done", bin("==", call("len", vec![field(name("third"), "pending")]), int(0))),
        ]),
    )
}

/// 取一个部分结果里可展示的三项（`more` 是方法值，不进对照）
fn snapshot(which: &str) -> jpp_core::Expr {
    rec(vec![
        ("plan", field(name(which), "plan")),
        ("pending", field(name(which), "pending")),
        ("checks", field(name(which), "checks")),
    ])
}

#[test]
fn 部分候选先交付再续解() {
    let program = partial_program();
    assert_clean(&program);

    let log = Rc::new(RefCell::new(Vec::new()));
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let outcome = run(
        &program,
        &mut client,
        &calibrations(),
        &actions(log.clone()),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("程序应当跑完：{}", e.render()));

    assert_eq!(
        outcome.value_json(),
        json!({
            "first":  {"plan": {"members": ["A", "B"], "cost": 9}, "pending": ["C", "D"], "checks": 2},
            "second": {"plan": {"members": ["C"],      "cost": 2}, "pending": ["D"],      "checks": 3},
            "third":  {"plan": {"members": ["C"],      "cost": 2}, "pending": [],         "checks": 3},
            "done": true
        })
    );
    assert!(
        outcome.pending.is_empty(),
        "程序没有被挂起：未决是算法的返回值，不是程序级出口"
    );
    assert_eq!(client.log.len(), 6, "六次固定观察");
    assert_eq!(outcome.cost.calls, 6);
    assert_eq!(outcome.trace.count("judge", false), 6);
    assert_eq!(
        log.borrow().len(),
        3,
        "三次本地检查：A、B 各一次，C 补材料后一次"
    );
    assert_eq!(outcome.trace.count("do", false), 3);
    assert_eq!(
        outcome.trace.count("transform", false),
        1,
        "只给 C 补了材料"
    );
    assert_eq!(outcome.cost.replayed, 0, "第一次跑没有旧账本可重放");
    assert!(
        outcome.trace.warnings.is_empty(),
        "不该有 W-bound / W-header：{:?}",
        outcome.trace.warnings
    );

    // 旧检查不重做：三条 do 记录的键互不相同，A、B 的那两条在后两轮没有再出现
    let keys: Vec<&str> = ledger
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Effect { key, kind, .. } if kind == "do" => Some(key.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(keys.len(), 3);
    assert_eq!(
        keys.iter().collect::<std::collections::HashSet<_>>().len(),
        3,
        "三条检查是三个不同的账本键"
    );
    let names: Vec<String> = log
        .borrow()
        .iter()
        .map(|v| v["name"].as_str().unwrap_or("?").to_string())
        .collect();
    assert_eq!(names, vec!["A", "B", "C"]);
}

#[test]
fn 部分候选程序重放零调用() {
    let program = partial_program();
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut client = fixed_client();
    let mut ledger = Ledger::new();
    let first = run(
        &program,
        &mut client,
        &calibrations(),
        &actions(log.clone()),
        &mut ledger,
    )
    .expect("首跑");

    let replay_log = Rc::new(RefCell::new(Vec::new()));
    let mut replay = NoCallClient;
    let again = run(
        &program,
        &mut replay,
        &calibrations(),
        &actions(replay_log.clone()),
        &mut ledger,
    )
    .unwrap_or_else(|e| panic!("重放不该发调用：{}", e.render()));

    assert_eq!(again.value_json(), first.value_json());
    assert_eq!(again.cost.calls, 0, "重放零调用");
    assert_eq!(
        again.cost.replayed, 10,
        "6 次判断 + 3 次动作 + 1 次变换全部命中账本"
    );
    assert!(
        replay_log.borrow().is_empty(),
        "重放不重新执行动作，只取账本里的输出"
    );
    assert!(
        again.trace.warnings.is_empty(),
        "{:?}",
        again.trace.warnings
    );
}
