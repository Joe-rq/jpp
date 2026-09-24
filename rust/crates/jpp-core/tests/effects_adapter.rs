//! 步 9：旧 `Client` 经默认方法接到 `jpp-effects` 的效应实例；原路径的画像类型与 `jpp-effects` 是同一个类型。

use jpp_core::effects::{Client, FixedClient, NoCallClient, Profile, Tri};

#[test]
fn 客户端按效应给出实例() {
    let c = FixedClient::default();
    let v = c.instances();
    let names: Vec<_> = v.iter().map(|i| jpp_effects::spec(i.effect).name).collect();
    assert_eq!(names, ["judge", "gen", "ask"]);
    assert!(v.iter().all(|i| i.model == c.model_id()));
    assert_eq!(NoCallClient.instances().len(), 3);
}

#[test]
fn 原路径的画像就是新crate的画像() {
    let p: jpp_effects::Profile = Profile::default();
    assert_eq!(p.arithmetic_capable, jpp_effects::Tri::未测);
    let _: Tri = p.arithmetic_capable;
}
