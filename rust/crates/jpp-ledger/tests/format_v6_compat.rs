//! B0630 格式版本与兼容读取（预注册 `规划/B0630-键与格式稳定-预注册.md` §四、§2.3；k-fmt）。
//!
//! 钉的是：旧版本账本（头里没有 `key_version`）照读不崩；新账本头的 `key_version` 读写无损、旧键法不写字段
//! （账本逐字节不变）；格式版本高于本二进制报 `E-ledger-newer`、不 panic；`key_version::choose` 的场合表
//! （新跑、审计重放、续接、不认识的值）。
//!
//! 写法不依赖 `LEDGER_VERSION` 与 `KEY_VERSION_CURRENT` 的具体取值：键法切到结构化键（line-key 合入）前后
//! 同一份测试都成立，凡是随当前键法变的断言按常量分支。
use jpp_ledger::key_version::{KEY_VERSION_CURRENT, KeyVersion, choose};
use jpp_ledger::{Header, LEDGER_READS_AS_IS, LEDGER_VERSION, Ledger, encode_head};

fn 空头(v: u64) -> String {
    format!("{{\"version\":{v},\"header\":null}}\n")
}

fn 头(kv: Option<&str>) -> Header {
    Header::new(1, 1.0, "m", "r2", "h").with_key_version(kv.map(Into::into))
}

#[test]
fn 旧版本照读_新于本二进制的报更新不崩() {
    for v in LEDGER_READS_AS_IS.iter().copied().chain([LEDGER_VERSION]) {
        assert!(Ledger::decode(&空头(v as u64)).is_ok(), "v{v} 照读");
    }
    for v in [
        LEDGER_VERSION as u64 + 1,
        LEDGER_VERSION as u64 + 2,
        99,
        u32::MAX as u64,
    ] {
        let e = Ledger::decode(&空头(v)).expect_err("更新的版本拒读");
        assert!(e.starts_with("E-ledger-newer"), "{e}");
        assert!(
            e.contains(&format!("v{v}")) && e.contains(&format!("v{LEDGER_VERSION}")),
            "报文写出双方版本：{e}"
        );
        assert!(e.contains("不带账本重跑"), "报文写出修法：{e}");
    }
    // 更新的版本头里多出不认识的字段、条目也读不了的情形：照样先报「更新」，不报 corrupt、不 panic
    let t = format!(
        "{{\"version\":{},\"header\":{{\"future\":1}}}}\n{{\"seq\":1,\"prev\":\"x\",\"entry\":{{\"Future\":{{}}}}}}\n",
        LEDGER_VERSION + 1
    );
    let e = Ledger::decode(&t).unwrap_err();
    assert!(e.starts_with("E-ledger-newer"), "{e}");
}

#[test]
fn 头行版本不是整数或为空_报错不崩() {
    for t in [
        "{\"version\":\"6\",\"header\":null}\n",
        "{\"header\":null}\n",
        "{\"version\":-1,\"header\":null}\n",
        "",
        "\n",
    ] {
        let e = Ledger::decode(t).expect_err(t);
        assert!(e.starts_with("E-ledger-"), "{t:?} → {e}");
    }
}

#[test]
fn 旧账本头没有key_version_读成缺省_重编码逐字节不变() {
    // 旧账本头：没有 key_version 字段（B0630 之前所有账本）
    let old = r#"{"version":5,"header":{"budget":{"calls":1,"cost":1.0},"compared":{"model_id":"m","render_version":"r2","handler_version":"h","profile_hash":null,"behavior_hash":null,"calib_hash":null,"lib_version":null,"bank_version":null,"ir_version":null,"entry_hash":null}}}"#;
    let (l, t) = Ledger::decode(&format!("{old}\n")).unwrap();
    assert!(t.is_none());
    assert_eq!(l.header.as_ref().unwrap().compared.key_version, None);
    assert_eq!(
        KeyVersion::from_header(None),
        Ok(KeyVersion::Offset),
        "缺省 = 旧键法"
    );
    // 头的内容逐字节不变；头行版本是本二进制写的格式版本（B0630 起 6）
    assert_eq!(
        encode_head(&l.header),
        old.replacen("\"version\":5", &format!("\"version\":{LEDGER_VERSION}"), 1),
        "旧账本头重编码逐字节不变"
    );
}

#[test]
fn key_version读写无损_旧键法不写字段() {
    let h0 = 头(None);
    assert!(
        !encode_head(&Some(h0.clone())).contains("key_version"),
        "为空不写"
    );
    let h1 = 头(Some("1"));
    let s = encode_head(&Some(h1.clone()));
    assert!(s.contains(r#""key_version":"1""#), "{s}");
    let (l, _) = Ledger::decode(&format!("{s}\n")).unwrap();
    assert_eq!(l.header, Some(h1.clone()));
    // 键法不同在两种场合都进比对集合
    for mode in [
        jpp_ledger::HeaderCompare::Resume,
        jpp_ledger::HeaderCompare::Replay,
    ] {
        let d = h0.compared.diff_in(&h1.compared, mode);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].0, "key_version");
    }
    // 头里写不认识的值：解码照读（字符串），由运行时入口报 E-key-version
    let s9 = encode_head(&Some(头(Some("9"))));
    let (l9, _) = Ledger::decode(&format!("{s9}\n")).unwrap();
    assert_eq!(
        l9.header.unwrap().compared.key_version.as_deref(),
        Some("9")
    );
}

