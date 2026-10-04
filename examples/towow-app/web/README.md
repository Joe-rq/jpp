# 通爻网实时 3D 前端

personal agent 陌生发现网络的实时画面：判断在空中飞，关系落成海上航线，谈成的构型开垦一座新港。方向与取舍见 [BRIEF.md](BRIEF.md)，锁定规范见 [DESIGN.md](DESIGN.md)，帧率实测见 [PERF.md](PERF.md)。

## 运行

```bash
npm install
npm run dev          # http://localhost:5178/
npm run build        # 静态包输出到 dist/，相对路径，可直接放 GitHub Pages
```

## 事件源（URL 参数）

不给 `?src` 时：页面由宿主托管（或 vite 代理）就先试同源 `/events`，再试 `ws://localhost:8794/events`，各等 3 秒；都连不上就放打包进来的 `replay.jsonl`。左下角最前面写数据出处：「实时」「回放（真机数据）」「回放（真机 JEV 读数，真实接入）」「回放（离线伪读数）」「回放」「模拟数据」，回放加速时后面接「。4 倍速」。回放的出处取文件首行的 `{"type":"meta","source":"jev"|"fixture"|"mock"}`，没有这一行时只写「回放」；meta 同时带 `me` 时写「真实接入」，并自动以它为中心（等同 `?me=`）。

| 参数 | 作用 |
|---|---|
| `?src=ws://localhost:8794/events` | 连 WebSocket；先收 `snapshot`，之后是增量 |
| `?replay=nature` | 放 `public/replay-nature.jsonl`：真实接入——Nature 的 agent（Claude Code，经 MCP）接入 500 个背景 agent，约 72 秒（4 倍速）。meta 带 `me`，自动以它为中心，侧栏常驻且画面让出右侧一列。画面讲的是：召回 32 个候选、判完大多数暗下去、对方来要信息、多给一层、一条成形的航线、另一些被放下（编排规则见 [DESIGN.md](DESIGN.md) §7a）。这份回放里背景 agent 彼此不判断，没有构型，所以看不到开垦新港，那在 `?replay=full` 里看。不先探测后端，直接回放 |
| `?replay=full` | 放 `public/replay.jsonl`：真机 full-500（JEV 真读数），覆盖前 413 人、23 分钟，第 454 人之后额度用完，回放截在那之前 |
| `?src=replay&file=xxx.jsonl` | 回放任意 `.jsonl`（`public/` 下的相对路径），按 `t` 播放 |
| `?speed=4` | 回放倍速，默认 4：10 分钟的运行 2 分半看完。左下角的「每分钟 N 次判断」按事件时间算，不随倍速放大 |
| `?data=jev` | 替没写 meta 行的回放声明出处（jev / fixture / mock） |
| `?me=<agent_id>` | 「我的 agent」视角：镜头以它为中心，侧栏常驻它的机会列表；回放 meta 带 `me` 时自动开启，URL 里的优先 |
| `?src=mock` | 浏览器内模拟源，与真后端同协议（`../design/events.md`） |
| `?n=10000` | 模拟源的节点数（压力模式） |
| `?seed=7` | 模拟源随机种子 |
| `?fps=1` | 右上角显示帧率 |
| `?cruise=1` | 镜头只巡航，不追事件 |
| `?fixdpr=1` | 关掉自动降档（测帧率用） |

回放文件超过 20 MB 时先截取前若干行再放进 `public/`。本地模拟后端：`npm run mock:ws -- --n 500 --port 8787`。录一段模拟回放：`npm run mock:record -- public/replay.jsonl --sec 40`。前端还在等后端补的可选字段见 [NEEDS-BACKEND.md](NEEDS-BACKEND.md)。

## 信息层的读数

点开一座港（或 `?me` 的侧栏），每个机会写：形式与价值档位、成员、**置信度**（决定这个机会的那一道题的读数，只在判「成立」时显示：两两关系是边的决定性题，构型是「这几个人按这个形状合作，能成吗？」；题判「否」时的读数是「不成立」的把握，只写在「再判后不再成立（判『不成立』的把握 0.55）」里，不当成立置信度）、题面、这一对已释放的层（边事件的 `tier_seen`，是两边加起来已经释放给对方的层的并集，不分方向；没有时退回披露事件累积的「对方向这边 / 这边向对方」各解锁到哪一层）、补信息的经过（朱色是拿不准，钠灯色是对方多给了一层与再判）、还缺什么、方案。全部从事件累积；连着宿主时改用 `/api/opportunities` 的同一份渲染。

## 操作

默认是导演模式，镜头按 `spotlight`、`config`、`plan`、`node_join` 排队运镜，每个镜头停 5–11 秒，队空时缓慢巡航；补信息被批准（`disclose_granted`）时推近那一对，字幕说清谁拿不准、要了什么、对方多给了一层，随后的再判读数再补一行。拖动或滚轮转为自由视角，6 秒不动回到导演模式。点一座港看它的机会与披露阶梯，点一座新港看构型与方案；Esc 或点空白处关闭。

## 截图与帧率

`npx tsx scripts/shoot.ts <url> <输出前缀> --at 5,10 [--perf 15] [--w 1600 --h 900] [--hide-text]`，用本机 Chrome 的 GPU。最新一轮截图在 `shots/`（全量回放的导演镜头、信息层、`?me` 侧栏，和真实接入回放 12–66 秒五帧）。
