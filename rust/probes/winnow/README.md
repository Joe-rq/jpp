# 阶段 5 探针 / Stage-5 reproduction probes

评估与缺口见 / Evaluation and gaps: `地基/过程记录/阶段5-探针-首轮.md`.

| 目录 | 复现对象 | 固定观察 | 真机 |
|---|---|---|---|
| `winnow/` | GhalebDweikat/winnow（工具输出逐块筛选） | `jpp run winnow.jpp --fixtures fixture.json` | `--backend live --calib <form-topic 线目录>`；报错题的线在 `truth/calib/` |
| `folio/` | jev-folio-recursive-classifier（本体逐层下探） | `jpp run folio.jpp --fixtures fixture.json` | `--backend live --calib <form-topic 线目录>` |
| `entity-align/` | TypeSafe cookbook entity alignment（候选对匹配） | `jpp run align.jpp --fixtures fixture.json` | `--backend live` |

在各自目录下运行（`winnow.jpp` 读 `input.json`）。`make_fixture.py` 生成夹具，读数为合成值，只检验构造。
Run from each directory. Fixture readings are synthetic (construction checks only).
