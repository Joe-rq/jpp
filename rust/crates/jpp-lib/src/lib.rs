//! J++ S 库的 Rust 侧（L5，`20` §2.1、§2.3「`jpp-lib`」；`21` 步 26，A-11；主会话裁定 2026-09-29 第十四、十五条）。
//!
//! **本 crate 隐藏的设计决定**：标准库的版本怎么算（`lib_version`），以及哪些宿主函数随标准库注册
//! （本版只有诊断闸门 `diagnose`，B47；其余 S 库函数不在本条搬）。
//!
//! `lib/*.jpp` 仍是文件，由作者 `import`；本 crate 不装载它们，只按宿主交来的已装载文件算版本。
//!
//! 依赖（`20` §2.2 第 1 条）：`jpp-lib → ir, value, effects, check`；本版实际用到 `ir`、`effects`、`check`。

/// 诊断规则集的版本（`jpp-check::diag::RULES_VERSION`）；改任何一条诊断规则都要改它（裁定第十五条）。
pub use jpp_check::diag::RULES_VERSION;

mod marks;
pub use marks::{Mark, MarkStore, mat_mark_transform, mat_marks_transform, premise_hash};

use jpp_check::diag::RuntimeGate;
use jpp_effects::{HostTaint, HostTransform, TransformTable};
use jpp_ir::diag_gate::{GateQuestion, QuestionGate};
use jpp_ir::ir::Program;
use serde_json::{Value as Json, json};
use std::rc::Rc;

/// 随标准库注册给运行时的宿主侧（`20` §2.3：`s_library()` 含 `diagnose` 包装，B47）：
/// - 宿主变换表：本版只有 `diagnose`（`.jpp` 经 `transform("diagnose", 题材料)` 调用，记账，裁定第二十条）；
/// - 运行期闸门：登记判断处对检查期没见过的题只提示、不记账（裁定第二十条「只提示的不记」）。
///
/// 运行时经 `jpp_effects::TransformTable` 与 `jpp_ir::diag_gate::QuestionGate` 看见它们，不依赖 `jpp-check`。
pub struct SLibrary {
    gate: RuntimeGate,
    transforms: TransformTable,
}

impl SLibrary {
    /// 交给 `Interp::set_gate` 的运行期闸门
    pub fn gate(&self) -> &dyn QuestionGate {
        &self.gate
    }
    /// 交给 `Interp::set_transforms` 的宿主变换表
    pub fn transforms(&self) -> &TransformTable {
        &self.transforms
    }
}

/// 为一个程序构造 S 库的宿主侧（A-11：注册点在 `jpp-lib::s_library()`，主会话裁定 2026-09-29 第十四条）。
/// 要程序是因为运行期闸门先收下检查器诊断过的字面题，运行期不重复报（`jpp-check::diag::gate` 头注）。
pub fn s_library(program: &Program) -> SLibrary {
    s_library_with(program, None)
}

/// [`s_library`]，另可挂料库标记存储（步 29，B20 第 1 种）：`mat_marks`、`mat_mark` 两个变换
/// （`lib/skeletons/select.jpp` 用）一直注册，给了存储才真读真写，不给则读为空、写为空操作（返回 `stored: false`）。
pub fn s_library_with(program: &Program, marks: Option<Rc<dyn MarkStore>>) -> SLibrary {
    SLibrary {
        gate: RuntimeGate::new(program),
        transforms: standard_transforms(marks),
    }
}

/// 标准库的宿主变换表（登记处；`s_library_with` 与跨运行缓存索引共用这一份）。每个变换的
/// `reads_external_state` 在各自的构造函数里定；这一位只取决于变换本身，与有没有挂存储无关，
/// 所以只想查语义位的一方（`jpp::store::CacheIndex::build`）传 `None` 即可。
pub fn standard_transforms(marks: Option<Rc<dyn MarkStore>>) -> TransformTable {
    let mut transforms = TransformTable::new();
    transforms.register(diagnose_transform());
    transforms.register(mat_marks_transform(marks.clone()));
    transforms.register(mat_mark_transform(marks));
    transforms
}

