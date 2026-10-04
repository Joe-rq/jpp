#!/bin/zsh
# （重）开 Cloudflare 快速隧道，把新地址写进 Worker 再部署。只动隧道，不碰服务（接入者的状态不受影响）。
set -e
cd "${0:A:h}/.."
mkdir -p runs/real
pkill -f 'cloudflared tunnel --no-autoupdate --url http://localhost:8794' || true
sleep 1
: > runs/real/tunnel.log
nohup cloudflared tunnel --no-autoupdate --url http://localhost:8794 --http-host-header localhost:8794 > runs/real/tunnel.log 2>&1 &
until grep -qoE 'https://[a-z0-9-]+\.trycloudflare\.com' runs/real/tunnel.log; do sleep 2; done
ORIGIN=$(grep -oE 'https://[a-z0-9-]+\.trycloudflare\.com' runs/real/tunnel.log | head -1)
sed -i '' "s|^ORIGIN = .*|ORIGIN = \"$ORIGIN\"|" deploy/worker/wrangler.toml
(cd deploy/worker && npx --no-install wrangler deploy >/dev/null)
echo "隧道 $ORIGIN → https://net.towow.ai"
