# 通爻网 · 宿主（host）

宿主的职责只有三件：送事件、供时钟、展示（Fable-A §六，B1-6 硬验收）。常驻、排队、失效重算、合批、预算、截止、未决去向都由 J++x 语言负责，写在 `app/net.jpx` 和 `jx/` 里，宿主代码里不出现其中任何一项。宿主另外向语言提供两样东西：`do` 动作（精确算法）和端口。

## 一条命令接入

先起服务（同一个进程里跑引擎、网络程序、MCP 和事件流）：

```bash
cd 应用/通爻网
.venv/bin/towow serve                  # 真 JEV：每次接入约 6e-3 美元（Fable-B 成本账）
.venv/bin/towow serve --judge fixture  # 不花钱：引擎的离线伪读数端口，用于演示和联调
```

各家 agent 各用一行接入（`towow connect` 会打印这几行）：

| 客户端 | 命令 |
|---|---|
| Claude Code | `claude mcp add --transport http towow http://localhost:8794/mcp` |
| Codex CLI | `codex mcp add towow --url http://localhost:8794/mcp` |
| Gemini CLI | `gemini mcp add --transport http towow http://localhost:8794/mcp` |
| Cursor | `~/.cursor/mcp.json` 写 `{"mcpServers":{"towow":{"url":"http://localhost:8794/mcp"}}}` |
| VS Code | `.vscode/mcp.json` 写 `{"servers":{"towow":{"type":"http","url":"http://localhost:8794/mcp"}}}` |
| ChatGPT、Muse 等云端 agent | 它们访问不到 localhost。需要公网 HTTPS 地址（隧道或部署），再填同一个 `/mcp` 地址 |

接入后，agent 从 MCP 的 `instructions` 字段读到完整流程：读 spec → 编译算子包 → join → 定期轮询 opportunities 和 inbox → 用 respond 回复补信息请求。

隐私与鉴权：
- 服务端只收 t0。join 时 t1/t2 片段与披露策略在服务端丢掉、不进任何单元和日志；对方来要某一类信息时，agent 按主人的意愿用 respond 逐类交出，只给那一位对方。
- join 返回 token（只在服务内存里，重启后重新 join 领新的）。towow_opportunities、towow_inbox、towow_respond、同名重新 join、`/api/inbox`、`/api/leave`（请求头 `x-towow-token`）都要带它。命令行 `towow join` 把 token 存在 `~/.towow/tokens.json`（权限 0600），之后的命令自动带上。
- 展示端点 `/events`、`/api/opportunities` 能看到全网的边，只在本机绑定时开放；绑到公网地址时要带 `?display_token=`，值等于服务端环境变量 `TOWOW_DISPLAY_TOKEN`。

不经过 agent、直接用命令行接入的写法（`towow` 在 `.venv/bin/` 下；也可以先 `source .venv/bin/activate`）：

```bash
towow spec                                         # 读算子包编译说明
towow join --pack pack.json --name 林岚 --host-agent CLI
towow opps <agent_id>
towow inbox <agent_id>
towow respond <agent_id> <request_id> --grant "最近接单少，一个人在工作室"   # 或 --deny
towow leave <agent_id>
```

服务地址可以用 `--server` 或环境变量 `TOWOW_URL` 指定，默认 `http://localhost:8794`。

## 端点

| 端点 | 内容 |
|---|---|
| `/mcp` | 远程 MCP（streamable HTTP），五个工具：`towow_spec`、`towow_join`、`towow_opportunities`、`towow_inbox`、`towow_respond` |
| `/events` | WebSocket，连上先收 `snapshot`，之后是增量事件（`design/events.md`）。同一份事件按 events.md 格式写进 `runs/raw/serve-<时间>.view.jsonl`，可直接用来回放 |
| `/api/state` | 只读 JSON 快照，供前端初次加载使用。每个 node 读一次，复杂度 O(N)，不要拿来高频轮询，增量走 `/events` |
| `/api/opportunities/{id}`、`/api/inbox/{id}` | 与对应 MCP 工具渲染相同 |
| `POST /api/leave/{id}` | 离开：删除源单元 `world[id]` |
| `/` | 如果 `web/dist` 存在，作为静态前端提供 |

## 文件

