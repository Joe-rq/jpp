//! 精确图算法宿主动作 `graph:*`（比赛 R2b，2026-09-25）。
//!
//! 六个动作：`graph:matching`（带权最大匹配，二分图 Hungarian / 一般图精确位掩码 DP）、
//! `graph:shortest_path`（Dijkstra）、`graph:max_clique`（Bron–Kerbosch + 剪枝）、
//! `graph:components`（并查集）、`graph:set_cover`（精确位掩码 DP + 贪心）、`graph:max_flow`
//! （Dinic）。全部纯函数、可逆、`taint_out: inherit`、成本 0（附注 §五）。设计决定、范围裁剪、
//! JSON 契约详见 `地基/过程记录/工程-比赛R2b.md`；六项隐性知识见 `actions/mod.rs` 头注与本文件
//! 各函数上的局部注记。
//!
//! JSON 契约要点（各函数的输入/输出细节见函数头注）：节点 id 统一转字符串（输入可为 JSON 字符串
//! 或数字）；边 `{"u", "v", "w"}`（`max_flow` 用 `"cap"`）；自环一律报错；每个动作的输出都带
//! `edge_index`（用到的边在输入 `edges` 数组里的下标），供 L2 `on_graph`/`interval` 按下标把算法
//! 产物映回边元素做 `compose(exits, "all")` 与 `differs`（附注 §3.3）。
//!
//! rebase 到 main（C-1b 合入后）接入库轨的统一动作表（`super::HostAction`/`BUILTIN_ACTIONS`）：
//! 本文件只留六个 `pub(super) fn <name>(_: &Ctx, args: &[Value]) -> Result<Value, String>` 包装
//! （与 `actions/io.rs` 同一写法），登记行搬进 `mod.rs::BUILTIN_ACTIONS`；原先本模块自带的
//! `register`/`ALGORITHMS`/过渡期常量表已删（不再需要独立注册入口）。

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

use serde_json::{Value as Json, json};

use super::Ctx;
use crate::interp::json_to_value;
use crate::value::Value;

/// 六个动作共用的包装：取唯一实参、转 JSON、跑对应算法、转回 `Value`（附注 §三·3.3）。
fn wrap(
    args: &[Value],
    name: &str,
    run: fn(&Json) -> Result<Json, String>,
) -> Result<Value, String> {
    let [input] = args else {
        return Err(format!(
            "{name} expects exactly one JSON argument: do(\"{name}\", [graph], seq)"
        ));
    };
    let out = run(&input.to_json())?;
    Ok(json_to_value(&out))
}

pub(super) fn matching(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:matching", run_matching)
}
pub(super) fn shortest_path(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:shortest_path", run_shortest_path)
}
pub(super) fn max_clique(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:max_clique", run_max_clique)
}
pub(super) fn components(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:components", run_components)
}
pub(super) fn set_cover(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:set_cover", run_set_cover)
}
pub(super) fn max_flow(_: &Ctx, args: &[Value]) -> Result<Value, String> {
    wrap(args, "graph:max_flow", run_max_flow)
}

// ---------- 输入解析共用 ----------

/// 节点 id 允许 JSON 字符串或数字，内部一律转字符串比较（过程记录 §四）。
fn node_id(j: &Json) -> Result<String, String> {
    match j {
        Json::String(s) => Ok(s.clone()),
        Json::Number(n) => Ok(n.to_string()),
        other => Err(format!(
            "expected a node id (string or number), got {other}"
        )),
    }
}

fn get_arr<'a>(input: &'a Json, key: &str) -> Result<&'a Vec<Json>, String> {
    input
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("missing or non-array field '{key}'"))
}

fn get_bool(input: &Json, key: &str, default: bool) -> bool {
    input.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn get_node_field(input: &Json, key: &str) -> Result<String, String> {
    match input.get(key) {
        Some(v) => node_id(v),
        None => Err(format!("missing required field '{key}'")),
    }
}

struct RawEdge {
    u: String,
    v: String,
    w: f64,
    index: usize,
}

/// 解析 `edges`（`weight_key` 是权重字段名——`matching`/`shortest_path` 用 `"w"`，`max_flow` 用
/// `"cap"`；`max_clique`/`components` 也走这条但权重被忽略）。自环一律报错（过程记录 §四）；
/// `allow_negative=false` 时负权在解析期就拒绝，不留给算法内部悄悄吞掉。
fn parse_edges(
    input: &Json,
    weight_key: &str,
    default_w: f64,
    allow_negative: bool,
) -> Result<Vec<RawEdge>, String> {
    let arr = get_arr(input, "edges")?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, e) in arr.iter().enumerate() {
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
        let w = match e.get(weight_key) {
            Some(Json::Number(n)) => n
                .as_f64()
                .ok_or_else(|| format!("edges[{i}].{weight_key} is not a finite number"))?,
            Some(other) => {
                return Err(format!(
                    "edges[{i}].{weight_key} must be a number, got {other}"
                ));
            }
            None => default_w,
        };
        if w.is_nan() {
            return Err(format!("edges[{i}].{weight_key} is NaN"));
        }
        if !allow_negative && w < 0.0 {
            return Err(format!(
                "edges[{i}].{weight_key} = {w} is negative; this action requires non-negative values"
            ));
        }
        out.push(RawEdge { u, v, w, index: i });
    }
    Ok(out)
}

/// 节点集合：`nodes` 显式给出则校验边端点都在表里（防手误漏写），否则按边出现顺序推断。
fn collect_nodes(input: &Json, edges: &[RawEdge]) -> Result<Vec<String>, String> {
    if let Some(Json::Array(ns)) = input.get("nodes") {
        let declared: Vec<String> = ns.iter().map(node_id).collect::<Result<_, _>>()?;
        let set: HashSet<&str> = declared.iter().map(|s| s.as_str()).collect();
        for e in edges {
            if !set.contains(e.u.as_str()) {
                return Err(format!(
                    "edges[{}] references node '{}' which is not in 'nodes'",
                    e.index, e.u
                ));
            }
            if !set.contains(e.v.as_str()) {
                return Err(format!(
                    "edges[{}] references node '{}' which is not in 'nodes'",
                    e.index, e.v
                ));
            }
        }
        Ok(declared)
    } else {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for e in edges {
            if seen.insert(e.u.clone()) {
                out.push(e.u.clone());
            }
            if seen.insert(e.v.clone()) {
                out.push(e.v.clone());
            }
        }
        Ok(out)
    }
}

// ---------- 并列解（C-8，主控 Z0174） ----------
//
// 「并列」= 与主解同样优的其他解。案例 01–03 缺口：动作只返回搜索顺序里的第一个最优解，另一个同样好的解
// 被静默丢掉，读者会以为结论唯一；`interval` 也因此漏掉并列解用到的未决边。
//
// 形态（`地基/过程记录/工程-C7C8-动作表.md` §一·1.3、主控 2026-09-29 裁定）：原有输出字段一字不改，
// 主解仍是原来那一个；另加三个字段——
//   `tie_count`：并列最优解的个数（含主解）；`ties_complete=false` 时是「至少这么多」（等于枚举上限）；
//   `ties`：并列最优解列表，每项与主解同形，主解排第一；
//   `ties_complete`：`ties` 是否列全了全部并列解。无并列时 `tie_count=1`、`ties` 只有主解、`ties_complete=true`。
// 无解（不可达、覆盖不了全集）时 `tie_count=0`、`ties=[]`、`ties_complete=true`。
// 并列数可能指数增长，枚举上限 [`TIES_CAP`]（64），超过如实报 `ties_complete=false`，不静默截断。
// `max_flow` 不枚举流的分配方式（个数无界、流量是连续量），只报 `flow_unique`。
//
// 并列的判据是数值相等：和、距离、成本先各自算出，再按相对容差 [`close`] 比较（相对 1e-9，没有绝对项——
// 权重远小于 1e-9 时也照样按各自的量纲比较，理由同 PR #36 复核 P2）；主解永远在 `ties` 里。

/// 并列解枚举上限。
const TIES_CAP: usize = 64;

