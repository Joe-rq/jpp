// 模拟后端：与真后端同协议的 WebSocket 服务器。连上先收 snapshot，之后是增量，每条一行 JSON。
// 用法：npm run mock:ws -- [--n 500] [--port 8787]；前端打开 ?src=ws://localhost:8787/events
import { WebSocketServer, WebSocket } from 'ws';
import { MockNetwork } from '../src/mock/generator';

const arg = (k: string, d: number) => { const i = process.argv.indexOf(k); return i >= 0 ? Number(process.argv[i + 1]) : d; };
const n = arg('--n', 500), port = arg('--port', 8787);
const net = new MockNetwork({ n });
const wss = new WebSocketServer({ port, path: '/events' });
wss.on('connection', (ws) => { ws.send(JSON.stringify(net.snapshot())); });
let last = performance.now();
setInterval(() => {
  const now = performance.now(); const dt = (now - last) / 1000; last = now;
  const evs = net.step(dt);
  if (!evs.length) return;
  const payload = evs.map((e) => JSON.stringify(e)).join('\n');
  for (const c of wss.clients) if (c.readyState === WebSocket.OPEN) c.send(payload);
}, 50);
console.log(`模拟事件源：ws://localhost:${port}/events（n=${n}）`);