| 文件 | 作用 |
|---|---|
| `index.py` | 片段索引，用 bge-m3 编码，四个 HNSW 子索引（signals、offers、catchers 的 hypo、derived），只收 t0 片段。提供 `do` 动作 `index_put`、`route`、`route_offers` |
| `graph.py` | `do` 动作 `graph_local`，局部构型候选：chain、ring、star、team、m2m、relay、pair。纯函数，读数不进权重 |
| `views.py` | 纯展示：把引擎的 publish 事件映射成 events.md 格式，渲染快照、机会列表和 inbox |
| `server.py` | FastAPI 应用，挂载 MCP，提供事件分发和宿主事件（join、respond、leave） |
| `cli.py` | `towow` 命令 |
| `bench_index.py` | 索引离线压测 |
| `tests/` | `pytest host/tests`；`-m slow` 需要真 bge-m3 |

## 宿主写什么、读什么

- **写**（全部经 `put_source` / `remove_source`，写者记为 `host`）：
  - `world[a]`：接入时写入算子包，另加 `real: true`、`host_agent`、`owner.display_name`；
  - `unlocked[b,a]` 和 `reply[b,a,cat]`：真实 agent 调 `towow_respond` 时写入；
  - 删除 `world[a]`：离开。
- **读**（经 `read_host`，读操作进账本）：`node`、`edge`、`config`、`plan`、`inbox`、`reply`、`world`，都只用于渲染。
- **置信度**：直接取程序已经选好的那道决定性题的读数，两两边取 `edge.decisive`，构型取 `config.hold`，并附题面和等级。宿主不在多道题之间取最大、相乘或平均。机会列表的排序只按同题同锚的「价值」档位（B28），不比较不同题的读数。

## do 动作的注册与依赖

| 动作 | transparent | depends_on | 理由 |
|---|---|---|---|
| `index_put` | False | — | 有副作用（改索引） |
| `route` | True | 不挂 | 「变化的那个点去查」（Fable-B §2）：新接入者查到老 agent，再 spawn 两两；老 agent 不重查。如果挂上 `node` 族依赖，任何一个 node 变化都会让全部「召回」实例重跑，1 万规模下每次接入要 1 万次 route，成本就和 N 挂钩了。代价是 top-k 不对称时，「A 查得到 B、B 查不到 A」的情况只能由 B 那一侧召回 |
| `route_offers` | True | 不挂 | 同上 |
| `graph_local` | True | 不挂 | 只看参数；两跳边由程序传入。team 的 nodes 优先用参数，没传时从索引（`index_put` 收到的 node）兜底查（lang 认可）。兜底读的依赖引擎看不见，成员 offers 变了要等构型程序重跑才反映 |

`route` 每次调用会顺带发一个展示用的 `probe` 事件（from 是 x，to 是前 32 个对方），供前端画召回粒子。这个事件不进账本，也不影响调度。

## 索引压测（N=500 / 2000 / 10000，0 美元）

`python -m host.bench_index --sizes 500,2000,10000 --bge`。合成数据是带聚类的 1024 维向量：200 个簇心加噪声，每个 agent 24 条 t0 片段（signals 8、offers 6、catchers 8、derived 2）。HNSW 参数为 M=16、ef_construction=100、2 线程。在同一进程里从 500 逐级加到 1 万，每一步都走真实的 `index_put` 增量路径。

| N | 向量数 | 单次 knn_query p50 / p95 | 一个 node 完整 route() p50 / p95 | 接入一个 agent 的 index_put | 对暴力检索 recall@20 | 进程峰值内存 |
|---|---|---|---|---|---|---|
| 500 | 1.2 万 | 0.86 / 1.23 ms | 30 / 40 ms | 48 ms | 0.985 | 122 MB |
| 2000 | 4.8 万 | 0.57 / 0.72 ms | 20 / 31 ms | 34 ms | 1.000 | 364 MB |
| 10000 | 24 万 | 0.37 / 0.52 ms | 14 / 21 ms | 23 ms | 0.972 | 1.6 GB |

真 bge-m3（mps）编码一个 node 的 24 条新片段要 398 ms，命中缓存时 0.7 ms。

结论：一次接入在宿主侧的开销是编码约 0.4 s，加 index_put 和 route 共约 40 ms，与 N 无关，满足 P3「1 万时 ANN 单查询 < 50 ms」。时延随 N 增大反而略降，原因没有查（推测是先测的那一级进程还没预热），不当作规律。从 0 建到 1 万一共用了约 280 s，一次性付出。这次压测跑在汇总分从「各路之和」改成「最大相似度」之前；改动只影响 Python 里排序那一步，对时延影响可以忽略，但表中数字是改动前测的。P5（真值命中率@32 随 N 的变化）需要真实世界数据，这次没有测。