/// 两个数在相对 1e-9 内视为相等（同为 0 或逐位相同也相等）。没有绝对项。
fn close(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

/// 把并列字段加进动作输出（`out` 必须是 JSON 对象）。`ties.len() > TIES_CAP` 时截到上限、标不完整。
fn with_ties(mut out: Json, mut ties: Vec<Json>, mut complete: bool) -> Json {
    if ties.len() > TIES_CAP {
        ties.truncate(TIES_CAP);
        complete = false;
    }
    let obj = out.as_object_mut().expect("动作输出是 JSON 对象");
    obj.insert("tie_count".into(), json!(ties.len()));
    obj.insert("ties".into(), Json::Array(ties));
    obj.insert("ties_complete".into(), json!(complete));
    out
}

// ---------- graph:matching ----------
//
// 输入：`{"edges":[{"u","v","w"}], "nodes":[...]?, "bipartite": bool?(默认false),
//        "parts": [[左...],[右...]]?(bipartite=true 时必给), "size": int?(截断)}`。
// 输出（两种模式共用形状）：`{"algo":"matching","mode":"bipartite"|"general_exact",
//        "pairs":[{"u","v","w","edge_index"}],"total_weight","matched_nodes","unmatched_nodes",
//        "exact":true,"node_count"}`。
//
// 范围决定（过程记录 §三·3.1）：二分图模式用 Hungarian（0-权哑元规约），规模到 325 经过实测；
// 一般图模式用精确位掩码 DP，上限 n≤20（超界报错，不近似）。`size` 只做「按权降序截断」，不强制
// 凑够基数（问题清单 #1）。

fn run_matching(input: &Json) -> Result<Json, String> {
    if get_bool(input, "bipartite", false) {
        run_matching_bipartite(input)
    } else {
        run_matching_general(input)
    }
}

fn size_cap(input: &Json) -> Option<usize> {
    input
        .get("size")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
}

fn run_matching_bipartite(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, false)?;
    let parts = input
        .get("parts")
        .and_then(|p| p.as_array())
        .ok_or_else(|| "bipartite matching requires 'parts': [[left...],[right...]]".to_string())?;
    if parts.len() != 2 {
        return Err("'parts' must have exactly two node-id lists".into());
    }
    let left: Vec<String> = parts[0]
        .as_array()
        .ok_or_else(|| "'parts[0]' must be an array".to_string())?
        .iter()
        .map(node_id)
        .collect::<Result<_, _>>()?;
    let right: Vec<String> = parts[1]
        .as_array()
        .ok_or_else(|| "'parts[1]' must be an array".to_string())?
        .iter()
        .map(node_id)
        .collect::<Result<_, _>>()?;
    let li: HashMap<&str, usize> = left
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let ri: HashMap<&str, usize> = right
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    for l in &left {
        if ri.contains_key(l.as_str()) {
            return Err(format!("node '{l}' listed in both parts"));
        }
    }

    // 同一对 (left, right) 若有多条边只保留权最大的一条（记原下标供输出回指）。
    let mut best: HashMap<(usize, usize), (f64, usize)> = HashMap::new();
    for e in &edges {
        let pair = if let (Some(&il), Some(&ir)) = (li.get(e.u.as_str()), ri.get(e.v.as_str())) {
            (il, ir)
        } else if let (Some(&il), Some(&ir)) = (li.get(e.v.as_str()), ri.get(e.u.as_str())) {
            (il, ir)
        } else {
            return Err(format!(
                "edges[{}] ('{}' - '{}') does not connect the two parts",
                e.index, e.u, e.v
            ));
        };
        best.entry(pair)
            .and_modify(|cur| {
                if e.w > cur.0 {
                    *cur = (e.w, e.index);
                }
            })
            .or_insert((e.w, e.index));
    }

    let n = left.len();
    let m = right.len();
    let all_rows: Vec<usize> = (0..n).collect();
    let all_cols: Vec<usize> = (0..m).collect();
    let root = bip_solve(&best, &all_rows, &all_cols);
    let mut pairs = root.pairs.clone();
    let cap = size_cap(input);
    let truncated_away = cap.is_some_and(|c| pairs.len() > c);
    if let Some(cap) = cap {
        pairs = truncate_pairs(pairs, cap);
    }
    let item = |ps: &[(usize, usize, f64, usize)]| -> Json {
        let pairs_json: Vec<Json> = ps
            .iter()
            .map(|(row, col, w, idx)| json!({"u": left[*row], "v": right[*col], "w": w, "edge_index": idx}))
            .collect();
        let (matched_nodes, unmatched_nodes) = matched_unmatched(
            left.iter().chain(right.iter()),
            ps.iter()
                .flat_map(|(row, col, _, _)| [left[*row].as_str(), right[*col].as_str()]),
        );
        json!({
            "pairs": pairs_json,
            "total_weight": ps.iter().map(|p| p.2).sum::<f64>(),
            "matched_nodes": matched_nodes,
            "unmatched_nodes": unmatched_nodes,
        })
    };
    let mut out = item(&pairs);
    let o = out.as_object_mut().expect("对象");
    o.insert("algo".into(), json!("matching"));
    o.insert("mode".into(), json!("bipartite"));
    o.insert("exact".into(), json!(true));
    o.insert("node_count".into(), json!(n + m));

    // C-8：并列的最大权匹配。先枚举未截断的全部最优匹配（主解第一），再各自按 `size` 截断、按截断结果去重
    let (all_ties, mut complete) = bip_enumerate(&best, n, m, &root);
    let mut ties: Vec<Vec<(usize, usize, f64, usize)>> = Vec::new();
    for t in all_ties {
        let t = match cap {
            Some(c) => truncate_pairs(t, c),
            None => t,
        };
        if !ties.iter().any(|x| same_pairs(x, &t)) {
            ties.push(t);
        }
    }
    // 截断会让「同样优」的判据变模糊（边界上同权的边取谁也是并列来源），如实标不完整
    if truncated_away {
        complete = false;
    }
    Ok(with_ties(
        out,
        ties.iter().map(|t| item(t)).collect(),
        complete,
    ))
}

/// 按权降序取前 `cap` 对（稳定排序，`size` 选项的既有语义）。
fn truncate_pairs(
    mut pairs: Vec<(usize, usize, f64, usize)>,
    cap: usize,
) -> Vec<(usize, usize, f64, usize)> {
    pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(Ordering::Equal));
    pairs.truncate(cap);
    pairs
}

/// 两个匹配是同一组节点对（不看顺序）。
fn same_pairs(a: &[(usize, usize, f64, usize)], b: &[(usize, usize, f64, usize)]) -> bool {
    let key = |p: &[(usize, usize, f64, usize)]| {
        let mut k: Vec<(usize, usize)> = p.iter().map(|x| (x.0, x.1)).collect();
        k.sort_unstable();
        k
    };
    key(a) == key(b)
}

type BipEdges = HashMap<(usize, usize), (f64, usize)>;
/// 一组指派：每个解是 (左, 右, 权, 边号) 的列表
type 指派组 = Vec<Vec<(usize, usize, f64, usize)>>;

/// 二分图带权匹配一次求解的结果：`pairs` 是权 > 0 的实边配对（按右端升序）；`cost`/`u`/`v` 是补成方阵后的
/// 匈牙利代价矩阵与对偶势（1-索引），供并列枚举判「紧边」。
struct BipSol {
    pairs: Vec<(usize, usize, f64, usize)>,
    cost: Vec<Vec<f64>>,
    u: Vec<f64>,
    v: Vec<f64>,
    /// 匈牙利的指派（1-索引）：`assign[j]` = 配到列 `j` 的行号，0 = 该列空。
    assign: Vec<usize>,
    size: usize,
}

/// 在给定的左行集合与右列集合上求最大权匹配（补零权哑元成方阵，同原有做法）。`rows`/`cols` 是全局下标。
fn bip_solve(best: &BipEdges, rows: &[usize], cols: &[usize]) -> BipSol {
    let (n, m) = (rows.len(), cols.len());
    let size = n.max(m);
    let mut cost = vec![vec![0.0f64; size + 1]; size + 1];
    for (li, &l) in rows.iter().enumerate() {
        for (ri, &r) in cols.iter().enumerate() {
            if let Some(&(w, _)) = best.get(&(l, r)) {
                cost[li + 1][ri + 1] = -w;
            }
        }
    }
    let (assignment, u, v) = hungarian_full(&cost, size, size);
    let mut pairs = Vec::new();
    for (j, &i) in assignment.iter().enumerate().skip(1).take(size) {
        if i == 0 {
            continue;
        }
        let (row, col) = (i - 1, j - 1);
        if row < n
            && col < m
            && let Some(&(w, orig_idx)) = best.get(&(rows[row], cols[col]))
            && w > 0.0
        {
            pairs.push((rows[row], cols[col], w, orig_idx));
        }
    }
    BipSol {
        pairs,
        cost,
        u,
        v,
        assign: assignment,
        size,
    }
}

