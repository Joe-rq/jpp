# Maintenance practice / 维护约定

Keep each update focused on one verifiable, useful outcome, consistent with [Contributing](../CONTRIBUTING.md).
每次更新围绕一项可验证的实际进展，遵循[贡献指南](../CONTRIBUTING.md)。

## Sync completed progress / 同步已完成的进展

After each verifiable advance, sync the corresponding code, necessary tests and documentation to GitHub as one coherent update.
每完成一项可验证的实际进展，就把相应代码、必要测试和文档作为完整更新同步到 GitHub。

Check the affected behavior and documented commands before publishing. Follow the existing installation, test and CI workflow; do not introduce new automation for this practice.
发布前检查受影响的行为和文档命令，沿用现有安装、测试与 CI 流程；本约定不引入新自动化。

Keep research, local verification, GitHub publication and installable releases distinct. Work that has not passed verification must not be described as released.
明确区分在研、已本地验证、已同步 GitHub 与已发行的可安装版本；验证未过的在研代码不能写成已发布。

## Commits and progress notes / 提交与进度记录

Write commit messages in both Chinese and English, stating the actual change and its practical effect. Keep each commit focused on one outcome.
Commit 信息使用中英双语，清楚说明实际改变及其效果；每个提交聚焦一个结果。

Example / 示例：

```text
Add offline replay so the demo runs without API access
增加离线回放，使演示无需访问 API 即可运行
```

For each completed advance, add a dated entry to [progress.md](progress.md) with:
每项已完成的进展都在 [progress.md](progress.md) 中按日期记录：

- Change and effect / 改变及效果。
- Usage command or example / 使用命令或示例。
- Verification and its limits / 验证结果及其适用范围。
- Next step or open question / 下一步或待解决的问题。
- Links to the relevant commits / 相关 commit 链接。

Use a follow-up documentation commit when a commit link is only available after committing. Historical test results do not establish verification for later changes.
提交完成后才能取得链接时，用后续文档提交补齐；历史测试结果不能证明后续改动已经通过验证。

## Shared work and releases / 协作与版本发行

When sharing a directory, stage and commit only your own changes. Before pushing, coordinate which commits are ready to publish, since a push may include others' commits.
多人共用目录时，只暂存和提交自己的改动。推送可能包含他人的提交，推送前须协调并确认本次需要发布的提交。

Reserve releases for important installable versions, using the existing release process. Nature makes release decisions as stated in the contribution guide.
重要的可安装版本再做 release，沿用现有发行流程；按照贡献指南，由 Nature 决定版本发行。
