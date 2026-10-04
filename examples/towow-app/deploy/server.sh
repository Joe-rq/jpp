#!/bin/zsh
# （重）启本机通爻服务，公网模式。真实接入存在 runs/real/joins.json（只有 t0 包与 token 摘要），重启后自动恢复。
# 背景人口预载走判断缓存、彼此不判断，重启不花钱。展示 token 在 ~/.towow/display-token（0600）。
set -e
cd "${0:A:h}/.."
mkdir -p ~/.towow runs/real
# 重启期间挂一个标记，watch.sh 看到就不插手（否则两边同时重启，互相 pkill）
mkdir runs/real/.restarting 2>/dev/null || { echo "已有一次重启在进行"; exit 0; }
trap 'rmdir runs/real/.restarting 2>/dev/null' EXIT
[ -f ~/.towow/display-token ] || { python3 -c "import secrets;print(secrets.token_urlsafe(24))" > ~/.towow/display-token; chmod 600 ~/.towow/display-token; }
pkill -f 'towow serve' || true
sleep 1
TOWOW_DISPLAY_TOKEN=$(cat ~/.towow/display-token) OMP_NUM_THREADS=2 nohup .venv/bin/towow serve --judge live \
  --judge-cache runs/judge-cache.sqlite --preload world/packs --preload-n 500 --preload-background \
  --port 8794 --public --max-real-agents 200 > runs/real/public-serve.log 2>&1 &
until grep -qE 'preload 500/500|Traceback' runs/real/public-serve.log; do sleep 2; done
grep -q Traceback runs/real/public-serve.log && { tail -20 runs/real/public-serve.log; exit 1; }
echo "服务已起：http://localhost:8794"
