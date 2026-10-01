//! 单元的身份与键（`12` §2.13 R1；B193）。
//!
//! 五种单元各有自己的键空间，键带种类，不同种类的键不会相等（原型里作者手写的代码单元键与程序标识共用
//! 一个命名空间，撞过车，过程记录第 20 节）。
//!
//! - 代码单元：`(方法身份, 实参内容哈希)`（B193 第 1 条）。方法身份按 `13` §4：函数的结构哈希
//!   （`ir::Function.source_hash`）、实际捕获状态的指纹、`lib_version`；与 `transform` 的键同一组成分。
//!   哪些调用成为代码单元由检查器的纯性判定给出（C2），这里只算键。作者手写键的写法不进语言。
//! - 判断单元：题与材料的内容键，就是现有缓存键的摘要（R1、B40）。
//! - 源单元：宿主给的键。
//! - 程序单元：常驻程序 = `resident` 声明里的名字；一次输入一次返回的程序 = 入口方法身份 + 段编号（B193 第 4 条）。
//! - 多写者单元：声明时给的名字（B195 `cell <名> reducer <归约器>`）。
//!
//! 全部可以只从 IR 与实参算出（B193 第 5 条），所以放 L0。

use crate::key::{CacheKey, hash_of};
use serde::{Deserialize, Serialize};

/// 单元的五种（R1）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellKind {
    Source,
    Code,
    Judge,
    Program,
    Shared,
}

impl CellKind {
    pub fn tag(self) -> &'static str {
        match self {
            CellKind::Source => "src",
            CellKind::Code => "code",
            CellKind::Judge => "judge",
            CellKind::Program => "prog",
            CellKind::Shared => "shared",
        }
    }
}

/// 单元键：种类加种类内的标识。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CellKey {
    pub kind: CellKind,
    pub id: String,
}

impl std::fmt::Display for CellKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind.tag(), self.id)
    }
}

/// 方法身份（`13` §4）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct MethodIdentity {
    /// 函数的结构哈希（`ir::Function.source_hash`）
    pub source_hash: String,
    /// 实际捕获状态的指纹（运行时算；没有捕获时为空串）
    pub captured: String,
    /// 标准库版本（B48；宿主没装载时为 `None`）
    pub lib_version: Option<String>,
}

/// 代码单元的键：`(方法身份, 实参内容哈希)`。实参按位置排列。
pub fn code_cell_key(m: &MethodIdentity, arg_hashes: &[&str]) -> CellKey {
    let lib = m.lib_version.as_deref().unwrap_or("");
    let n = arg_hashes.len().to_string();
    // 各实参是 `hash_of` 的独立分量（分量之间有分隔符），实参个数也进键，拼接不会让两组实参撞到一起
    let mut parts: Vec<&str> = vec!["cell/code", &m.source_hash, &m.captured, lib, &n];
    parts.extend_from_slice(arg_hashes);
    CellKey {
        kind: CellKind::Code,
        id: hash_of(&parts),
    }
}

/// 判断单元的键：缓存键的摘要（同模型、同状态、同题、同物理形式即同一次观察，B40）。
pub fn judge_cell_key(k: &CacheKey) -> CellKey {
    CellKey {
        kind: CellKind::Judge,
        id: k.digest(),
    }
}

/// 账本求值体的代码单元的键（B199 第 2 条：`gen`、`transform` 的输出是求值体为账本条目的代码单元）：效应的可复用
/// 缓存键摘要（`EffectKey::cache_digest`，不含调用位置）加前缀（C2b）。
pub fn effect_cell_key(cache_digest: &str) -> CellKey {
    CellKey {
        kind: CellKind::Code,
        id: hash_of(&["cell/effect", cache_digest]),
    }
}

/// 源单元的键：宿主给的键。
pub fn source_cell_key(host_key: &str) -> CellKey {
    CellKey {
        kind: CellKind::Source,
        id: host_key.to_string(),
    }
}

/// 多写者单元的键：声明名。
pub fn shared_cell_key(name: &str) -> CellKey {
    CellKey {
        kind: CellKind::Shared,
        id: name.to_string(),
    }
}