/// 枚举全部最大权匹配（C-8；Z0223 改为增广路核对）。返回 `(解们, 是否列全)`，主解排第一，解们两两不同，
/// 最多 `TIES_CAP + 1` 个（多出的那一个只用来判「不止 64 个」，由 `with_ties` 截掉）。
///
/// 做法：补成方阵后（同 `bip_solve`），根问题的最优指派恰好是「紧格」图（约化代价 `代价 - u - v` 为 0 的格）
/// 里的完美匹配（线性规划互补松弛）。左行按序逐行决定「配某条紧的实边」或「不配」（不配 = 落在一个紧的零格上），
/// 每个决定用紧格图里的一次增广路核对「已定的前缀还能补成完美匹配」，所以不会走进死路，每个节点都至少通向一个解。
/// 手里始终有一个与前缀相容的完美匹配（见证）：见证自己选的分支零成本，其他分支只需从被挤掉的那一行找一条增广路，
/// 单次 O(紧格数)；C-8 原做法每个分支重跑一次匈牙利（O(n²m)），162×163 上是主要耗时。
fn bip_enumerate(
    best: &BipEdges,
    n: usize,
    m: usize,
    root: &BipSol,
) -> (指派组, bool) {
    let mut found = vec![root.pairs.clone()];
    let wmax = best.values().map(|x| x.0).fold(0.0f64, f64::max);
    if wmax <= 0.0 || root.pairs.is_empty() {
        return (found, true);
    }
    let tol = 1e-9 * wmax;
    let s = root.size;
    // 紧格：实格（有正权边）看约化代价；零格（无边、零权、哑元）要求代价 0 且约化代价 0
    let mut pos_tight: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut zero_tight: Vec<Vec<usize>> = vec![Vec::new(); s];
    for i in 0..s {
        for j in 0..s {
            let c = root.cost[i + 1][j + 1];
            let tight = (c - root.u[i + 1] - root.v[j + 1]).abs() <= tol;
            if !tight {
                continue;
            }
            if c == 0.0 {
                zero_tight[i].push(j);
            } else if i < n && j < m && best.get(&(i, j)).is_some_and(|x| x.0 > 0.0) {
                pos_tight[i].push(j);
            }
        }
    }
    // 见证：匈牙利给出的指派（1-索引的 assign[j] = 行），限在紧格里；万一浮点使某格落在容差外，再由增广补齐
    let mut st = BipState {
        mode: vec![0; s],
        col_fixed: vec![false; s],
        mate_row: vec![NONE; s],
        mate_col: vec![NONE; s],
    };
    let ctx = BipCtx {
        best,
        n,
        pos_tight,
        zero_tight,
    };
    for (j, &i1) in root.assign.iter().enumerate().skip(1).take(s) {
        if i1 == 0 {
            continue;
        }
        let (i, j0) = (i1 - 1, j - 1);
        if ctx.pos_tight_has(i, j0) || ctx.zero_tight[i].contains(&j0) {
            st.mate_row[i] = j0;
            st.mate_col[j0] = i;
        }
    }
    for i in 0..s {
        if st.mate_row[i] == NONE && !ctx.augment(&mut st, i) {
            // 不该发生（根解本身就是紧格图里的完美匹配）；发生就如实标不完整，不猜
            return (found, false);
        }
    }
    let mut chosen = Vec::new();
    let mut overflow = false;
    bip_rec(&ctx, 0, &st, &mut chosen, &mut found, &mut overflow);
    (found, !overflow)
}

const NONE: usize = usize::MAX;

/// 增广路核对的状态：`mode[i]` 0 = 自由（可配任一紧格），1 = 只能落零格（已决定不配实边），2 = 已定（其列 `col_fixed`）。
#[derive(Clone)]
struct BipState {
    mode: Vec<u8>,
    col_fixed: Vec<bool>,
    mate_row: Vec<usize>,
    mate_col: Vec<usize>,
}

struct BipCtx<'a> {
    best: &'a BipEdges,
    n: usize,
    /// 左行 `l` 的紧实边（右端列号）。
    pos_tight: Vec<Vec<usize>>,
    /// 方阵每一行的紧零格（列号）。
    zero_tight: Vec<Vec<usize>>,
}

impl BipCtx<'_> {
    fn pos_tight_has(&self, i: usize, j: usize) -> bool {
        i < self.n && self.pos_tight[i].contains(&j)
    }

    /// 从未配的行 `x` 出发在紧格图里宽搜一条增广路，成功就沿路翻转并返回 true。
    /// 自由行可走紧实边与紧零格，`mode=1` 的行与哑元行只走紧零格；已定行的列被封住，所以已定行不会被挤动。
    fn augment(&self, st: &mut BipState, x: usize) -> bool {
        let s = st.mode.len();
        let mut par_row = vec![NONE; s]; // 列 j 是被哪一行走到的（NONE = 还没走到）
        let mut queue = vec![x];
        let mut head = 0;
        while head < queue.len() {
            let row = queue[head];
            head += 1;
            let real: &[usize] = if st.mode[row] == 0 && row < self.n {
                &self.pos_tight[row]
            } else {
                &[]
            };
            for &j in real.iter().chain(self.zero_tight[row].iter()) {
                if par_row[j] != NONE || st.col_fixed[j] {
                    continue;
                }
                par_row[j] = row;
                let owner = st.mate_col[j];
                if owner == NONE {
                    // 空列：沿来路翻转
                    let mut col = j;
                    loop {
                        let r = par_row[col];
                        let old = st.mate_row[r];
                        st.mate_row[r] = col;
                        st.mate_col[col] = r;
                        if r == x {
                            return true;
                        }
                        col = old;
                    }
                }
                queue.push(owner);
            }
        }
        false
    }

    /// 决定左行 `i` 配紧实边到列 `r`（`r` 未被封）。返回决定后的状态；核对不通（前缀补不成完美匹配）返回 None。
    fn try_choose(&self, st: &BipState, i: usize, r: usize) -> Option<BipState> {
        let mut t = st.clone();
        let j0 = t.mate_row[i];
        t.mode[i] = 2;
        t.col_fixed[r] = true;
        if j0 != r {
            let k = t.mate_col[r]; // 完美匹配里每列都有主；r 未封，所以 k 不是已定行
            t.mate_row[i] = r;
            t.mate_col[r] = i;
            t.mate_col[j0] = NONE;
            t.mate_row[k] = NONE;
            if !self.augment(&mut t, k) {
                return None;
            }
        }
        Some(t)
    }

    /// 决定左行 `i` 不配实边（落在一个紧零格上）。
    fn try_none(&self, st: &BipState, i: usize) -> Option<BipState> {
        let mut t = st.clone();
        t.mode[i] = 1;
        let j0 = t.mate_row[i];
        if self.zero_tight[i].contains(&j0) {
            return Some(t);
        }
        t.mate_row[i] = NONE;
        t.mate_col[j0] = NONE;
        if self.augment(&mut t, i) {
            Some(t)
        } else {
            None
        }
    }
}

fn bip_rec(
    ctx: &BipCtx,
    i: usize,
    st: &BipState,
    chosen: &mut Vec<(usize, usize, f64, usize)>,
    found: &mut Vec<Vec<(usize, usize, f64, usize)>>,
    overflow: &mut bool,
) {
    if *overflow {
        return;
    }
    if i == ctx.n {
        if !found.iter().any(|x| same_pairs(x, chosen)) {
            if found.len() >= TIES_CAP {
                *overflow = true;
                return;
            }
            found.push(chosen.clone());
        }
        return;
    }
    let mut options: Vec<Option<usize>> = ctx.pos_tight[i]
        .iter()
        .filter(|&&r| !st.col_fixed[r])
        .map(|&r| Some(r))
        .collect();
    if !ctx.zero_tight[i].is_empty() {
        options.push(None);
    }
    for opt in options {
        if *overflow {
            return;
        }
        let next = match opt {
            Some(r) => ctx.try_choose(st, i, r),
            None => ctx.try_none(st, i),
        };
        let Some(next) = next else { continue };
        if let Some(r) = opt {
            let &(w, idx) = ctx.best.get(&(i, r)).expect("紧边一定是实边");
            chosen.push((i, r, w, idx));
        }
        bip_rec(ctx, i + 1, &next, chosen, found, overflow);
        if opt.is_some() {
            chosen.pop();
        }
    }
}

