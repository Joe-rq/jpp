#!/bin/zsh
# （重）开 Cloudflare 快速隧道，把新地址写进 Worker 再部署。只动隧道，不碰服务（接入者的状态不受影响）。
set -e
cd "$(dirname "$0")/.."
mkdir -p runs/real
pkill -f 'cloudflared tunnel --no-autoupdate --url http://localhost:8794' || true
sleep 1
: > runs/real/tunnel.log
nohup cloudflared tunnel --no-autoupdate --url http://localhost:8794 --http-host-header localhost:8794 > runs/real/tunnel.log 2>&1 &
until grep -qoE 'https://[a-z0-9-]+\.trycloudflare\.com' runs/real/tunnel.log; do sleep 2; done
ORIGIN=$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' runs/real/tunnel.log | head -1)
echo "$ORIGIN" > runs/real/origin.txt          # 当前隧道地址只记在本机，不进仓库
deploy/worker.sh --no-build                  # 只换转发地址，用已构建好的前端
echo "隧道 $ORIGIN → https://net.towow.ai"
