//! 精确图算法宿主动作 `graph:cycles`（T0 步 39，主控板 Z0454；S14 扩展点，`20` v2 §五）。
//!
//! 在有向图上找「经过指定节点的简单环」，按节点集合交出：输入
//! `{"edges": [{"u", "v"}], "through": 节点, "among": [节点…]?, "sizes": [k…]}`，输出
//! `{"algo": "cycles", "through", "sizes", "sets": [[节点…]], "count"}`。一个节点集合 S 在输出里，当且仅当
//! S 含 `through`、其余节点都在 `among` 里（不给 `among` 则不限）、|S| 在 `sizes` 里，并且只用 S 内部的边
//! 能排出一个经过 S 全部节点的有向环。每个集合按节点 id 升序，集合列表按字典序，去重。
//!
//! 为什么要它（隐性知识六项，见 `地基/过程记录/工程-T0-夹具端口.md` §三）：
//! 1. 来源：变更单 §3.2 T0 段「找闭合构型：现有 `graph:*` 里没有找环，按扩展点加一个宿主动作或库函数」。
//!    T1（网络发现）的候选构型就是「发信号的主体加名单里 2 或 3 个、能排成一个环」，共用夹具
//!    `reference::closed_in` 的定义；本动作与它逐字同义。
//! 2. 替代：写成 `.jpp` 库函数（嵌套循环逐个子集试排列）也做得到，放弃的理由是找环是精确算法，
//!    与另外六个 `graph:*` 同族（纯函数、可逆、成本 0、taint 继承），放在动作表里 C4、C6 的完整 T1
//!    与 CLI 常驻运行都能直接用；另开文件是因为 `graph.rs` 已超过 A8 的 1500 行。
//! 3. 依赖：只依赖 `serde_json`；被骨架系统检验的 `地基/骨架原型/对照/jpp/t1.jpp` 用。
//! 4. 推翻条件：若骨架 v1 的构型搜索改成由判断出口逐步扩展（不再先判全图再找环），本动作只剩通用用途，
//!    按消融（步 32）决定去留。
//! 5. 改法：契约改动同步 `tests/actions_graph_cycles.rs` 的暴力对拍（测试里另写，与 `closed_in` 同定义）与 `t1.jpp`。
//! 6. 边界：`sizes` 每项须在 2..=6（深度优先枚举的代价随环长指数增长，上限写死防误用）；自环报错，
//!    与其余 `graph:*` 一致；重复边不影响结果。

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};

use super::Ctx;
use crate::interp::json_to_value;
use crate::value::Value;

/// 环长上限（含 `through`）。
const MAX_SIZE: usize = 6;

pub(super) fn cycles(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    let [input] = args else {
        return Err(
            "graph:cycles expects exactly one JSON argument: do(\"graph:cycles\", [graph], seq)"
                .into(),
        );
    };
    Ok(json_to_value(&run_cycles(&input.to_json())?))
}

fn node_id(j: &Json) -> Result<String, String> {
    match j {
        Json::String(s) => Ok(s.clone()),
        Json::Number(n) => Ok(n.to_string()),
        other => Err(format!(
            "expected a node id (string or number), got {other}"
        )),
    }
}

/// 纯函数本体。
fn run_cycles(input: &Json) -> Result<Json, String> {
    let through = node_id(
        input
            .get("through")
            .ok_or("missing required field 'through'")?,
    )?;
    let sizes: BTreeSet<usize> = input
        .get("sizes")
        .and_then(|s| s.as_array())
        .ok_or("missing or non-array field 'sizes'")?
        .iter()
        .map(|k| match k.as_u64() {
            Some(k) if (2..=MAX_SIZE as u64).contains(&k) => Ok(k as usize),
            _ => Err(format!(
                "sizes entries must be integers in 2..={MAX_SIZE}, got {k}"
            )),
        })
        .collect::<Result<_, _>>()?;
    if sizes.is_empty() {
        return Err("'sizes' must not be empty".into());
    }
    let among: Option<BTreeSet<String>> = match input.get("among") {
        None | Some(Json::Null) => None,
        Some(Json::Array(a)) => Some(a.iter().map(node_id).collect::<Result<_, _>>()?),
        Some(other) => return Err(format!("'among' must be an array of node ids, got {other}")),
    };
    let allowed = |n: &str| n == through || among.as_ref().is_none_or(|a| a.contains(n));
    let edges = input
        .get("edges")
        .and_then(|e| e.as_array())
        .ok_or("missing or non-array field 'edges'")?;
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (i, e) in edges.iter().enumerate() {
        let u = node_id(
            e.get("u")
                .ok_or_else(|| format!("edges[{i}] missing 'u'"))?,
        )?;
        let v = node_id(
            e.get("v")
                .ok_or_else(|| format!("edges[{i}] missing 'v'"))?,
        )?;
        if u == v {
            return Err(format!(
                "edges[{i}] is a self-loop ('{u}' == '{v}'), which graph:* actions do not support"
            ));
        }
        if allowed(&u) && allowed(&v) {
            out.entry(u).or_default().insert(v);
        }
    }
    let max = *sizes.iter().max().expect("非空");
    let mut found: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut path = vec![through.clone()];
    dfs(&out, &through, &sizes, max, &mut path, &mut found);
    let sets: Vec<Json> = found.iter().map(|s| json!(s)).collect();
    Ok(json!({
        "algo": "cycles",
        "through": through,
        "sizes": sizes.iter().collect::<Vec<_>>(),
        "count": sets.len(),
        "sets": sets,
    }))
}

/// 从 `through` 出发的简单路径深度优先枚举；回到 `through` 且长度在 `sizes` 里即记下节点集合。
fn dfs(
    out: &BTreeMap<String, BTreeSet<String>>,
    through: &str,
    sizes: &BTreeSet<usize>,
    max: usize,
    path: &mut Vec<String>,
    found: &mut BTreeSet<Vec<String>>,
) {
    let last = path.last().expect("路径非空").clone();
    let Some(next) = out.get(&last) else { return };
    for v in next {
        if v == through {
            if sizes.contains(&path.len()) {
                let mut s = path.clone();
                s.sort();
                found.insert(s);
            }
        } else if path.len() < max && !path.contains(v) {
            path.push(v.clone());
            dfs(out, through, sizes, max, path, found);
            path.pop();
        }
    }
}