fn matched_unmatched<'a>(
    all: impl Iterator<Item = &'a String>,
    matched: impl Iterator<Item = &'a str>,
) -> (Vec<String>, Vec<String>) {
    let matched_set: HashSet<&str> = matched.collect();
    let mut matched_nodes: Vec<String> = matched_set.iter().map(|s| s.to_string()).collect();
    matched_nodes.sort();
    let mut unmatched: Vec<String> = all
        .filter(|n| !matched_set.contains(n.as_str()))
        .cloned()
        .collect();
    unmatched.sort();
    unmatched.dedup();
    (matched_nodes, unmatched)
}

/// 最小费用指派（Kuhn 算法 + 势函数，O(n^2 m)；cp-algorithms「Assignment problem, Hungarian
/// algorithm」同型写法）。`cost` 是 (n+1)×(m+1) 的 1-索引矩阵（第 0 行/列不用），要求 n ≤ m。
/// 每一行都被指派到一列（所有格子代价有限）：`p[j]` = 匹配到列 j 的行号（1..=n，0 = 该列空）。
/// 调用方都是方阵（n = m）。
fn hungarian_full(cost: &[Vec<f64>], n: usize, m: usize) -> (Vec<usize>, Vec<f64>, Vec<f64>) {
    const INF: f64 = f64::INFINITY;
    let mut u = vec![0.0f64; n + 1];
    let mut v = vec![0.0f64; m + 1];
    let mut p = vec![0usize; m + 1];
    let mut way = vec![0usize; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![INF; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = INF;
            let mut j1 = 0usize;
            for j in 1..=m {
                if !used[j] {
                    let cur = cost[i0][j] - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    // 同时返回对偶势 u、v（`cost[i][j] - u[i] - v[j] >= 0`，匹配格取等号）：并列枚举据此认「紧边」（C-8）
    (p, u, v)
}

const GENERAL_MATCHING_MAX_N: usize = 20;

fn run_matching_general(input: &Json) -> Result<Json, String> {
    let edges_raw = parse_edges(input, "w", 1.0, true)?;
    let nodes = collect_nodes(input, &edges_raw)?;
    let n = nodes.len();
    if n > GENERAL_MATCHING_MAX_N {
        return Err(format!(
            "graph:matching general (non-bipartite) mode supports at most {GENERAL_MATCHING_MAX_N} \
             nodes (exact bitmask DP); got {n} nodes. Use \"bipartite\": true with \"parts\" for \
             larger graphs (Hungarian; tested to 325 nodes, see 工程-比赛R2b.md §三·3.1)."
        ));
    }
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut best: HashMap<(usize, usize), (f64, usize)> = HashMap::new();
    for e in &edges_raw {
        let a = *idx
            .get(e.u.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.u))?;
        let b = *idx
            .get(e.v.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.v))?;
        let key = (a.min(b), a.max(b));
        best.entry(key)
            .and_modify(|cur| {
                if e.w > cur.0 {
                    *cur = (e.w, e.index);
                }
            })
            .or_insert((e.w, e.index));
    }

    let size = 1usize << n;
    let mut dp = vec![0.0f64; size];
    for mask in 1..size {
        let i = mask.trailing_zeros() as usize;
        let without_i = mask & !(1 << i);
        let mut best_val = dp[without_i];
        for j in 0..n {
            if j == i {
                continue;
            }
            let jb = 1usize << j;
            if mask & jb == 0 {
                continue;
            }
            if let Some(&(w, _)) = best.get(&(i.min(j), i.max(j))) {
                let rest = without_i & !jb;
                let cand = dp[rest] + w;
                if cand > best_val {
                    best_val = cand;
                }
            }
        }
        dp[mask] = best_val;
    }

    let full = size - 1;
    let mut mask = full;
    let mut pairs: Vec<(usize, usize, f64, usize)> = Vec::new();
    while mask != 0 {
        let i = mask.trailing_zeros() as usize;
        let without_i = mask & !(1 << i);
        // 精确相等，不是放宽的阈值比较（PR #36 复核 P2）：`dp[mask]` 在正向 DP 里是直接赋值
        // 成 `dp[without_i]` 或某个候选 `dp[rest]+w` 本身（不是多个和的再运算），重放同一个
        // 加法一定逐位相同——用 `1e-9` 的绝对阈值反而会在权重远小于 1e-9 时把「真实存在的
        // 匹配」错判成「未匹配」，静默丢边却仍标 `exact:true`（过程记录 §二）。
        if dp[without_i] == dp[mask] {
            mask = without_i;
            continue;
        }
        let mut found: Option<(usize, f64, usize, usize)> = None;
        for j in 0..n {
            if j == i {
                continue;
            }
            let jb = 1usize << j;
            if mask & jb == 0 {
                continue;
            }
            if let Some(&(w, orig_idx)) = best.get(&(i.min(j), i.max(j))) {
                let rest = without_i & !jb;
                if dp[rest] + w == dp[mask] {
                    found = Some((j, w, orig_idx, rest));
                    break;
                }
            }
        }
        let (j, w, orig_idx, rest) =
            found.expect("bug: general matching DP reconstruction found no valid transition");
        pairs.push((i.min(j), i.max(j), w, orig_idx));
        mask = rest;
    }

    let primary_full = pairs.clone();
    let cap = size_cap(input);
    let truncated_away = cap.is_some_and(|c| pairs.len() > c);
    if let Some(cap) = cap {
        pairs = truncate_pairs(pairs, cap);
    }
    let item = |ps: &[(usize, usize, f64, usize)]| -> Json {
        let pairs_json: Vec<Json> = ps
            .iter()
            .map(
                |(a, b, w, idx)| json!({"u": nodes[*a], "v": nodes[*b], "w": w, "edge_index": idx}),
            )
            .collect();
        let (matched_nodes, unmatched_nodes) = matched_unmatched(
            nodes.iter(),
            ps.iter()
                .flat_map(|(a, b, _, _)| [nodes[*a].as_str(), nodes[*b].as_str()]),
        );
        json!({
            "pairs": pairs_json,
            "total_weight": ps.iter().map(|p| p.2).sum::<f64>(),
            "matched_nodes": matched_nodes,
            "unmatched_nodes": unmatched_nodes,
        })
    };
    let mut out = item(&pairs);
    let o = out.as_object_mut().expect("对象");
    o.insert("algo".into(), json!("matching"));
    o.insert("mode".into(), json!("general_exact"));
    o.insert("exact".into(), json!(true));
    o.insert("node_count".into(), json!(n));

    // C-8：并列的最大权匹配（位掩码 DP 上逐点回溯，`dp` 是精确的子集最优，所以不会走进死路）
    let mut found = vec![primary_full];
    let mut overflow = false;
    let mut chosen = Vec::new();
    gen_rec(&best, &dp, n, full, &mut chosen, &mut found, &mut overflow);
    let mut ties: Vec<Vec<(usize, usize, f64, usize)>> = Vec::new();
    for t in found {
        let t = match cap {
            Some(c) => truncate_pairs(t, c),
            None => t,
        };
        if !ties.iter().any(|x| same_pairs(x, &t)) {
            ties.push(t);
        }
    }
    let complete = !overflow && !truncated_away;
    Ok(with_ties(
        out,
        ties.iter().map(|t| item(t)).collect(),
        complete,
    ))
}

/// 一般图匹配的并列枚举：沿最低位节点「不配」或「配某个邻居」逐点回溯，只走 `dp[mask]` 允许的分支
/// （相对容差相等）。只看权 > 0 的边，与主解只含正权对一致（零权对不改变总权，不算另一种匹配）。
fn gen_rec(
    best: &HashMap<(usize, usize), (f64, usize)>,
    dp: &[f64],
    n: usize,
    mask: usize,
    chosen: &mut Vec<(usize, usize, f64, usize)>,
    found: &mut Vec<Vec<(usize, usize, f64, usize)>>,
    overflow: &mut bool,
) {
    if *overflow {
        return;
    }
    if mask == 0 {
        if !found.iter().any(|x| same_pairs(x, chosen)) {
            if found.len() >= TIES_CAP {
                *overflow = true;
                return;
            }
            found.push(chosen.clone());
        }
        return;
    }
    let i = mask.trailing_zeros() as usize;
    let without_i = mask & !(1 << i);
    if close(dp[without_i], dp[mask]) {
        gen_rec(best, dp, n, without_i, chosen, found, overflow);
    }
    for j in 0..n {
        if j == i || mask & (1 << j) == 0 {
            continue;
        }
        let Some(&(w, idx)) = best.get(&(i.min(j), i.max(j))) else {
            continue;
        };
        if w <= 0.0 {
            continue;
        }
        let rest = without_i & !(1 << j);
        if close(dp[rest] + w, dp[mask]) {
            chosen.push((i.min(j), i.max(j), w, idx));
            gen_rec(best, dp, n, rest, chosen, found, overflow);
            chosen.pop();
        }
    }
}

// ---------- graph:shortest_path ----------
//
// 输入：`{"edges":[{"u","v","w"}], "nodes":[...]?, "directed": bool?(默认false),
//        "source", "target"}`。权重非负（Dijkstra 前提），负权在解析期报错。
// 输出：`{"algo":"shortest_path","reachable":bool,"distance":f64|null,"path":[...],
//        "path_edges":[{"u","v","w","edge_index"}],"exact":true}`。

#[derive(PartialEq)]
struct OrderedF64(f64);
impl Eq for OrderedF64 {}
impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Equal)
    }
}

