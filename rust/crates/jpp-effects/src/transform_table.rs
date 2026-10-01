//! 宿主变换表（`12` §2.8 `transform(f: HostFn, args: [Mat]) → Mat`；`20` §2.3 `Ports.transform: &TransformTable`；
//! 步 26 建出，主会话裁定 2026-09-29 第二十条）。
//!
//! `.jpp` 写 `transform("名字", 材料…)`：第一参是登记过的宿主变换名（与 `do("graph:…")` 同一写法，不新增值类型），
//! 运行时按名字取宿主函数，对各材料的内容求值，产物是材料。记账与闭包形式的 `transform` 完全相同（输入输出进账本，
//! 重放取账本），键的方法位是 [`HostTransform::identity`]（名字加版本）。登记者是 `jpp-lib::s_library()`（B47、A-11）。

use std::collections::BTreeMap;

use serde_json::Value as Json;

/// 产物的 taint 声明（`12` §2.11：`transform` 缺省 ∨ args.taint；D14.5 每个 S 库函数带 `taint_out`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostTaint {
    /// ∨ 输入材料的 taint
    Inherit,
    Trusted,
    Untrusted,
}

/// 一个宿主变换：纯函数，输入各材料的内容，输出一份内容。纯性由账本核（`12` §2.8）。
pub struct HostTransform {
    pub name: String,
    /// 版本串：函数行为一变就要改（进键，旧账本上的旧产物不会被当成新函数的产物）
    pub version: String,
    pub taint_out: HostTaint,
    /// 一行参数说明（D14.5），报错时给作者看
    pub doc: String,
    /// 读外部状态，不跨运行缓存：结果随宿主外部状态（如料库当前内容）变，而键里只有名字、版本与输入，
    /// 不含那份状态；另一本账本里同输入的记录拿来当本次的复用，读会读到旧状态、写会被跳过。
    /// 真则 `jpp::store::CacheIndex` 不收它的条目。本运行内的复用与账本重放不经跨运行索引，不受影响。
    /// 这一位不进键、不进账本（Z0222；复用规则只在注册表里定，B74、B151）。
    pub reads_external_state: bool,
    #[allow(clippy::type_complexity)]
    pub run: Box<dyn Fn(&[Json]) -> Result<Json, String>>,
}

impl HostTransform {
    /// 键的方法位：`host:<名字>@<版本>`
    pub fn identity(&self) -> String {
        format!("host:{}@{}", self.name, self.version)
    }
}

/// 宿主变换表：名字 → 宿主函数。
#[derive(Default)]
pub struct TransformTable {
    fns: BTreeMap<String, HostTransform>,
}

impl TransformTable {
    pub fn new() -> TransformTable {
        TransformTable::default()
    }

    /// 登记一个宿主变换；同名再登记即替换。
    pub fn register(&mut self, t: HostTransform) {
        self.fns.insert(t.name.clone(), t);
    }

    pub fn get(&self, name: &str) -> Option<&HostTransform> {
        self.fns.get(name)
    }

    /// 按键的方法位（[`HostTransform::identity`]，`host:<名字>@<版本>`）找；账本条目里只有这个串。
    /// 名字对、版本不同的不算找到（旧账本上旧版本产物的语义位，以登记时的版本为准）。
    pub fn by_identity(&self, identity: &str) -> Option<&HostTransform> {
        self.fns.values().find(|t| t.identity() == identity)
    }

    /// 已登记的名字（报错时列出可用的）
    pub fn names(&self) -> Vec<&str> {
        self.fns.keys().map(|k| k.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 变换(名: &str, 版本: &str, 读外部: bool) -> HostTransform {
        HostTransform {
            name: 名.into(),
            version: 版本.into(),
            taint_out: HostTaint::Inherit,
            doc: String::new(),
            reads_external_state: 读外部,
            run: Box::new(|_| Ok(Json::Null)),
        }
    }

    #[test]
    fn 按方法位找变换_版本不同不算找到() {
        let mut t = TransformTable::new();
        t.register(变换("a", "1", false));
        t.register(变换("b", "2", true));
        assert!(!t.by_identity("host:a@1").unwrap().reads_external_state);
        assert!(t.by_identity("host:b@2").unwrap().reads_external_state);
        assert!(t.by_identity("host:b@1").is_none(), "版本不同");
        assert!(t.by_identity("闭包哈希").is_none());
    }
}
