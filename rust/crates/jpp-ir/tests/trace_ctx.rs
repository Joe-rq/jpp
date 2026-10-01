//! C-2：追踪上下文 `TraceCtx` 的推导与文本形式（预注册 `地基/过程记录/工程-C2-追踪编号.md` §十，预测 C8）。
use jpp_ir::key::{SpanId, TraceCtx, TraceId};

#[test]
fn 起点与推导是确定的() {
    let a = TraceCtx::start("种子", "ping");
    let b = TraceCtx::start("种子", "ping");
    assert_eq!(a, b, "同样的种子与标签得到同样的上下文");
    assert_eq!(a.parent, None);
    assert_eq!(a.trace.as_str().len(), 32);
    assert_eq!(a.span.as_str().len(), 16);
    assert_ne!(a, TraceCtx::start("另一个种子", "ping"));
    assert_ne!(
        a.span,
        TraceCtx::start("种子", "pong").span,
        "标签不同段不同"
    );
    assert_eq!(
        a.trace,
        TraceCtx::start("种子", "pong").trace,
        "追踪编号只看种子"
    );

    let c = a.enter("pong");
    assert_eq!(c, a.enter("pong"));
    assert_eq!(c.trace, a.trace, "追踪编号沿用");
    assert_eq!(c.parent.as_ref(), Some(&a.span), "父段 = 调用者的段");
    assert_ne!(c.span, a.span);
    assert_ne!(c.span, a.enter("pong-2").span, "标签不同是不同的段");
    assert_ne!(c.span, c.enter("pong").span, "不同的调用者得到不同的段");
}

#[test]
fn json形状_没有父段不写parent_键序固定() {
    let root = TraceCtx::start("s", "l");
    let j = serde_json::to_string(&root).unwrap();
    assert!(
        j.starts_with(r#"{"trace":""#) && j.contains(r#"","span":""#) && !j.contains("parent"),
        "{j}"
    );
    // 预注册 A2：根段整个 `,"trace":{…}` 共 79 字节
    assert_eq!(format!(",\"trace\":{j}").len(), 79);
    let child = root.enter("x");
    let jc = serde_json::to_string(&child).unwrap();
    assert!(jc.contains(r#""parent":""#), "{jc}");
    assert_eq!(format!(",\"trace\":{jc}").len(), 107);
    let back: TraceCtx = serde_json::from_str(&jc).unwrap();
    assert_eq!(back, child);
}

#[test]
fn traceparent往返_调用者的上下文() {
    let ctx = TraceCtx::start("s", "l").enter("m");
    let tp = ctx.to_traceparent();
    assert!(tp.starts_with("00-") && tp.ends_with("-01"), "{tp}");
    let back = TraceCtx::from_traceparent(&tp).unwrap();
    assert_eq!(back.trace, ctx.trace);
    assert_eq!(back.span, ctx.span);
    assert_eq!(back.parent, None, "traceparent 里没有调用者自己的父段");
    // 用它推导被调用段，与直接从原上下文推导的结果只差父段的父段（不进推导）
    assert_eq!(back.enter("n").span, ctx.enter("n").span);
    assert_eq!(back.enter("n").parent, ctx.enter("n").parent);
}

#[test]
fn 不合格的编号被拒收_不兜底() {
    let ok_t = "0123456789abcdef0123456789abcdef";
    let ok_s = "0123456789abcdef";
    assert!(TraceId::parse(ok_t).is_ok());
    assert!(SpanId::parse(ok_s).is_ok());
    for bad in [
        "",
        "abc",
        &ok_t[..31],
        &format!("{ok_t}0"),
        "0123456789ABCDEF0123456789ABCDEF",
        "0123456789abcdef0123456789abcdeg",
        &"0".repeat(32),
    ] {
        assert!(TraceId::parse(bad).is_err(), "{bad:?}");
    }
    assert!(SpanId::parse(&"0".repeat(16)).is_err());
    assert!(SpanId::parse(&ok_s[..15]).is_err());
    for bad in [
        format!("01-{ok_t}-{ok_s}-01"),
        format!("00-{ok_t}-{ok_s}"),
        format!("00-{ok_t}-{ok_s}-1"),
        format!("00-{ok_t}-{ok_s}-zz"),
        format!("00-{}-{ok_s}-01", "0".repeat(32)),
        format!("00-{ok_t}-{}-01", "0".repeat(16)),
        format!("00-{ok_t}-{ok_s}-01-extra"),
        String::new(),
    ] {
        assert!(TraceCtx::from_traceparent(&bad).is_err(), "{bad:?}");
    }
    // 反序列化同样拒收
    assert!(serde_json::from_str::<TraceCtx>(r#"{"trace":"x","span":"y"}"#).is_err());
    assert!(
        serde_json::from_str::<TraceCtx>(&format!(
            r#"{{"trace":"{ok_t}","span":"{ok_s}","extra":1}}"#
        ))
        .is_err(),
        "未知字段拒收"
    );
}
