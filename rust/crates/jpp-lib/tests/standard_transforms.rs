//! Z0222：标准库宿主变换表里各变换的「读外部状态，不跨运行缓存」位。

use jpp_lib::standard_transforms;

#[test]
fn 料库标记读写带位_诊断不带() {
    let t = standard_transforms(None);
    assert!(t.get("mat_marks").unwrap().reads_external_state);
    assert!(t.get("mat_mark").unwrap().reads_external_state);
    assert!(!t.get("diagnose").unwrap().reads_external_state);
}
