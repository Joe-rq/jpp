//! 步 34 V5：账本 v5 格式（预注册 `地基/过程记录/工程-V5-账本格式.md`，预测 P6、P7、P10 与 §三、§四、§六）。
//!
//! 每种新变体与一行固定的 JSON 字面串逐字节比对（钉住冻结清单 §4.1 与本步取的英文字段名），再解码回来相等；
//! 六种 v4 去向事件与 `Duty` 互转无损（给 C3 的投影）；账本头新字段为空不写、比对取并集；`CarryRecord` 旧名照读；
//! 版本闸与不认识的条目种类报清楚的错。
use jpp_ledger::skeleton::*;
use jpp_ledger::{
    CarryRecord, Entry, Header, HeaderCompare, LEDGER_VERSION, Ledger, Skipped, SpanId, TraceId,
    line_hash,
};
use serde_json::json;

fn at() -> AttemptRef {
    AttemptRef {
        program: "P".into(),
        n: 2,
    }
}

fn mark() -> DebtMark {
    DebtMark {
        frame: FrameKind::Code,
        owner: "c1".into(),
        via: Via::Cut,
        nth: 3,
        key: "q9".into(),
        cause: "tie".into(),
    }
}

/// (条目, 它的线上 JSON)：线上 JSON 是冻结的，改了就是改格式。
fn 全部新变体() -> Vec<(Entry, &'static str)> {
    vec![
        (
            Entry::Attempt {
                program: "P".into(),
                n: 2,
                host_epoch: 4,
                flush_epoch: 7,
                snapshot: Snapshot {
                    settled_reads: vec![VersionAt {
                        unit: "Q".into(),
                        version: 3,
                    }],
                    shared: vec![VersionAt {
                        unit: "cell".into(),
                        version: 1,
                    }],
                },
                peeks: vec![Peek {
                    prog: "R".into(),
                    version: 2,
                    state: PubState::InProgress,
                    hash: "h".into(),
                }],
                stale: vec!["S".into()],
            },
            r#"{"Attempt":{"program":"P","n":2,"host_epoch":4,"flush_epoch":7,"snapshot":{"settled_reads":[{"unit":"Q","version":3}],"shared":[{"unit":"cell","version":1}]},"peeks":[{"prog":"R","version":2,"state":"in_progress","hash":"h"}],"stale":["S"]}}"#,
        ),
        (
            Entry::Flush {
                f: 1,
                calls: vec![FlushCall {
                    call: 5,
                    material: "m".into(),
                    merged_by: Some("fuse".into()),
                    questions: vec![FlushQuestion {
                        key: "q".into(),
                        requesters: vec!["P".into(), "Q".into()],
                        payer: "P".into(),
                        identity: Some("id".into()),
                    }],
                }],
                flushed_at: None,
            },
            r#"{"Flush":{"f":1,"calls":[{"call":5,"material":"m","merged_by":"fuse","questions":[{"key":"q","requesters":["P","Q"],"payer":"P","identity":"id"}]}]}}"#,
        ),
        (
            Entry::HostEvent {
                epoch: 3,
                event: json!({"tick": 1}),
            },
            r#"{"HostEvent":{"epoch":3,"event":{"tick":1}}}"#,
        ),
        (
            Entry::Merge {
                attempt: at(),
                cell: "c".into(),
                key: "k".into(),
                hash: "h".into(),
                version: 2,
                kind: MergeKind::Rejected,
                cause: Some("claim_conflict".into()),
            },
            r#"{"Merge":{"attempt":{"program":"P","n":2},"cell":"c","key":"k","hash":"h","version":2,"type":"rejected","cause":"claim_conflict"}}"#,
        ),
        (
            Entry::Opaque {
                key: "a".into(),
                world_epoch: 9,
                decl: OpaqueDecl::Undeclared,
                value: json!([1, 2]),
            },
            r#"{"Opaque":{"key":"a","world_epoch":9,"decl":"undeclared","value":[1,2]}}"#,
        ),
        (
            Entry::Transparent {
                key: "t".into(),
                port_reads: vec![PortRead {
                    key: "w".into(),
                    version: 4,
                }],
                hash: "h".into(),
            },
            r#"{"Transparent":{"key":"t","port_reads":[{"key":"w","version":4}],"hash":"h"}}"#,
        ),
        (
            Entry::Publish {
                attempt: at(),
                version: 3,
                state: PubState::Unsure,
                cause: Some("violation".into()),
                hash: "h".into(),
            },
            r#"{"Publish":{"attempt":{"program":"P","n":2},"version":3,"state":"unsure","cause":"violation","hash":"h"}}"#,
        ),
        (
            Entry::Violation {
                attempt: at(),
                mark: mark(),
                also: vec![],
            },
            r#"{"Violation":{"attempt":{"program":"P","n":2},"mark":{"frame":"code","owner":"c1","via":"cut","nth":3,"key":"q9","cause":"tie"}}}"#,
        ),
        // Z0593：同一判断的其余视图
        (
            Entry::Violation {
                attempt: at(),
                mark: mark(),
                also: vec![mark()],
            },
            r#"{"Violation":{"attempt":{"program":"P","n":2},"mark":{"frame":"code","owner":"c1","via":"cut","nth":3,"key":"q9","cause":"tie"},"also":[{"frame":"code","owner":"c1","via":"cut","nth":3,"key":"q9","cause":"tie"}]}}"#,
        ),
        (
            Entry::Unasked {
                attempt: at(),
                key: "q".into(),
                payer: "P".into(),
                reason: StopCause::Depth,
            },
            r#"{"Unasked":{"attempt":{"program":"P","n":2},"key":"q","payer":"P","reason":"depth"}}"#,
        ),
        (
            Entry::Stop {
                attempt: at(),
                cause: StopCause::Deadline,
            },
            r#"{"Stop":{"attempt":{"program":"P","n":2},"cause":"deadline"}}"#,
        ),
        (
            Entry::Withheld {
                attempt: at(),
                key: "intent:e".into(),
                cause: WithheldCause::Violation,
            },
            r#"{"Withheld":{"attempt":{"program":"P","n":2},"key":"intent:e","cause":"violation"}}"#,
        ),
        // G2 附录三（Z0564）：挂起与出错
        (
            Entry::Withheld {
                attempt: at(),
                key: "intent:e".into(),
                cause: WithheldCause::Suspended,
            },
            r#"{"Withheld":{"attempt":{"program":"P","n":2},"key":"intent:e","cause":"suspended"}}"#,
        ),
        (
            Entry::Withheld {
                attempt: at(),
                key: "intent:e".into(),
                cause: WithheldCause::Error,
            },
            r#"{"Withheld":{"attempt":{"program":"P","n":2},"key":"intent:e","cause":"error"}}"#,
        ),
        (
            Entry::Intent {
                key: "intent:e".into(),
                at: 1,
                attempt: Some(at()),
            },
            r#"{"Intent":{"key":"intent:e","at":1,"attempt":{"program":"P","n":2}}}"#,
        ),
        (
            Entry::Skip {
                of: vec!["k".into()],
                kind: "do".into(),
                site: 4,
                attempt: Some(at()),
            },
            r#"{"Skip":{"of":["k"],"kind":"do","site":4,"attempt":{"program":"P","n":2}}}"#,
        ),
    ]
}