/// 程序单元的身份（B193 第 4 条）。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ProgramIdentity {
    /// 常驻程序：`resident <名>` 的名字，同一追踪内唯一（重名报 `E-program-dup`，由检查器与引擎报）
    Resident { name: String },
    /// 一次输入一次返回的程序：入口方法身份（结构哈希）加段编号
    OneShot { entry: String, segment: u32 },
}

/// 程序单元的键。常驻程序直接用名字（它是作者在声明里给的、跨追踪稳定的名字）；一次性程序取哈希。
pub fn program_cell_key(p: &ProgramIdentity) -> CellKey {
    let id = match p {
        ProgramIdentity::Resident { name } => name.clone(),
        ProgramIdentity::OneShot { entry, segment } => {
            hash_of(&["cell/prog", entry, &segment.to_string()])
        }
    };
    CellKey {
        kind: CellKind::Program,
        id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(src: &str, cap: &str, lib: Option<&str>) -> MethodIdentity {
        MethodIdentity {
            source_hash: src.into(),
            captured: cap.into(),
            lib_version: lib.map(Into::into),
        }
    }

    #[test]
    fn 代码单元键由方法身份与实参决定() {
        let a = code_cell_key(&m("f", "", None), &["x", "y"]);
        assert_eq!(
            a,
            code_cell_key(&m("f", "", None), &["x", "y"]),
            "同方法同实参即同单元"
        );
        assert_ne!(
            a,
            code_cell_key(&m("f", "", None), &["y", "x"]),
            "实参按位置"
        );
        assert_ne!(
            a,
            code_cell_key(&m("g", "", None), &["x", "y"]),
            "函数体不同"
        );
        assert_ne!(
            a,
            code_cell_key(&m("f", "c1", None), &["x", "y"]),
            "捕获状态不同（13 §4）"
        );
        assert_ne!(
            a,
            code_cell_key(&m("f", "", Some("L1")), &["x", "y"]),
            "库版本不同"
        );
        // 各实参是 `hash_of` 的独立分量（分隔符 `\x1f`），键里另有实参个数：把两个实参拼成一个串不会与它们撞键。
        // 这里拼接用的是逗号，没碰到真实分隔符；实参是十六进制哈希，本来就不含 `\x1f`，靠的主要是个数进键。
        assert_ne!(
            code_cell_key(&m("f", "", None), &["x\u{1f}y"]),
            a.clone(),
            "实参个数进键，拼接不撞"
        );
        assert_ne!(code_cell_key(&m("f", "", None), &["x,y"]), a.clone());
        assert_eq!(a.kind, CellKind::Code);
    }

    #[test]
    fn 键带种类_同名不撞() {
        let p = program_cell_key(&ProgramIdentity::Resident { name: "X".into() });
        let s = source_cell_key("X");
        let w = shared_cell_key("X");
        assert_ne!(p, s);
        assert_ne!(p, w);
        assert_eq!(p.to_string(), "prog:X");
        assert_ne!(
            program_cell_key(&ProgramIdentity::OneShot {
                entry: "e".into(),
                segment: 1
            }),
            program_cell_key(&ProgramIdentity::OneShot {
                entry: "e".into(),
                segment: 2
            })
        );
    }

    #[test]
    fn 判断单元键就是缓存键摘要() {
        let jk = crate::key::JudgeKey::new("m", "s", "q", "yes", 0, 3, 17);
        let ck = jk.cache_key();
        assert_eq!(judge_cell_key(&ck).id, ck.digest());
        let other_site = crate::key::JudgeKey::new("m", "s", "q", "yes", 0, 9, 99);
        assert_eq!(
            judge_cell_key(&other_site.cache_key()),
            judge_cell_key(&ck),
            "调用位置不进内容键"
        );
    }

    #[test]
    fn 账本单元键按效应缓存键摘要_与判断键不撞() {
        let a = effect_cell_key("d1");
        assert_eq!(a, effect_cell_key("d1"));
        assert_ne!(a, effect_cell_key("d2"));
        assert_eq!(a.kind, CellKind::Code);
        assert_ne!(a.id, "d1", "带前缀，与缓存键摘要本身不同");
    }
}
