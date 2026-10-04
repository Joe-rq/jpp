#!/bin/zsh
# 重新部署转发 Worker（首页、说明、前端改了时）：先构建前端、拷进 deploy/worker/public，用本机记下的当前隧道地址，不换隧道。
set -e
cd "${0:A:h}/.."
ORIGIN=$(cat runs/real/origin.txt)
(cd web && npm run build >/dev/null)
mkdir -p deploy/worker/public
rsync -a --delete web/dist/ deploy/worker/public/
(cd deploy/worker && npx --no-install wrangler deploy --var "ORIGIN:$ORIGIN" >/dev/null)
echo "Worker 已部署，转发到 $ORIGIN"