fn duty(form: DutyForm) -> Entry {
    Entry::Duty(Duty {
        attempt: at(),
        of: vec!["k".into()],
        cause: Some("band".into()),
        site: 3,
        marks: vec![mark()],
        detail: "说明".into(),
        form,
    })
}

/// `Duty` 的六种形式，各自的线上 JSON（外壳相同，只 `form` 不同）。
fn 六种形式() -> Vec<(Entry, String)> {
    let 外 = |form: &str| {
        format!(
            r#"{{"Duty":{{"attempt":{{"program":"P","n":2}},"of":["k"],"cause":"band","site":3,"marks":[{{"frame":"code","owner":"c1","via":"cut","nth":3,"key":"q9","cause":"tie"}}],"detail":"说明","form":{form}}}}}"#
        )
    };
    vec![
        (
            duty(DutyForm::Refine {
                how: "literalize".into(),
                to: Some("k2".into()),
            }),
            外(r#"{"refine":{"how":"literalize","to":"k2"}}"#),
        ),
        (
            duty(DutyForm::Enrich {
                need: "证据".into(),
                round: 1,
                got: true,
                asked_by: Some("k3".into()),
            }),
            外(r#"{"enrich":{"need":"证据","round":1,"got":true,"asked_by":"k3"}}"#),
        ),
        (
            duty(DutyForm::Reselect {
                from: 0,
                chosen: Some(1),
                skipped: vec![Skipped { k: 0, p: 0.6 }],
            }),
            外(r#"{"reselect":{"from":0,"chosen":1,"skipped":[{"k":0,"p":0.6}]}}"#),
        ),
        (
            duty(DutyForm::Escalate {
                ask: Some("a1".into()),
            }),
            外(r#"{"escalate":{"ask":"a1"}}"#),
        ),
        (
            duty(DutyForm::DropAccounted {}),
            外(r#"{"drop_accounted":{}}"#),
        ),
        (
            duty(DutyForm::Handoff {
                to: "program".into(),
            }),
            外(r#"{"handoff":{"to":"program"}}"#),
        ),
    ]
}

#[test]
fn 每种新变体_线上字面串冻结_解码往返() {
    let mut all: Vec<(Entry, String)> = 全部新变体()
        .into_iter()
        .map(|(e, s)| (e, s.to_string()))
        .collect();
    all.extend(六种形式());
    for (e, want) in &all {
        let got = serde_json::to_string(e).unwrap();
        assert_eq!(&got, want, "线上形状变了");
        let back: Entry = serde_json::from_str(want).unwrap();
        assert_eq!(&back, e);
        // 新变体不进键索引（`Intent` 是旧变体，本来就有键）
        assert!(
            matches!(e, Entry::Intent { .. }) || e.key().is_empty(),
            "{want}"
        );
    }
    // 整份账本往返：编码—解码—再编码逐字节相同
    let mut l = Ledger::new();
    l.set_header(Header::new(1, 1.0, "m", "r", "h"));
    for (e, _) in &all {
        l.put(e.clone());
    }
    let text = l.encode();
    assert!(text.starts_with(&format!("{{\"version\":{LEDGER_VERSION},")));
    let (back, t) = Ledger::decode(&text).unwrap();
    assert!(t.is_none());
    assert_eq!(back.entries, l.entries);
    assert_eq!(back.encode(), text);
}

#[test]
fn 新结构拒收未知字段() {
    for bad in [
        r#"{"Attempt":{"program":"P","n":1,"host_epoch":0,"flush_epoch":0,"x":1}}"#,
        r#"{"Violation":{"attempt":{"program":"P","n":1,"x":1},"mark":{"frame":"code","owner":"o","via":"cut","nth":1,"key":"k","cause":"tie"}}}"#,
        r#"{"Violation":{"attempt":{"program":"P","n":1},"mark":{"frame":"code","owner":"o","via":"cut","nth":1,"key":"k","cause":"tie","x":1}}}"#,
        r#"{"Duty":{"attempt":{"program":"P","n":1},"form":{"handoff":{"to":"p","x":1}}}}"#,
        r#"{"Duty":{"attempt":{"program":"P","n":1},"form":{"drop":{}}}}"#,
        r#"{"Stop":{"attempt":{"program":"P","n":1},"cause":"violation"}}"#,
        r#"{"Merge":{"attempt":{"program":"P","n":1},"cell":"c","key":"k","hash":"h","version":1,"type":"rejected:claim_conflict"}}"#,
    ] {
        assert!(serde_json::from_str::<Entry>(bad).is_err(), "应拒收：{bad}");
    }
}

#[test]
fn 六种v4去向事件与duty互转无损_类由形式推出() {
    let 旧 = vec![
        (
            Entry::Drop {
                of: vec!["k1".into()],
                cause: "band".into(),
                site: 3,
            },
            "drop_accounted",
            DutyClass::Dropped,
        ),
        (
            Entry::Refine {
                of: vec!["k1".into()],
                cause: "tie".into(),
                site: 3,
                how: "literalize".into(),
                to: Some("k2".into()),
            },
            "refine",
            DutyClass::Another,
        ),
        (
            Entry::Enrich {
                of: vec!["k1".into()],
                cause: "band".into(),
                site: 3,
                need: "证据".into(),
                round: 1,
                got: true,
                asked_by: Some("k3".into()),
            },
            "enrich",
            DutyClass::Another,
        ),
        (
            Entry::Handoff {
                of: vec![],
                cause: "budget".into(),
                site: 9,
                to: "program".into(),
            },
            "handoff",
            DutyClass::HandedOff,
        ),
        (
            Entry::Escalate {
                of: vec!["k1".into()],
                cause: "band".into(),
                site: 3,
                ask: Some("a1".into()),
            },
            "escalate",
            DutyClass::Escalated,
        ),
        (
            Entry::Reselect {
                of: vec!["k4".into()],
                site: 5,
                from: 0,
                chosen: None,
                skipped: vec![Skipped { k: 0, p: 0.6 }, Skipped { k: 1, p: 0.4 }],
            },
            "reselect",
            DutyClass::Another,
        ),
    ];
    for (e, name, class) in 旧 {
        let d = Duty::from_legacy(&e, at(), vec![mark()]).expect("是去向事件");
        assert_eq!(d.form.name(), name);
        assert_eq!(d.form.class(), class);
        assert_eq!(d.attempt, at());
        assert_eq!(d.marks, vec![mark()]);
        assert_eq!(d.to_legacy(), e, "转回与原事件相等");
        // 经线上一趟也无损
        let wire: Entry =
            serde_json::from_str(&serde_json::to_string(&Entry::Duty(d.clone())).unwrap()).unwrap();
        assert_eq!(wire, Entry::Duty(d));
    }
    assert!(
        Duty::from_legacy(
            &Entry::Stop {
                attempt: at(),
                cause: StopCause::Budget
            },
            at(),
            vec![]
        )
        .is_none()
    );
}

#[test]
fn 欠账记号_六项任一项不同即不同() {
    let base = mark();
    let t = base.token();
    assert_eq!(t, mark().token(), "确定");
    let mut vs = vec![];
    let mut m = base.clone();
    m.frame = FrameKind::Program;
    vs.push(m);
    let mut m = base.clone();
    m.owner = "c2".into();
    vs.push(m);
    let mut m = base.clone();
    m.via = Via::Fit;
    vs.push(m);
    let mut m = base.clone();
    m.nth = 4;
    vs.push(m);
    let mut m = base.clone();
    m.key = "q8".into();
    vs.push(m);
    let mut m = base.clone();
    m.cause = "band".into();
    vs.push(m);
    for v in &vs {
        assert_ne!(v.token(), t, "{v:?}");
    }
}

#[test]
fn 账本头新字段为空不写_有值往返() {
    let h = Header::new(1, 1.0, "m", "r", "h");
    let s = serde_json::to_string(&h).unwrap();
    for k in ["\"trace\"", "\"segments\"", "\"key_version\"", "\"schema\""] {
        assert!(!s.contains(k), "为空不写 {k}：{s}");
    }
    let mut h2 = h.clone();
    h2.compared.trace = Some(TraceId::parse("0123456789abcdef0123456789abcdef").unwrap());
    h2.compared.segments = vec![
        Segment {
            seg: SpanId::parse("00000000000000aa").unwrap(),
            parent: None,
        },
        Segment {
            seg: SpanId::parse("00000000000000bb").unwrap(),
            parent: Some(SpanId::parse("00000000000000aa").unwrap()),
        },
    ];
    h2.compared.key_version = Some("1".into());
    let s2 = serde_json::to_string(&h2).unwrap();
    assert!(s2.contains(r#""trace":"0123456789abcdef0123456789abcdef","segments":[{"seg":"00000000000000aa"},{"seg":"00000000000000bb","parent":"00000000000000aa"}],"key_version":"1""#), "{s2}");
    let back: Header = serde_json::from_str(&s2).unwrap();
    assert_eq!(back, h2);
}

#[test]
fn 账本头比对取并集_段按前缀比() {
    let seg = |s: &str| Segment {
        seg: SpanId::parse(s).unwrap(),
        parent: None,
    };
    let mut a = Header::new(1, 1.0, "m", "r", "h").compared;
    a.segments = vec![seg("00000000000000aa")];
    let mut b = a.clone();
    b.segments.push(seg("00000000000000bb"));
    for mode in [HeaderCompare::Resume, HeaderCompare::Replay] {
        assert!(a.diff_in(&b, mode).is_empty(), "旧的是新的前缀：不算不同");
        let d = b.diff_in(&a, mode);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "segments");
        let mut c = a.clone();
        c.trace = Some(TraceId::parse("0123456789abcdef0123456789abcdef").unwrap());
        assert_eq!(a.diff_in(&c, mode)[0].0, "trace");
        let mut k = a.clone();
        k.key_version = Some("2".into());
        assert_eq!(a.diff_in(&k, mode)[0].0, "key_version");
    }
    // 空对空：今天的账本头照旧不报
    let e = Header::new(1, 1.0, "m", "r", "h").compared;
    assert!(e.diff_in(&e.clone(), HeaderCompare::Resume).is_empty());
}

#[test]
fn 余额记录_旧名depth_at照读_写出hop与round() {
    let v4: CarryRecord = serde_json::from_str(
        r#"{"calls":2,"cost":0.0,"latency_p95":null,"escalate":0,"depth_at":3,"depth_cap":256}"#,
    )
    .unwrap();
    assert_eq!((v4.hop, v4.round, v4.depth_cap), (3, 0, 256));
    assert_eq!(
        serde_json::to_string(&v4).unwrap(),
        r#"{"calls":2,"cost":0.0,"latency_p95":null,"escalate":0,"hop":3,"round":0,"depth_cap":256}"#
    );
    // 新旧两个名字同时出现不收
    assert!(
        serde_json::from_str::<CarryRecord>(
            r#"{"calls":2,"cost":0.0,"latency_p95":null,"escalate":0,"depth_at":3,"hop":3,"depth_cap":256}"#
        )
        .is_err()
    );
}

fn 空头(v: u32) -> String {
    format!("{{\"version\":{v},\"header\":null}}\n")
}

#[test]
fn 版本闸_v3v4照读_v6报更新() {
    // B0630 起 v6（结构化站点）；v3、v4、v5 照读，v7 起报更新
    assert_eq!(LEDGER_VERSION, 6);
    for v in [3, 4, 5, 6] {
        assert!(Ledger::decode(&空头(v)).is_ok(), "v{v} 照读");
    }
    for v in [7, 9] {
        let e = Ledger::decode(&空头(v)).expect_err("更新的版本拒读");
        assert!(e.starts_with("E-ledger-newer"), "{e}");
    }
    // 新版本的头即使多出本二进制不认识的字段，也先报「更新」
    let e = Ledger::decode("{\"version\":7,\"header\":null,\"schema_extra\":1}\n").unwrap_err();
    assert!(e.starts_with("E-ledger-newer"), "{e}");
}

/// 头行加一行条目（`entry` 为原文），链接好。
fn 链(version: u32, entry: &str, 换行: bool) -> String {
    let head = format!("{{\"version\":{version},\"header\":null}}");
    let line = format!(
        r#"{{"seq":1,"prev":"{}","entry":{entry}}}"#,
        line_hash(&head)
    );
    format!("{head}\n{line}{}", if 换行 { "\n" } else { "" })
}

#[test]
fn 不认识的条目种类_报行号与种类_不静默跳过() {
    // CarryCap 先例：同版本内出现本二进制不认识的变体（更新的二进制写的）
    let e =
        Ledger::decode(&链(5, r#"{"FutureEvent":{"x":1}}"#, true)).expect_err("不认识的种类拒收");
    assert!(e.starts_with("E-ledger-corrupt: 第 2 行读不成"), "{e}");
    assert!(
        e.contains("FutureEvent") && e.contains("更新的二进制"),
        "{e}"
    );
    // 例外（B55 现行规则，预注册 P7 写明）：末行没有换行时按半写截断，只报截断
    let (l, t) = Ledger::decode(&链(5, r#"{"FutureEvent":{"x":1}}"#, false)).unwrap();
    assert!(l.entries.is_empty() && t.is_some());
    // 删掉的 `Halt`：v4 账本里若有，指出行号与种类（仓库里 359 份旧账本一份都没有）
    let e = Ledger::decode(&链(4, r#"{"Halt":{"reason":"budget","layer":1}}"#, true)).unwrap_err();
    assert!(
        e.starts_with("E-ledger-corrupt: 第 2 行读不成") && e.contains("Halt"),
        "{e}"
    );
}

#[test]
fn v4账本头的余额照读() {
    let text = "{\"version\":4,\"header\":{\"budget\":{\"calls\":1,\"cost\":1.0},\"compared\":{\"model_id\":\"m\",\"render_version\":\"r\",\"handler_version\":\"h\",\"profile_hash\":null,\"behavior_hash\":null,\"calib_hash\":null,\"lib_version\":null,\"bank_version\":null,\"ir_version\":null,\"entry_hash\":null},\"carry\":{\"calls\":2,\"cost\":0.0,\"latency_p95\":null,\"escalate\":0,\"depth_at\":1,\"depth_cap\":8},\"carry_from\":0}}\n";
    let (l, _) = Ledger::decode(text).unwrap();
    let c = l.header.unwrap().carry.unwrap();
    assert_eq!((c.hop, c.round, c.depth_cap), (1, 0, 8));
}

/// V5 复核：`reselect` 在 v4 没有原因，转成 `Duty` 后不写 `"cause":""`；`identity` 为空不写。
#[test]
fn 没有原因的duty与没有身份的题_为空不写() {
    let e = Entry::Reselect {
        of: vec!["k4".into()],
        site: 5,
        from: 0,
        chosen: None,
        skipped: vec![],
    };
    let d = Duty::from_legacy(&e, at(), vec![]).unwrap();
    assert_eq!(d.cause, None);
    assert_eq!(
        serde_json::to_string(&Entry::Duty(d.clone())).unwrap(),
        r#"{"Duty":{"attempt":{"program":"P","n":2},"of":["k4"],"site":5,"form":{"reselect":{"from":0}}}}"#
    );
    assert_eq!(d.to_legacy(), e);
    let q = FlushQuestion {
        key: "q".into(),
        requesters: vec!["P".into()],
        payer: "P".into(),
        identity: None,
    };
    assert_eq!(
        serde_json::to_string(&q).unwrap(),
        r#"{"key":"q","requesters":["P"],"payer":"P"}"#
    );
}
