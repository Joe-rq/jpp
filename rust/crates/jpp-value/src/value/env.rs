//! 环境链与闭包（`EnvNode`、`env_*`、`Closure`）。步 36 G3 从 `value.rs` 原样搬出（只搬不改）。

use super::*;

/// 显式环境链：名字→值，可打印。
#[derive(Debug)]
pub struct EnvNode {
    pub vars: RefCell<Vec<(String, Value)>>,
    pub parent: Option<Env>,
}
pub type Env = Rc<EnvNode>;

pub fn env_root() -> Env {
    Rc::new(EnvNode {
        vars: RefCell::new(vec![]),
        parent: None,
    })
}
pub fn env_child(parent: &Env) -> Env {
    Rc::new(EnvNode {
        vars: RefCell::new(vec![]),
        parent: Some(parent.clone()),
    })
}

pub fn env_lookup(env: &Env, name: &str) -> Option<Value> {
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        if let Some((_, v)) = e.vars.borrow().iter().rev().find(|(n, _)| n == name) {
            return Some(v.clone());
        }
        cur = e.parent.clone();
    }
    None
}
pub fn env_define(env: &Env, name: &str, v: Value) {
    env.vars.borrow_mut().push((name.to_string(), v));
}
/// 环境的可读形式（只列名字，避免打印巨大值；给 INTERFACE 的「函数值环境表示」）
pub fn env_names(env: &Env) -> Vec<Vec<String>> {
    let mut out = vec![];
    let mut cur = Some(env.clone());
    while let Some(e) = cur {
        out.push(e.vars.borrow().iter().map(|(n, _)| n.clone()).collect());
        cur = e.parent.clone();
    }
    out
}

#[derive(Debug)]
pub struct Closure {
    pub function: Function,
    pub env: Env,
    pub name: Option<String>,
    pub span: Span,
    /// 函数体的结构哈希（transform 的键用）
    pub hash: String,
    /// 创建时经捕获环境可达的未销账未决责任（出口 id；B52，步 21）。只作运行期记账，
    /// 不进 `hash`、不序列化。
    pub captures: Vec<usize>,
    /// Fn¹（B52）：本闭包是其中每条责任的**唯一可达路径**，由创建它的帧返回时判定；非空即 Fn¹。
    pub linear: RefCell<Vec<usize>>,
    /// 判为 Fn¹ 之后是否已被调用过一次（第二次调用是 J-05）
    pub linear_called: Cell<bool>,
}
