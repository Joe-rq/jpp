# J++ 介绍片 / Intro film

`jpp-v6.mp4`：110 秒，1920×1080，30 fps，带代码合成的配乐。画面由 `jpp-v6.html` 逐帧渲染，配乐由 `music.py` 合成，两者共用 `data/timeline.js`（96 BPM，每场按整小节，切换落在重拍上）。

`jpp-v6.mp4`: 110 s, 1920×1080, 30 fps, with a synthesized score. Frames are rendered from `jpp-v6.html`; `music.py` synthesizes the score from the same beat-aligned timeline.

脚注引用的研究与报告在 `research/18-repetition-in-real-jev-projects.zh-CN.md`、`research/19-what-the-rewrites-cut.zh-CN.md`、`research/2026-09-26-rewrite-study.zh-CN.md` 与 [rewrite-study.html](https://towow-ai.github.io/jpp/rewrite-study.html)。

## 内容

| 时间 | 场景 | 讲什么 |
|---|---|---|
| 0:00 | JEV | 语义判断器：材料加一道题，返回读数；单次调用约 1.2 秒、$0.0004 |
| 0:07 | J++ | 三个基本操作：材料（mat / state）、题（test / select / measure）、出口（cut / handle）；判断由 JEV 完成，批调度、出口路由、账本由运行时完成 |
| 0:17 | 运行时的八项决定 | 81 个开源项目各自手写的比例：判断力缺席 79%、批调度 75%、校准 65%、未决去向 37%、缓存与增量 33%、预算 12%；深度与终止、时延预算没有单独统计 |
| 0:32 | 运行时接管 | 八项决定由 J++ 运行时统一处理 |
| 0:37 | 五个能力 | 三路出口（judge · cut · handle）· 批量筛选（sieve）· 判断建图 + 图算法（judged_bipartite · interval）· 有界搜索（search · ground · keep）· 题树（gen · walk · enrich）；每个配公开仓库里的真实代码 |
| 1:15 | 改写实测 | 8 处原项目缺陷（7 个项目）· 判断器接入样板代码 0 行（响应校验 35→0、重试退避 36→0、候选回退 30→0，固定观察重放下核对）· 批调度向量化 14→1 · 账本与跨运行缓存 |
| 1:27 | 规模 | 通爻网络真机事件流：307 个主体，27,519 次判断 |
| 1:37 | 开始使用 | github.com/towow-ai/jpp · towow-ai.github.io/jpp/demos |

标"示意"的两个动画（批量筛选的分列、判断建图的边）是按常理排的，不是运行结果。

## 文件

| 文件 | 用途 |
|---|---|
| `jpp-v6.html` | 画面源码；浏览器直接打开会实时播放，空格暂停，地址后加 `#t=40` 从第 40 秒开始 |
| `render.cjs` | 逐帧截图并编码成 MP4（需要 Chrome、puppeteer-core、ffmpeg） |
| `music.py` | 用 numpy 合成配乐，输出 `music.wav`（不入库） |
| `data/timeline.js` | 画面与配乐共用的时间轴 |
| `data/net.js` | 通爻网络真机事件流导出的节点与传播数据，由 `extract_data.py` 生成 |
| `assets/cut/` | 两个角色各三种表情、13 个图标；由 `crop_assets.py` 从 Codex 生成的素材表切出，提示词在 `prompts/`，生成脚本 `gen_asset.sh` |

```sh
python3 crop_assets.py
python3 extract_data.py
.venv/bin/python music.py
FFMPEG=/path/to/ffmpeg node render.cjs --page jpp-v6.html --out silent.mp4 --fps 30
ffmpeg -i silent.mp4 -i music.wav -map 0:v -map 1:a -c:v libx264 -crf 20 -pix_fmt yuv420p -c:a aac -b:a 192k -t 110 jpp-v6.mp4
```

