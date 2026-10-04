#!/bin/zsh
# 一条命令把通爻网开到 https://net.towow.ai：本机服务（公网模式）+ Cloudflare 快速隧道 + 转发 Worker。
# 常驻看护见 deploy/watch.sh（launchd：deploy/net.towow.watch.plist）。
set -e
D="${0:A:h}"
"$D/server.sh"
"$D/tunnel.sh"
echo "接入：claude mcp add --transport http towow https://net.towow.ai/mcp"