#[test]
fn 头值与键法互转() {
    assert_eq!(
        KeyVersion::from_header(Some("1")),
        Ok(KeyVersion::Structured)
    );
    assert_eq!(KeyVersion::Offset.header_tag(), None);
    assert_eq!(KeyVersion::Structured.header_tag(), Some("1"));
    for bad in ["0", "2", "9", "", "1.0", " 1", "一"] {
        let e = KeyVersion::from_header(Some(bad)).unwrap_err();
        assert!(e.starts_with("E-key-version"), "{bad:?} → {e}");
        assert!(e.contains(&format!("{bad:?}")), "报出那个值：{e}");
    }
}

#[test]
fn 场合表_不认识的值任何场合都拒() {
    for audit in [false, true] {
        for entries in [false, true] {
            let e = choose(Some("9"), entries, audit).unwrap_err();
            assert!(e.starts_with("E-key-version"), "{e}");
        }
    }
}

#[test]
fn 场合表_新跑取当前键法() {
    // 没有账本或账本为空：不看头里记的键法（空账本没有键可对）
    for audit in [false, true] {
        assert_eq!(choose(None, false, audit), Ok(KEY_VERSION_CURRENT));
        assert_eq!(choose(Some("1"), false, audit), Ok(KEY_VERSION_CURRENT));
    }
}

#[test]
fn 场合表_头里的键法与当前相同照用() {
    for audit in [false, true] {
        assert_eq!(
            choose(KEY_VERSION_CURRENT.header_tag(), true, audit),
            Ok(KEY_VERSION_CURRENT)
        );
    }
}

#[test]
fn 场合表_旧键法账本() {
    // 审计重放：能算旧键法就按账本的算（写回的头也写它），不能算报错
    let r = choose(None, true, true);
    if KeyVersion::Offset.supported() {
        assert_eq!(r, Ok(KeyVersion::Offset));
    } else {
        assert!(r.unwrap_err().starts_with("E-key-version"));
    }
    // 续接：当前键法也是旧键法就照常；否则拒绝，报文写修法（--replay、--cache）
    let r = choose(None, true, false);
    if KEY_VERSION_CURRENT == KeyVersion::Offset {
        assert_eq!(r, Ok(KeyVersion::Offset));
    } else {
        let e = r.unwrap_err();
        assert!(e.starts_with("E-key-version"), "{e}");
        assert!(e.contains("--replay") && e.contains("--cache"), "{e}");
        assert!(e.contains("key_version 缺省"), "{e}");
    }
}

#[test]
fn 场合表_结构化键账本() {
    let r = choose(Some("1"), true, true);
    if KeyVersion::Structured.supported() {
        assert_eq!(r, Ok(KeyVersion::Structured));
    } else {
        assert!(r.unwrap_err().starts_with("E-key-version"));
    }
    let r = choose(Some("1"), true, false);
    if KEY_VERSION_CURRENT == KeyVersion::Structured {
        assert_eq!(r, Ok(KeyVersion::Structured));
    } else {
        assert!(r.unwrap_err().starts_with("E-key-version"));
    }
}

#[test]
fn 当前键法本二进制能算() {
    assert!(
        KEY_VERSION_CURRENT.supported(),
        "新写的账本自己必须读得回去"
    );
}

/// v6 账本的判断条目里 `jkey.site` 是串（结构化站点，B0630）：照读，`SiteRef::Path` 原样回来，再编码逐字节不变；
/// 同一条目把站点换成数（旧键法）也照读，读成 `SiteRef::Offset`。
#[test]
fn v6账本_jkey站点为串与为数都照读() {
    use jpp_ir::key::SiteRef;
    use jpp_ledger::{Entry, JudgeKey};
    let mut l = Ledger::new();
    l.set_header(头(KEY_VERSION_CURRENT.header_tag()));
    for (i, site) in [
        SiteRef::Path("purpose_land/λ2:judge#1".into()),
        SiteRef::Offset(107140),
    ]
    .into_iter()
    .enumerate()
    {
        let mut e = Entry::judge(
            format!("k{i}"),
            jpp_value::value::Answer::Noul(0.9),
            0,
            0.0,
            "m",
            1,
        );
        if let Entry::Judge { jkey, .. } = &mut e {
            *jkey = Some(JudgeKey::new("m", "s", "q", "noul", 0, 0, site));
        }
        l.put(e);
    }
    let text = l.encode();
    assert!(
        text.contains(r#""site":"purpose_land/λ2:judge#1""#),
        "{text}"
    );
    assert!(text.contains(r#""site":107140"#), "{text}");
    let (back, _) = Ledger::decode(&text).expect("v6 照读");
    let sites: Vec<SiteRef> = back
        .entries
        .iter()
        .filter_map(|e| match e {
            Entry::Judge { jkey: Some(k), .. } => Some(k.site.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sites,
        vec![
            SiteRef::Path("purpose_land/λ2:judge#1".into()),
            SiteRef::Offset(107140)
        ]
    );
    assert_eq!(back.encode(), text, "再编码逐字节不变");
}
