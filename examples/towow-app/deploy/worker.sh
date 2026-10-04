#!/bin/zsh
# 重新部署转发 Worker：用本机记下的当前隧道地址，不换隧道。
# 平时（改了首页、前端或说明）：先构建前端、拷进 deploy/worker/public 再部署。
# --no-build：直接用已有的 deploy/worker/public（tunnel.sh 无人值守时这样调，工作区里改了一半的前端不会挡住换隧道地址）。
set -e
cd "$(dirname "$0")/.."
ORIGIN=$(cat runs/real/origin.txt)
if [[ "$1" != "--no-build" || ! -f deploy/worker/public/index.html ]]; then
  (cd web && npm run build >/dev/null)
  mkdir -p deploy/worker/public
  rsync -a --delete web/dist/ deploy/worker/public/
fi
(cd deploy/worker && npx --no-install wrangler deploy --var "ORIGIN:$ORIGIN" >/dev/null)
echo "Worker 已部署，转发到 $ORIGIN"
