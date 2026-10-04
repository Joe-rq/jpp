# 事件协议 v0（运行时 → 前端 / 观察者）

2026-10-04 主会话定。网络运行时把它内部发生的每一件事都作为事件发出，前端只消费事件、不调后端逻辑。传输：WebSocket `ws://<host>:8794/events`（默认端口 8794），每条一行 JSON；连上后先收一条 `snapshot`，之后是增量。回放：同格式的 `.jsonl` 文件，按 `t` 时间戳播放。字段可增不可删。

公共字段：`t`（服务器时间，秒，浮点）、`type`。

| type | 字段 | 含义 |
|---|---|---|
| `snapshot` | `nodes[]`, `edges[]`, `configs[]`, `stats` | 当前全图 |
| `node_join` | `id`, `kind`(agent/config), `label`, `host_agent`, `lang`, `city`, `tier`, `vec3?`([x,y,z] 语义布局坐标，可选) | 一个 agent 接入，或一个构型被提升为节点（组合封闭） |
| `node_leave` | `id` | 离开 |
| `disclose` | `id`, `tier`(0–3), `added_chars`, `reason` | 某 agent 多给了一层上下文 |
| `probe` | `from`, `to[]`, `stage`(recall/operator) | 召回阶段：一个信号被路由到一批接收方（粒子从 from 飞向 to） |
| `batch` | `id`, `n_states`, `n_questions`, `latency_ms`, `merged_from` | 运行时合批发出的一次判断调用 |
| `judge` | `a`, `b?`, `config?`, `q`(题的短名), `p`(读数摘要，0–1), `exit`(act/ignore/unsure), `batch` | 一道判断的出口 |
| `unsure_route` | `a`, `b?`, `config?`, `missing`(缺的信息类别), `ask_to[]`(向谁要，可多方), `route`(disclose_request/refine/near_boundary/escalate/drop/return) | 未决的去向 |
| `disclose_request` | `id`, `to`, `from`, `category`, `purpose`, `status`(sent/granted/denied) | 补信息请求及其结果 |
| `edge` | `a`, `b`, `dir`(a>b / b>a / both), `form`(合作形式), `conf`(0–1), `state`(new/up/down/gone) | 两点之间的关系读数变化（持续计算的产物） |
| `config` | `id`, `shape`(pair/relay/chain/ring/team/star/m2m/meta), `members[]`, `roles{}`, `conf`, `stage`(candidate/judged/growing/plan/dissolved) | 一个多方构型的形成、成长、定案或解散 |
| `config_grow` | `id`, `add`, `tighter_p` | 加入一员是否让合作更紧密 |
| `plan` | `config`, `title`, `summary`, `conf` | 详细合作方案写好了 |
| `invalidate` | `cause`(disclose/join/leave), `n_judgments` | 增量重算：一次变化让多少判断失效 |
| `stats` | `agents`, `configs`, `calls`, `questions`, `cost_usd`, `qps`, `cache_hit`, `p50_join_s` | 每秒一条 |
| `spotlight` | `id`, `why` | 导演提示：值得镜头去看的节点或构型 |

2026-10-04 修订：按宿主与前端实际对齐（ask_to 为数组；route 增 refine/near_boundary；judge 增 value、cause；invalidate 增 ids、source_ids；spotlight.why 取 join_first_opp/config_formed/meta_formed/plan_ready/disclose_granted；构型 node_join.id 等于构型 id，带 config、members、shape；node_join.vec3 为语义投影；disclose.tier 0/1/2 对应 t0/t1/t2；端口 8794）。