fn run_shortest_path(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, false)?;
    let nodes = collect_nodes(input, &edges)?;
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let directed = get_bool(input, "directed", false);
    let source = get_node_field(input, "source")?;
    let target = get_node_field(input, "target")?;
    let s = *idx
        .get(source.as_str())
        .ok_or_else(|| format!("source '{source}' is not a node"))?;
    let t = *idx
        .get(target.as_str())
        .ok_or_else(|| format!("target '{target}' is not a node"))?;

    let n = nodes.len();
    let mut adj: Vec<Vec<(usize, f64, usize)>> = vec![Vec::new(); n];
    for e in &edges {
        let a = idx[e.u.as_str()];
        let b = idx[e.v.as_str()];
        adj[a].push((b, e.w, e.index));
        if !directed {
            adj[b].push((a, e.w, e.index));
        }
    }

    let mut dist = vec![f64::INFINITY; n];
    let mut prev: Vec<Option<(usize, usize)>> = vec![None; n];
    dist[s] = 0.0;
    let mut heap: BinaryHeap<std::cmp::Reverse<(OrderedF64, usize)>> = BinaryHeap::new();
    heap.push(std::cmp::Reverse((OrderedF64(0.0), s)));
    let mut visited = vec![false; n];
    while let Some(std::cmp::Reverse((OrderedF64(d), u))) = heap.pop() {
        if visited[u] {
            continue;
        }
        visited[u] = true;
        // C-8：不在弹出 t 时提前退出——并列的最短路要用到每个节点的最终距离（含与 t 等距的节点，
        // 零权边会让它们落在最短路上）。t 的距离与 `prev[t]` 在它弹出时已定，继续跑不会改主解。
        for &(v2, w, ei) in &adj[u] {
            let nd = d + w;
            // 标准 Dijkstra 配合 `visited` 标记不需要「改进幅度」门槛（PR #36 复核 P2）：
            // `- 1e-12` 的绝对阈值在边权远小于它时会把真实的更优路径当成「没有改进」而吞掉，
            // 静默算出错误的最短路却仍标 `exact:true`；纯 `<` 既正确又不受量纲影响。
            if nd < dist[v2] {
                dist[v2] = nd;
                prev[v2] = Some((u, ei));
                heap.push(std::cmp::Reverse((OrderedF64(nd), v2)));
            }
        }
    }

    if dist[t].is_infinite() {
        return Ok(with_ties(
            json!({
                "algo": "shortest_path", "reachable": false, "distance": Json::Null,
                "path": Json::Array(vec![]), "path_edges": Json::Array(vec![]), "exact": true,
            }),
            vec![],
            true,
        ));
    }
    let mut path_nodes = vec![t];
    let mut path_edges_idx = Vec::new();
    let mut cur = t;
    while let Some((p, ei)) = prev[cur] {
        path_edges_idx.push(ei);
        path_nodes.push(p);
        cur = p;
    }
    path_nodes.reverse();
    path_edges_idx.reverse();
    let path_ids: Vec<String> = path_nodes.iter().map(|&i| nodes[i].clone()).collect();
    let path_edges_json: Vec<Json> = path_edges_idx
        .iter()
        .map(|&ei| {
            let e = &edges[ei];
            json!({"u": e.u, "v": e.v, "w": e.w, "edge_index": ei})
        })
        .collect();

    // C-8：并列的最短路（同样长的其他路径）。只走「紧弧」（dist[u] + w ≈ dist[v]），且只走能到 t 的节点，
    // 所以不会走进死路；路径是简单路径（零权环不会把同一路径重复算成不同解）；按边下标序列区分，
    // 同一对节点间的平行边走哪条是不同的解（`interval` 用到的边不同）。
    let item = |nodes_seq: &[usize], edges_seq: &[usize]| -> Json {
        json!({
            "distance": dist[t],
            "path": nodes_seq.iter().map(|&i| nodes[i].clone()).collect::<Vec<_>>(),
            "path_edges": edges_seq.iter().map(|&ei| {
                let e = &edges[ei];
                json!({"u": e.u, "v": e.v, "w": e.w, "edge_index": ei})
            }).collect::<Vec<_>>(),
        })
    };
    let tight = |u: usize, v: usize, w: f64| dist[u].is_finite() && close(dist[u] + w, dist[v]);
    // 反向可达：能沿紧弧走到 t 的节点
    let mut radj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (u, edges) in adj.iter().enumerate().take(n) {
        for &(v2, w, _) in edges {
            if tight(u, v2, w) {
                radj[v2].push(u);
            }
        }
    }
    let mut reach_t = vec![false; n];
    reach_t[t] = true;
    let mut stack = vec![t];
    while let Some(x) = stack.pop() {
        for &p in &radj[x] {
            if !reach_t[p] {
                reach_t[p] = true;
                stack.push(p);
            }
        }
    }
    let mut found: Vec<(Vec<usize>, Vec<usize>)> =
        vec![(path_nodes.clone(), path_edges_idx.clone())];
    let mut overflow = false;
    let mut on_path = vec![false; n];
    on_path[s] = true;
    let mut ns = vec![s];
    let mut es: Vec<usize> = Vec::new();
    sp_rec(
        s,
        t,
        &adj,
        &tight,
        &reach_t,
        &mut on_path,
        &mut ns,
        &mut es,
        &mut found,
        &mut overflow,
    );
    let ties: Vec<Json> = found.iter().map(|(a, b)| item(a, b)).collect();

    Ok(with_ties(
        json!({
            "algo": "shortest_path",
            "reachable": true,
            "distance": dist[t],
            "path": path_ids,
            "path_edges": path_edges_json,
            "exact": true,
        }),
        ties,
        !overflow,
    ))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sp_rec(
    u: usize,
    t: usize,
    adj: &[Vec<(usize, f64, usize)>],
    tight: &dyn Fn(usize, usize, f64) -> bool,
    reach_t: &[bool],
    on_path: &mut Vec<bool>,
    ns: &mut Vec<usize>,
    es: &mut Vec<usize>,
    found: &mut Vec<(Vec<usize>, Vec<usize>)>,
    overflow: &mut bool,
) {
    if *overflow {
        return;
    }
    if u == t {
        if !found.iter().any(|(_, e)| e == &*es) {
            if found.len() >= TIES_CAP {
                *overflow = true;
                return;
            }
            found.push((ns.clone(), es.clone()));
        }
        return;
    }
    for &(v2, w, ei) in &adj[u] {
        if on_path[v2] || !reach_t[v2] || !tight(u, v2, w) {
            continue;
        }
        on_path[v2] = true;
        ns.push(v2);
        es.push(ei);
        sp_rec(v2, t, adj, tight, reach_t, on_path, ns, es, found, overflow);
        es.pop();
        ns.pop();
        on_path[v2] = false;
        if *overflow {
            return;
        }
    }
}

// ---------- graph:max_clique ----------
//
// 输入：`{"edges":[{"u","v"}], "nodes":[...]?}`（无向、不带权；`w` 若给忽略）。
// `n ≤ 60`（附注 §五原文界，超界报错）。输出：
// `{"algo":"max_clique","clique":[...],"size","edge_indices":[...],"exact":true,"node_count"}`。

const MAX_CLIQUE_MAX_N: usize = 60;

