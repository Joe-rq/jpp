//! 最简单的文件料库：实现 [`MatStorePort`]，并实现 `jpp_lib::MarkStore` 供 `.jpp` 的 `mat_marks`/`mat_mark` 变换用
//! （步 29，主控:B0460；`21` 步 29；`12` 料库行 B11、B20、B41、B65）。
//!
//! **临时存储，骨架最终裁定后换。** 主会话裁定（`地基/规划/骨架比较/裁定-纸面阶段-2026-09-29.md` 第七节）：
//! 递归筛的库写在现有料库接口之上，料库的存储怎么实现等最终裁定（乙案里它并进单元存储）；这里先用最简单的
//! 文件存储把接口接通，以后会换。所以格式带版本、只求读写正确，不求快：每次读整份文件。
//!
//! 布局（键是 [`Blob`] 的相对路径，目录后端与内存后端都行）：
//! - `mat-store.json`：`{"v": 1, "temporary": true, "note": "…"}`，第一次写入时落下；
//! - `mats/<Mat.hash>.json`：一份 [`Mat`] 的 JSON，内容寻址，同哈希不重写；
//! - `marks/<Mat.hash>.jsonl`：这份材料上的标记，每行一条 [`Mark`]，追加写；同一前提题（`q`）后写的覆盖先写的。
//!
//! `MatStorePort::marks` 返回 `CacheKey`，装不下答案；完整标记走本类型的固有方法（[`FileMatStore::marks_full`]）
//! 与 `MarkStore`。标记不知道当次判断的真实缓存键（`.jpp` 拿不到），所以 `marks()` 给的是标记空间里的伪键：
//! `model_id = "mark"`、`state = 材料哈希`、`q = 前提题哈希`、`phys = "test"`、`render = "mark-v1"`。

use std::cell::RefCell;
use std::path::Path;

use jpp_effects::views::MatStorePort;
use jpp_ir::key::CacheKey;
use jpp_lib::{Mark, MarkStore};
use jpp_value::value::Mat;

use super::blob::{Blob, DirBlob, IoErr};

const META: &str = "mat-store.json";

/// 文件料库（临时存储，骨架最终裁定后换）。
pub struct FileMatStore {
    blob: RefCell<Box<dyn Blob>>,
}

impl FileMatStore {
    /// 架在任意字节存储上（测试用 `MemBlob`）。
    pub fn new(blob: Box<dyn Blob>) -> FileMatStore {
        FileMatStore {
            blob: RefCell::new(blob),
        }
    }

    /// 落在一个目录里。
    pub fn open_dir(root: impl AsRef<Path>) -> FileMatStore {
        FileMatStore::new(Box::new(DirBlob::new(root.as_ref())))
    }

    fn ensure_meta(&self) -> Result<(), IoErr> {
        let mut b = self.blob.borrow_mut();
        if b.get(META)?.is_none() {
            let meta = serde_json::json!({
                "v": 1, "temporary": true,
                "note": "临时存储，骨架最终裁定后换（步 29，主控:B0460）",
            });
            b.put_atomic(META, meta.to_string().as_bytes())?;
        }
        Ok(())
    }

    /// 这份材料上的全部标记（每个前提题只留最后写的那条，按前提题哈希排序）。
    pub fn marks_full(&self, mat: &str) -> Vec<Mark> {
        let raw = match self.blob.borrow().get(&format!("marks/{mat}.jsonl")) {
            Ok(Some(b)) => b,
            _ => return vec![],
        };
        let mut by_q: std::collections::BTreeMap<String, Mark> = Default::default();
        for line in String::from_utf8_lossy(&raw).lines() {
            if let Ok(j) = serde_json::from_str::<serde_json::Value>(line)
                && let Ok(m) = Mark::from_json(&j)
            {
                by_q.insert(m.q.clone(), m);
            }
        }
        by_q.into_values().collect()
    }

    /// 存里所有有标记的材料哈希（排序）。
    pub fn marked_mats(&self) -> Vec<String> {
        let keys = self.blob.borrow().list("marks/").unwrap_or_default();
        keys.iter()
            .filter_map(|k| k.strip_prefix("marks/")?.strip_suffix(".jsonl"))
            .map(String::from)
            .collect()
    }

    /// (材料, 前提题) 不同键的标记总数。
    pub fn mark_count(&self) -> usize {
        self.marked_mats()
            .iter()
            .map(|m| self.marks_full(m).len())
            .sum()
    }
}

impl MatStorePort for FileMatStore {
    /// 内容寻址：地址就是 `Mat.hash`，同内容同地址，已有则不重写。写失败时地址照返回（接口无错误通道），
    /// 之后 `get` 取不到——文件料库是临时存储，错误处理留给最终形态。
    fn put(&mut self, m: &Mat) -> String {
        let key = format!("mats/{}.json", m.hash);
        let _ = self.ensure_meta();
        let mut b = self.blob.borrow_mut();
        if !matches!(b.get(&key), Ok(Some(_)))
            && let Ok(bytes) = serde_json::to_vec(m) {
                let _ = b.put_atomic(&key, &bytes);
            }
        m.hash.clone()
    }

    fn get(&self, addr: &str) -> Option<Mat> {
        let bytes = self
            .blob
            .borrow()
            .get(&format!("mats/{addr}.json"))
            .ok()??;
        serde_json::from_slice(&bytes).ok()
    }

    fn marks(&self, addr: &str) -> Vec<CacheKey> {
        self.marks_full(addr)
            .into_iter()
            .map(|m| CacheKey {
                model_id: "mark".into(),
                state: m.mat,
                q: m.q,
                phys: "test".into(),
                render: "mark-v1".into(),
                perm_seed: None,
            })
            .collect()
    }
}

impl MarkStore for FileMatStore {
    fn read(&self, mat: &str, q: &str) -> Option<Mark> {
        self.marks_full(mat).into_iter().find(|m| m.q == q)
    }

    fn write(&self, mut mark: Mark) -> Result<(), String> {
        self.ensure_meta().map_err(|e| e.to_string())?;
        if let Some(old) = self.read(&mark.mat, &mark.q) {
            mark.first_purpose = old.first_purpose.clone();
            mark.hits = old.hits;
            if old == mark {
                return Ok(());
            }
        }
        let line = mark.to_json().to_string();
        self.blob
            .borrow_mut()
            .append_durable(&format!("marks/{}.jsonl", mark.mat), line.as_bytes())
            .map_err(|e| e.to_string())
    }
}