/// 宿主变换 `diagnose`（B47）：输入一份题材料 `{op, text, template?, fill?}`（`lib/diag.jpp` 的 `diag_mat(q)` 造），
/// 输出 `[{code, message, fix}]`，没有问题为空列表。规则是 `jpp-check::diagnose_question` / `diagnose_fill`
/// 同一份；不论题是不是字面量都诊断。版本 = `RULES_VERSION`，进键：规则一改，旧账本上的旧产物不会被当成新规则的产物。
pub fn diagnose_transform() -> HostTransform {
    HostTransform {
        name: "diagnose".into(),
        version: RULES_VERSION.into(),
        taint_out: HostTaint::Inherit,
        reads_external_state: false,
        doc: "transform(\"diagnose\", 题材料)：题材料是 {op, text, template?, fill?}，lib/diag.jpp 的 diag_mat(q) 给出；返回 [{code, message, fix}]".into(),
        run: Box::new(|ins: &[Json]| {
            let [q] = ins else {
                return Err(format!("要一份题材料，收到 {} 份", ins.len()));
            };
            let text = q
                .get("text")
                .and_then(|t| t.as_str())
                .ok_or("题材料缺 text（题面）")?;
            let op = q.get("op").and_then(|t| t.as_str()).unwrap_or("test");
            let template = q.get("template").and_then(|t| t.as_str());
            let fill: Option<Vec<(String, String)>> = q.get("fill").and_then(|f| f.as_object()).map(|m| {
                m.iter()
                    .map(|(k, v)| {
                        let v = match v {
                            Json::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        (k.clone(), v)
                    })
                    .collect()
            });
            let gq = GateQuestion {
                op,
                text,
                template: if fill.is_some() { template } else { None },
                fill: fill.as_deref(),
            };
            let notes = RuntimeGate::default().diagnose(&gq);
            Ok(Json::Array(
                notes
                    .into_iter()
                    .map(|n| json!({"code": n.code, "message": n.message, "fix": n.fix}))
                    .collect(),
            ))
        }),
    }
}

/// 标准库版本，写进账本头 `lib_version`（`12` §2.10；20-v2 B45「随 `lib_version` 进账本头」「重放靠
/// `lib_version` 核对」）。
///
/// `loaded`：本次运行装载的 `lib/` 下文件，路径相对 `lib/`（例如 `"diag.jpp"`、`"compose/tree.jpp"`），
/// 内容是文件原字节。`bank/` 下的文件不算（题库有自己的 `bank_version`，B48），调用方交进来也会被排除。
///
/// 返回 `"<文件哈希>+diag:<RULES_VERSION>"`：文件部分按（路径，内容）排序后取哈希，与交来的顺序无关；
/// 什么都没装载时文件部分是空集合的哈希，仍带规则版本——运行期闸门每个程序都跑（裁定第十五条）。
/// 诊断规则一改，`RULES_VERSION` 变，`lib_version` 随之变，重放比对账本头报 `W-header: lib_version`。
pub fn lib_version(loaded: &[(String, Vec<u8>)]) -> String {
    lib_version_with(RULES_VERSION, loaded)
}

/// [`lib_version`] 的规则版本可换写法：供测试模拟「改过一次规则」，宿主一律用 [`lib_version`]。
pub fn lib_version_with(rules_version: &str, loaded: &[(String, Vec<u8>)]) -> String {
    let mut files: Vec<(String, String)> = loaded
        .iter()
        .map(|(p, c)| {
            let p = p.replace('\\', "/");
            let p = p.trim_start_matches("./").to_string();
            let h = jpp_ir::key::hash_of(&["lib-file", &String::from_utf8_lossy(c)]);
            (p, h)
        })
        .filter(|(p, _)| !p.starts_with("bank/"))
        .collect();
    files.sort();
    let mut parts: Vec<&str> = vec!["lib"];
    for (p, h) in &files {
        parts.push(p);
        parts.push(h);
    }
    format!("{}+diag:{rules_version}", jpp_ir::key::hash_of(&parts))
}