fn run_max_clique(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, true)?;
    let nodes = collect_nodes(input, &edges)?;
    let n = nodes.len();
    if n > MAX_CLIQUE_MAX_N {
        return Err(format!(
            "graph:max_clique supports at most {MAX_CLIQUE_MAX_N} nodes (branch and bound over 2^n \
             subsets); got {n}"
        ));
    }
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut adj = vec![0u64; n.max(1)];
    let mut edge_lookup: HashMap<(usize, usize), usize> = HashMap::new();
    for e in &edges {
        let a = *idx
            .get(e.u.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.u))?;
        let b = *idx
            .get(e.v.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.v))?;
        adj[a] |= 1u64 << b;
        adj[b] |= 1u64 << a;
        edge_lookup.entry((a.min(b), a.max(b))).or_insert(e.index);
    }
    let full: u64 = if n == 0 {
        0
    } else if n == 64 {
        u64::MAX
    } else {
        (1u64 << n) - 1
    };
    let mut best_r: u64 = 0;
    let mut best_size: u32 = 0;
    bron_kerbosch(0, full, 0, &adj, &mut best_r, &mut best_size);

    // 一个团（位集）→ 与主解同形的 `{clique, size, edge_indices}`
    let item = |mask: u64| -> Json {
        let mut clique: Vec<usize> = (0..n).filter(|&i| mask & (1 << i) != 0).collect();
        clique.sort_unstable();
        let mut edge_indices = Vec::new();
        for a_pos in 0..clique.len() {
            for b_pos in (a_pos + 1)..clique.len() {
                let (a, b) = (clique[a_pos], clique[b_pos]);
                if let Some(&ei) = edge_lookup.get(&(a.min(b), a.max(b))) {
                    edge_indices.push(ei);
                }
            }
        }
        let clique_ids: Vec<String> = clique.iter().map(|&i| nodes[i].clone()).collect();
        json!({"clique": clique_ids, "size": clique.len(), "edge_indices": edge_indices})
    };
    let mut out = item(best_r);
    let o = out.as_object_mut().expect("对象");
    o.insert("algo".into(), json!("max_clique"));
    o.insert("exact".into(), json!(true));
    o.insert("node_count".into(), json!(n));

    // C-8：并列的最大团。第二趟只收大小等于 `best_size` 的极大团（最大团一定极大，Bron–Kerbosch 每个只出一次），
    // 剪枝改成严格小于；主解排第一。
    let mut found = vec![best_r];
    let mut overflow = false;
    if n > 0 {
        bk_collect(
            0,
            full,
            0,
            &adj,
            best_size,
            best_r,
            &mut found,
            &mut overflow,
        );
    }
    Ok(with_ties(
        out,
        found.iter().map(|&m| item(m)).collect(),
        !overflow,
    ))
}

/// 收集全部大小为 `target` 的极大团（不含已在 `found` 里的主解 `primary`），到上限就标溢出。
#[allow(clippy::too_many_arguments)]
fn bk_collect(
    r: u64,
    p: u64,
    x: u64,
    adj: &[u64],
    target: u32,
    primary: u64,
    found: &mut Vec<u64>,
    overflow: &mut bool,
) {
    if *overflow {
        return;
    }
    if p == 0 && x == 0 {
        if r.count_ones() == target && r != primary {
            if found.len() >= TIES_CAP {
                *overflow = true;
                return;
            }
            found.push(r);
        }
        return;
    }
    if r.count_ones() + p.count_ones() < target {
        return;
    }
    let px = p | x;
    let mut pivot = px.trailing_zeros() as usize;
    let mut best_count = -1i32;
    let mut scan = px;
    while scan != 0 {
        let u = scan.trailing_zeros() as usize;
        let cnt = (p & adj[u]).count_ones() as i32;
        if cnt > best_count {
            best_count = cnt;
            pivot = u;
        }
        scan &= scan - 1;
    }
    let mut candidates = p & !adj[pivot];
    let mut pp = p;
    let mut xx = x;
    while candidates != 0 {
        let v = candidates.trailing_zeros() as usize;
        let vb = 1u64 << v;
        bk_collect(
            r | vb,
            pp & adj[v],
            xx & adj[v],
            adj,
            target,
            primary,
            found,
            overflow,
        );
        pp &= !vb;
        xx |= vb;
        candidates &= !vb;
    }
}

/// Bron–Kerbosch with pivoting，带「当前团 + 候选数 ≤ 已知最优即剪」的分支定界（派单要求「小规模
/// 分支定界」）。`r`/`p`/`x` 是标准三个位集：当前团、候选、已排除。
fn bron_kerbosch(r: u64, p: u64, x: u64, adj: &[u64], best_r: &mut u64, best_size: &mut u32) {
    if p == 0 && x == 0 {
        let sz = r.count_ones();
        if sz > *best_size {
            *best_size = sz;
            *best_r = r;
        }
        return;
    }
    if r.count_ones() + p.count_ones() <= *best_size {
        return;
    }
    let px = p | x;
    let mut pivot = px.trailing_zeros() as usize;
    let mut best_count = -1i32;
    let mut scan = px;
    while scan != 0 {
        let u = scan.trailing_zeros() as usize;
        let cnt = (p & adj[u]).count_ones() as i32;
        if cnt > best_count {
            best_count = cnt;
            pivot = u;
        }
        scan &= scan - 1;
    }
    let mut candidates = p & !adj[pivot];
    let mut pp = p;
    let mut xx = x;
    while candidates != 0 {
        let v = candidates.trailing_zeros() as usize;
        let vb = 1u64 << v;
        bron_kerbosch(r | vb, pp & adj[v], xx & adj[v], adj, best_r, best_size);
        pp &= !vb;
        xx |= vb;
        candidates &= !vb;
    }
}

// ---------- graph:components ----------
//
// 输入：`{"edges":[{"u","v"}], "nodes":[...]?}`（权重忽略）。输出：
// `{"algo":"components","components":[[...],[...]],"count","exact":true,"node_count"}`
// （每个分量成员排序，分量按首成员排序，确定性输出供金样比较）。

fn find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        parent[x] = find(parent, parent[x]);
    }
    parent[x]
}

fn union(parent: &mut [usize], rank: &mut [u32], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra == rb {
        return;
    }
    match rank[ra].cmp(&rank[rb]) {
        Ordering::Less => parent[ra] = rb,
        Ordering::Greater => parent[rb] = ra,
        Ordering::Equal => {
            parent[rb] = ra;
            rank[ra] += 1;
        }
    }
}

fn run_components(input: &Json) -> Result<Json, String> {
    let edges = parse_edges(input, "w", 1.0, true)?;
    let nodes = collect_nodes(input, &edges)?;
    let n = nodes.len();
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank = vec![0u32; n];
    for e in &edges {
        let a = *idx
            .get(e.u.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.u))?;
        let b = *idx
            .get(e.v.as_str())
            .ok_or_else(|| format!("edges[{}] references unknown node '{}'", e.index, e.v))?;
        union(&mut parent, &mut rank, a, b);
    }
    let mut groups: HashMap<usize, Vec<String>> = HashMap::new();
    for (i, node) in nodes.iter().enumerate().take(n) {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(node.clone());
    }
    let mut components: Vec<Vec<String>> = groups.into_values().collect();
    for c in &mut components {
        c.sort();
    }
    components.sort_by(|a, b| a.first().cmp(&b.first()));
    let count = components.len();

    // C-8：连通分量的划分是唯一的（没有并列的可能），恒 `tie_count=1`
    let item = json!({"components": components, "count": count});
    Ok(with_ties(
        json!({
            "algo": "components",
            "components": components,
            "count": count,
            "exact": true,
            "node_count": n,
        }),
        vec![item],
        true,
    ))
}

// ---------- graph:set_cover ----------
//
// 输入：`{"universe":[...], "sets":[{"id":Text?,"elements":[...],"cost":f64?(默认1)}]}`。
// `|universe| ≤ 20` 走精确位掩码 DP，否则贪心（附注 §五原文界）。输出：
// `{"algo":"set_cover","chosen":[...],"total_cost","exact":bool,"covers_universe":bool,"universe_size"}`。

struct SetIn {
    id: String,
    elems: Vec<usize>,
    cost: f64,
}

const SET_COVER_EXACT_MAX: usize = 20;