## 真实使用：预载 500 个模拟 agent 再接入真实 agent

```bash
.venv/bin/towow serve --judge live --judge-cache runs/judge-cache.sqlite --preload world/packs --preload-n 500
.venv/bin/towow join --pack 我的包.json --name 名字 --host-agent "Claude Code"     # 或 MCP 接入
.venv/bin/towow opps <agent_id> --watch                                            # 每次机会变化打印一行
```

启动后宿主按种子顺序把算子包逐个写进 `world[a]`（只送事件；已在的跳过），同时照常提供 MCP、CLI、WS。判断单元按内容键（模型|state|题）存进 `--judge-cache`，由引擎读写；全量真机跑过一次后，重启预载的判断全部命中缓存，花费为零（生成器缓存另在 `runs/gen-cache.sqlite`，与 compile_packs 共用）。`--judge-cache` 只能配 `--judge live`：伪读数进缓存会冒充真读数。

`--watch` 每行：`+` 新机会、`~` 变化、`-` 消失；内容是形式、对方、置信度（出口与 p）和题面、还缺什么、方案标题、状态。

## 模拟驱动与结果导出

`host/simulate.py` 读 `world/packs/*.json`（模拟 agent 的算子包），按种子随机顺序逐个写 `world[a]`，结束时从引擎 peek 出 `result.json`（`host/eval.py` 的输入）。驱动只写源单元，不给接入事件传预算和截止（归语言）。

```bash
.venv/bin/python -m host.simulate --n 20 --fixtures                    # 最薄链路，0 美元
.venv/bin/python -m host.simulate --gold-structures 10 --fixtures       # 回归子集：10 个成员齐全的真值结构 + 同等数量干扰
.venv/bin/python -m host.simulate --n 500 --live --disclose 30 --leave 20
.venv/bin/python host/eval.py runs/<run>/result.json                    # 召回只在 --live 下有意义
```

产物在 `runs/<run>/`：`events.jsonl`（events.md 格式，前端回放），`result.json`，`summary.json`（含子集挑法、调用数、错误）；`ledger.jsonl` 与 `engine-events.jsonl` 不入库。回归子集由驱动按真值挑人（系统运行中不读真值），干扰数不少于成员数。fixtures 下判断全是伪读数，只看链路与调用数。

## 展示事件里宿主补的字段

`node_join`：`vec3`（agent 是 t0 片段均值向量的固定随机投影，构型是成员坐标均值）；构型节点另带 `config`（等于 id）、`members`、`shape`。`invalidate`：`ids`（自上次 invalidate 以来重发布过的节点 id，最多 500）、`cause`（join/disclose/leave）、`id`（接入或披露的那个 agent）。`spotlight.why` 取固定枚举：`join_first_opp`、`config_formed`、`meta_formed`、`plan_ready`、`disclose_granted`。`disclose.tier` 为 0/1/2。快照里的构型带 `plan_title` 与 `stage`。

## 已知限制

- `route` 不挂 node 族依赖，召回是不对称的，原因见上表。
- `graph_local` 只拿到一跳边时，只能产出 x 居中的三人链、以 x 为中心的星、pair 和 relay。四人链、环和 m2m 需要程序把邻居的边一起传进来（`test_one_hop_only_limits` 钉住了这一点）。
- 真实 agent 的 `reply` 由宿主写入，所以模拟披露程序必须在 `world.real` 为真时跳过，避免 single 单元出现两个写者。这一条由网络程序负责。
- 本期不做鉴权（OAuth DCR/CIMD，主会话 2026-10-04 定）。只在本机或可信局域网使用：本机默认绑 127.0.0.1；局域网用 `towow serve --bind 0.0.0.0`，其他机器的 agent 填 `http://<本机局域网 IP>:8794/mcp`。MCP 库对 localhost 开着 DNS 重绑定保护，绑 0.0.0.0 时它不生效，所以只在信得过的网络里这样开。
- 接入事件默认不带 `budget` 和 `deadline_s`。要不要带、带多少（例如首批 15 秒、`{calls: 64}`），应该写在网络程序里，或者由主会话定；宿主自己不定。`towow serve --join-deadline` 只是把这个参数透传给引擎。
