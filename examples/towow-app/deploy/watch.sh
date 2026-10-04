#!/bin/zsh
# 看护：每分钟查一次。本机服务不应答就重启服务（真实接入从存盘恢复）；
# 服务正常但公网连续两次不通，就只重开隧道（快速隧道地址会变，Worker 跟着改）。
D="${0:A:h}"
LOG="$D/../runs/real/watch.log"
miss=0
while true; do
  L="$D/../runs/real/.restarting"
  [ -d "$L" ] && [ -n "$(find "$L" -maxdepth 0 -mmin +10)" ] && rmdir "$L"   # 超过 10 分钟的标记是重启中途被杀留下的
  if [ -d "$L" ]; then
    :   # 有人正在手动重启
  elif ! curl -fsS -m 10 http://localhost:8794/healthz >/dev/null 2>&1; then
    echo "$(date '+%F %T') 本机服务不应答，重启" >> "$LOG"
    "$D/server.sh" >> "$LOG" 2>&1 || echo "$(date '+%F %T') 服务重启失败" >> "$LOG"
    miss=0
  elif ! curl -fsS -m 15 https://net.towow.ai/healthz >/dev/null 2>&1; then
    miss=$((miss + 1))
    if [ $miss -ge 2 ]; then
      echo "$(date '+%F %T') 公网不通，重开隧道" >> "$LOG"
      "$D/tunnel.sh" >> "$LOG" 2>&1 || echo "$(date '+%F %T') 隧道重开失败" >> "$LOG"
      miss=0
    fi
  else
    miss=0
  fi
  sleep 60
done