fn run_set_cover(input: &Json) -> Result<Json, String> {
    let universe = get_arr(input, "universe")?
        .iter()
        .map(node_id)
        .collect::<Result<Vec<_>, _>>()?;
    let u_idx: HashMap<&str, usize> = universe
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    if u_idx.len() != universe.len() {
        return Err("'universe' contains duplicate elements".into());
    }
    let sets_arr = get_arr(input, "sets")?;
    let mut sets = Vec::with_capacity(sets_arr.len());
    for (i, s) in sets_arr.iter().enumerate() {
        let id = match s.get("id") {
            Some(v) => node_id(v)?,
            None => i.to_string(),
        };
        let elems_json = s
            .get("elements")
            .and_then(|e| e.as_array())
            .ok_or_else(|| format!("sets[{i}] missing 'elements' array"))?;
        let mut elems = Vec::new();
        for e in elems_json {
            let nid = node_id(e)?;
            if let Some(&ui) = u_idx.get(nid.as_str())
                && !elems.contains(&ui)
            {
                elems.push(ui);
            }
            // 不在 universe 里的元素静默忽略：它们无助于覆盖 universe，不是错误。
        }
        let cost = match s.get("cost") {
            Some(Json::Number(n)) => n
                .as_f64()
                .ok_or_else(|| format!("sets[{i}].cost is not a finite number"))?,
            Some(other) => return Err(format!("sets[{i}].cost must be a number, got {other}")),
            None => 1.0,
        };
        if cost < 0.0 {
            return Err(format!(
                "sets[{i}].cost = {cost} is negative; set cover requires non-negative costs"
            ));
        }
        sets.push(SetIn { id, elems, cost });
    }

    if universe.len() <= SET_COVER_EXACT_MAX {
        set_cover_exact(&universe, &sets)
    } else {
        set_cover_greedy(&universe, &sets)
    }
}

fn set_cover_exact(universe: &[String], sets: &[SetIn]) -> Result<Json, String> {
    let u = universe.len();
    let full: u32 = if u == 0 { 0 } else { ((1u64 << u) - 1) as u32 };
    let masks: Vec<u32> = sets
        .iter()
        .map(|s| s.elems.iter().fold(0u32, |m, &e| m | (1 << e)))
        .collect();
    let size = 1usize << u;
    const INF: f64 = f64::INFINITY;
    let mut dp = vec![INF; size];
    dp[0] = 0.0;
    let mut choice: Vec<Option<(usize, usize)>> = vec![None; size];
    for mask in 0..size {
        if dp[mask].is_infinite() {
            continue;
        }
        let m32 = mask as u32;
        if m32 == full {
            continue;
        }
        let uncovered = (!m32) & full;
        let bit = uncovered.trailing_zeros();
        for (si, s) in sets.iter().enumerate() {
            if masks[si] & (1 << bit) == 0 {
                continue;
            }
            let nm = (m32 | masks[si]) as usize;
            let nc = dp[mask] + s.cost;
            // 单趟正向松弛、掩码按升序处理，不需要「改进幅度」门槛，理由同 shortest_path
            // （PR #36 复核 P2）：`- 1e-12` 在成本远小于它时会吞掉真实更优的覆盖方案。
            if nc < dp[nm] {
                dp[nm] = nc;
                choice[nm] = Some((mask, si));
            }
        }
    }
    let full_usize = full as usize;
    if dp[full_usize].is_infinite() {
        return Ok(with_ties(
            json!({
                "algo": "set_cover", "chosen": Json::Array(vec![]), "total_cost": Json::Null,
                "exact": true, "covers_universe": false, "universe_size": u,
            }),
            vec![],
            true,
        ));
    }
    let mut chosen_idx = Vec::new();
    let mut mask = full_usize;
    while mask != 0 {
        let (prev, si) =
            choice[mask].expect("bug: set_cover_exact reconstruction reached unset choice");
        chosen_idx.push(si);
        mask = prev;
    }
    chosen_idx.sort_unstable();
    chosen_idx.dedup();
    let total_cost: f64 = chosen_idx.iter().map(|&i| sets[i].cost).sum();
    let chosen: Vec<String> = chosen_idx.iter().map(|&i| sets[i].id.clone()).collect();

    // C-8：并列的最小成本覆盖。反向 DP `g[mask]` = 已覆盖 mask 时覆盖其余元素的最小成本，
    // 逐点回溯只走「前缀成本 + 这一集合成本 + g[新掩码] ≈ 最优」的分支，所以不会走进死路；
    // 只收无冗余的覆盖（每个被选集合都有独占元素——零成本集合可以任意叠加，不算另一种覆盖），主解排第一。
    let opt = dp[full_usize];
    let mut g = vec![INF; size];
    g[full_usize] = 0.0;
    for mask in (0..size).rev() {
        let m32 = mask as u32;
        if m32 == full {
            continue;
        }
        let bit = ((!m32) & full).trailing_zeros();
        for (si, s) in sets.iter().enumerate() {
            if masks[si] & (1 << bit) == 0 {
                continue;
            }
            let c = s.cost + g[(m32 | masks[si]) as usize];
            if c < g[mask] {
                g[mask] = c;
            }
        }
    }
    let irredundant = |pick: &[usize]| -> bool {
        pick.iter().all(|&x| {
            let others = pick
                .iter()
                .filter(|&&y| y != x)
                .fold(0u32, |m, &y| m | masks[y]);
            others != full
        })
    };
    let mut found: Vec<Vec<usize>> = vec![chosen_idx.clone()];
    let mut seen: std::collections::HashSet<Vec<usize>> = std::collections::HashSet::new();
    let mut overflow = false;
    let mut pick: Vec<usize> = Vec::new();
    let ctx = ScCtx {
        sets,
        masks: &masks,
        g: &g,
        full,
        opt,
        irredundant: &irredundant,
    };
    sc_rec(
        &ctx,
        0,
        0.0,
        &mut pick,
        &mut seen,
        &mut found,
        &mut overflow,
    );
    let item = |ix: &[usize]| -> Json {
        json!({
            "chosen": ix.iter().map(|&i| sets[i].id.clone()).collect::<Vec<_>>(),
            "total_cost": ix.iter().map(|&i| sets[i].cost).sum::<f64>(),
        })
    };

    Ok(with_ties(
        json!({
            "algo": "set_cover", "chosen": chosen, "total_cost": total_cost,
            "exact": true, "covers_universe": true, "universe_size": u,
        }),
        found.iter().map(|ix| item(ix)).collect(),
        !overflow,
    ))
}

struct ScCtx<'a> {
    sets: &'a [SetIn],
    masks: &'a [u32],
    g: &'a [f64],
    full: u32,
    opt: f64,
    irredundant: &'a dyn Fn(&[usize]) -> bool,
}

fn sc_rec(
    ctx: &ScCtx,
    mask: u32,
    cw: f64,
    pick: &mut Vec<usize>,
    seen: &mut std::collections::HashSet<Vec<usize>>,
    found: &mut Vec<Vec<usize>>,
    overflow: &mut bool,
) {
    if *overflow {
        return;
    }
    let mut key = pick.clone();
    key.sort_unstable();
    if !seen.insert(key.clone()) {
        return;
    }
    if mask == ctx.full {
        if (ctx.irredundant)(&key) && !found.contains(&key) {
            if found.len() >= TIES_CAP {
                *overflow = true;
                return;
            }
            found.push(key);
        }
        return;
    }
    let bit = ((!mask) & ctx.full).trailing_zeros();
    for (si, s) in ctx.sets.iter().enumerate() {
        if ctx.masks[si] & (1 << bit) == 0 {
            continue;
        }
        let nm = mask | ctx.masks[si];
        if !close(cw + s.cost + ctx.g[nm as usize], ctx.opt) {
            continue;
        }
        pick.push(si);
        sc_rec(ctx, nm, cw + s.cost, pick, seen, found, overflow);
        pick.pop();
        if *overflow {
            return;
        }
    }
}

fn set_cover_greedy(universe: &[String], sets: &[SetIn]) -> Result<Json, String> {
    let u = universe.len();
    let mut covered = vec![false; u];
    let mut remaining = u;
    let mut chosen = Vec::new();
    let mut total_cost = 0.0;
    let mut used = vec![false; sets.len()];
    while remaining > 0 {
        let mut best_idx: Option<usize> = None;
        let mut best_ratio = -1.0f64;
        for (i, s) in sets.iter().enumerate() {
            if used[i] {
                continue;
            }
            let new_cover = s.elems.iter().filter(|&&e| !covered[e]).count();
            if new_cover == 0 {
                continue;
            }
            let cost = if s.cost <= 0.0 { 1e-9 } else { s.cost };
            let ratio = new_cover as f64 / cost;
            if ratio > best_ratio {
                best_ratio = ratio;
                best_idx = Some(i);
            }
        }
        let Some(bi) = best_idx else {
            break;
        };
        used[bi] = true;
        for &e in &sets[bi].elems {
            if !covered[e] {
                covered[e] = true;
                remaining -= 1;
            }
        }
        chosen.push(bi);
        total_cost += sets[bi].cost;
    }
    let covers_universe = remaining == 0;
    let chosen_ids: Vec<String> = chosen.iter().map(|&i| sets[i].id.clone()).collect();

    // C-8：贪心不是精确算法（`exact:false`），无法保证列全并列解，只报它自己给的这一个，`ties_complete=false`
    let item = json!({"chosen": chosen_ids, "total_cost": total_cost});
    Ok(with_ties(
        json!({
            "algo": "set_cover", "chosen": chosen_ids, "total_cost": total_cost,
            "exact": false, "covers_universe": covers_universe, "universe_size": u,
        }),
        vec![item],
        false,
    ))
}

// ---------- graph:max_flow ----------
//
// 输入：`{"edges":[{"u","v","cap"}], "nodes":[...]?, "source", "sink"}`（有向，容量非负）。
// 输出：`{"algo":"max_flow","max_flow":f64,"flow_edges":[{"u","v","flow","edge_index"}],
//        "exact":true,"node_count"}`。Dinic（BFS 分层 + DFS 阻塞流 + 当前弧优化）。

struct FlowEdge {
    from: usize,
    to: usize,
    cap: f64,
    flow: f64,
    orig_index: Option<usize>,
}

struct Dinic {
    graph: Vec<Vec<usize>>,
    edges: Vec<FlowEdge>,
    n: usize,
    /// 残量比较用的阈值，按输入容量的量纲取相对值（`new` 里算），不是写死的绝对数
    /// （PR #36 复核 P2：固定 `1e-9` 在所有容量都远小于它时会把真实容量当成零，
    /// 静默丢掉网络里的边却仍标 `exact:true`）。阈值本身不能省——Dinic 需要它避免浮点残量
    /// 在 0 附近抖动导致的死循环，只是刻度要跟着输入走。
    eps: f64,
}

impl Dinic {
    fn new(n: usize, max_cap: f64) -> Self {
        Dinic {
            graph: vec![Vec::new(); n],
            edges: Vec::new(),
            n,
            eps: (max_cap * 1e-9).max(0.0),
        }
    }

    fn add_edge(&mut self, u: usize, v: usize, cap: f64, orig_index: Option<usize>) {
        let e1 = self.edges.len();
        self.edges.push(FlowEdge {
            from: u,
            to: v,
            cap,
            flow: 0.0,
            orig_index,
        });
        self.graph[u].push(e1);
        let e2 = self.edges.len();
        self.edges.push(FlowEdge {
            from: v,
            to: u,
            cap: 0.0,
            flow: 0.0,
            orig_index: None,
        });
        self.graph[v].push(e2);
    }

    fn bfs(&self, s: usize, t: usize, level: &mut [i32]) -> bool {
        level.iter_mut().for_each(|l| *l = -1);
        level[s] = 0;
        let mut q = VecDeque::new();
        q.push_back(s);
        while let Some(u) = q.pop_front() {
            for &ei in &self.graph[u] {
                let e = &self.edges[ei];
                if level[e.to] < 0 && e.cap - e.flow > self.eps {
                    level[e.to] = level[u] + 1;
                    q.push_back(e.to);
                }
            }
        }
        level[t] >= 0
    }

    fn dfs(&mut self, u: usize, t: usize, f: f64, level: &[i32], it: &mut [usize]) -> f64 {
        if u == t {
            return f;
        }
        while it[u] < self.graph[u].len() {
            let ei = self.graph[u][it[u]];
            let (to, cap, flow) = {
                let e = &self.edges[ei];
                (e.to, e.cap, e.flow)
            };
            if level[to] == level[u] + 1 && cap - flow > self.eps {
                let d = self.dfs(to, t, f.min(cap - flow), level, it);
                if d > self.eps {
                    self.edges[ei].flow += d;
                    let rev = ei ^ 1;
                    self.edges[rev].flow -= d;
                    return d;
                }
            }
            it[u] += 1;
        }
        0.0
    }

    /// 最大流是否唯一（C-8）：最大流值本身唯一，并列的是边上的流量分配。残量图里有一个「非平凡」有向环
    /// （不是某条边与它自己的反向弧来回）就能沿环改流量、得到另一个同值的最大流；没有这样的环则分配唯一。
    /// 环上的残量按 `eps` 判正。返回 `true` = 唯一。
    fn flow_is_unique(&self) -> bool {
        // 状态：0 未访问，1 在栈上，2 已完成
        let mut state = vec![0u8; self.n];
        let mut enter_arc: Vec<Option<usize>> = vec![None; self.n];
        for root in 0..self.n {
            if state[root] != 0 {
                continue;
            }
            // 栈元素：(节点, 该节点已扫到 graph[u] 的第几条)
            let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
            state[root] = 1;
            while let Some(&mut (u, ref mut k)) = stack.last_mut() {
                if *k >= self.graph[u].len() {
                    state[u] = 2;
                    stack.pop();
                    continue;
                }
                let arc = self.graph[u][*k];
                *k += 1;
                let e = &self.edges[arc];
                if e.cap - e.flow <= self.eps {
                    continue;
                }
                // 跳过刚才进入本节点的那条弧的反向弧：来回走同一条边不算环
                if enter_arc[u].is_some_and(|p| p ^ 1 == arc) {
                    continue;
                }
                match state[e.to] {
                    1 => return false,
                    0 => {
                        state[e.to] = 1;
                        enter_arc[e.to] = Some(arc);
                        stack.push((e.to, 0));
                    }
                    _ => {}
                }
            }
        }
        true
    }

    fn max_flow(&mut self, s: usize, t: usize) -> f64 {
        let mut flow = 0.0;
        let mut level = vec![-1i32; self.n];
        while self.bfs(s, t, &mut level) {
            let mut it = vec![0usize; self.n];
            loop {
                let f = self.dfs(s, t, f64::INFINITY, &level, &mut it);
                if f <= self.eps {
                    break;
                }
                flow += f;
            }
        }
        flow
    }
}

fn run_max_flow(input: &Json) -> Result<Json, String> {
    let edges_raw = parse_edges(input, "cap", 1.0, false)?;
    let nodes = collect_nodes(input, &edges_raw)?;
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let source = get_node_field(input, "source")?;
    let sink = get_node_field(input, "sink")?;
    let s = *idx
        .get(source.as_str())
        .ok_or_else(|| format!("source '{source}' is not a node"))?;
    let t = *idx
        .get(sink.as_str())
        .ok_or_else(|| format!("sink '{sink}' is not a node"))?;
    if s == t {
        return Err("source and sink must be different nodes".into());
    }

    let n = nodes.len();
    // 相对阈值：按这张图自己的容量量纲取（PR #36 复核 P2），不是写死的绝对数——
    // `parse_edges(..., false)` 已经拒绝了负容量，`fold` 从 0.0 开始安全。
    let max_cap = edges_raw.iter().map(|e| e.w).fold(0.0_f64, f64::max);
    let mut din = Dinic::new(n, max_cap);
    for e in &edges_raw {
        let a = idx[e.u.as_str()];
        let b = idx[e.v.as_str()];
        din.add_edge(a, b, e.w, Some(e.index));
    }
    let flow = din.max_flow(s, t);
    let eps = din.eps;

    let mut flow_edges_json = Vec::new();
    for e in &din.edges {
        if let Some(orig) = e.orig_index
            && e.flow > eps
        {
            flow_edges_json.push(json!({
                "u": nodes[e.from], "v": nodes[e.to], "flow": e.flow, "edge_index": orig,
            }));
        }
    }

    Ok(json!({
        "algo": "max_flow",
        "max_flow": flow,
        "flow_edges": flow_edges_json,
        "exact": true,
        "node_count": n,
        // C-8：最大流值唯一，并列的是流量分配；不枚举（个数无界、流量连续），只报分配是否唯一
        "flow_unique": din.flow_is_unique(),
    }))
}
