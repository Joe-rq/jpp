// 事件源：不给 ?src 时先试真后端（宿主托管页面时的同源 /events，再 ws://localhost:8794/events），
// 连不上就放打包进来的回放 replay.jsonl。模拟源只在 ?src=mock 时用。
// ?src=mock / ?src=ws://host/events / ?src=replay&file=xxx.jsonl&speed=4 显式指定。
// 几种源接口一致：都把事件按服务器时间 t 排进本地时间轴，到点放出。
import type { NetEvent, Meta } from './protocol';
import { MockNetwork } from './mock/generator';

export type SrcKind = 'live' | 'mock' | 'replay' | 'probing';
export const DEFAULT_PORT = 8794;
export interface Source {
  pump(now: number, emit: (e: NetEvent) => void): void;
  label: string; kind: SrcKind;
  /** 画面时间相对事件时间的倍速：实时与模拟是 1，回放默认 4。给人看的时长乘它、速率除以它。 */
  speed: number;
  /** 数据出处，写在左下角最前面：实时 / 回放（真机数据）/ 回放（离线伪读数）/ 回放 / 模拟数据 */
  origin?: string;
  /** 回放首行 meta（带 me 时前端以它为中心） */
  meta?: Meta;
  /** 真后端的 HTTP 根（'' 表示同源），信息层从这里拉 /api/opportunities */
  api?: string;
  /** 实时源的连接状态：断线重连时为 false */
  connected?: boolean;
  close(): void;
}

class Timeline {
  q: NetEvent[] = []; head = 0; offset: number | null = null; speed = 1; base = 0;
  push(e: NetEvent, now: number, lead = 0) {
    if (this.offset === null) { this.offset = e.t; this.base = now + lead; }
    const q = this.q;
    if (q.length > this.head && q[q.length - 1].t > e.t) { // 少见：乱序，插入到位
      let i = q.length; while (i > this.head && q[i - 1].t > e.t) i--; q.splice(i, 0, e);
    } else q.push(e);
  }
  due(e: NetEvent) { return this.base + (e.t - (this.offset ?? e.t)) / this.speed; }
  release(now: number, emit: (e: NetEvent) => void) {
    const q = this.q;
    while (this.head < q.length && (q[this.head].type === 'snapshot' || this.due(q[this.head]) <= now)) emit(q[this.head++]);
    if (this.head > 4096) { this.q = q.slice(this.head); this.head = 0; }
  }
  reset() { this.q = []; this.head = 0; this.offset = null; }
}

export function mockSource(n: number, seed: number): Source {
  const net = new MockNetwork({ n, seed });
  const tl = new Timeline();
  let simEnd = -1;
  return {
    label: `mock n=${n}`, kind: 'mock', speed: 1, origin: '模拟数据',
    pump(now, emit) {
      if (simEnd < 0) { tl.push(net.snapshot(), now); simEnd = now; }
      // 模拟器跑在本地时间前面 0.2 秒
      const target = now + 0.2;
      if (target > simEnd) {
        const dt = Math.min(0.5, target - simEnd);
        for (const e of net.step(dt)) tl.push(e, now);
        simEnd = target;
      }
      tl.release(now, emit);
    },
    close() {},
  };
}

/** ws://host/events → http://host；与页面同源时返回 ''（走同源或 vite 代理）。 */
export function apiOf(url: string): string {
  try {
    const u = new URL(url, location.href);
    if (u.host === location.host) return '';
    return `${u.protocol === 'wss:' ? 'https:' : 'http:'}//${u.host}`;
  } catch { return ''; }
}

export function wsSource(url: string, first?: { ws: WebSocket; lines: string[] }): Source {
  const tl = new Timeline();
  let ws: WebSocket | null = null; let closed = false; let clock = 0;
  const src: Source = {
    label: url, kind: 'live', api: apiOf(url), connected: false, speed: 1, origin: '实时',
    pump(now, emit) { clock = now; tl.release(now, emit); },
    close() { closed = true; ws?.close(); },
  };
  const feed = (data: string) => {
    for (const line of data.split('\n')) {
      if (!line.trim()) continue;
      try {
        const e = JSON.parse(line) as NetEvent;
        if (e.type === 'snapshot') tl.reset();
        tl.push(e, clock, 0.25); // 留 0.25s 抖动缓冲
      } catch { /* 坏行跳过 */ }
    }
  };
  const attach = (s: WebSocket) => {
    ws = s; src.connected = true;
    s.onmessage = (m) => feed(String(m.data));
    s.onclose = () => { src.connected = false; if (!closed) setTimeout(connect, 2000); };
  };
  const connect = () => {
    const s = new WebSocket(url);
    s.onopen = () => attach(s);
    s.onclose = () => { if (!closed) setTimeout(connect, 2000); };
  };
  if (first) { attach(first.ws); for (const l of first.lines) feed(l); } else connect();
  return src;
}

/** 试连一个地址：第一条消息必须是 snapshot，才算连上了宿主。 */
function tryWs(url: string, timeoutMs: number): Promise<{ ws: WebSocket; lines: string[] } | null> {
  return new Promise((resolve) => {
    let done = false; let ws: WebSocket | undefined;
    const fail = () => { if (done) return; done = true; try { ws?.close(); } catch { /* 忽略 */ } resolve(null); };
    try { ws = new WebSocket(url); } catch { resolve(null); return; }
    const sock = ws;
    let timer = setTimeout(fail, timeoutMs);
    // 握手成功说明那里确实有 /events；宿主忙时快照可能要十几秒才发出，握手后放宽到 30 秒
    sock.onopen = () => { clearTimeout(timer); timer = setTimeout(fail, 30000); };
    sock.onerror = () => { clearTimeout(timer); fail(); };
    sock.onclose = () => { clearTimeout(timer); fail(); };
    sock.onmessage = (m) => {
      if (done) return;
      const txt = String(m.data);
      let ok = false;
      try { ok = JSON.parse(txt.split('\n')[0]).type === 'snapshot'; } catch { ok = false; }
      clearTimeout(timer);
      if (!ok) { fail(); return; }
      done = true; sock.onopen = null; sock.onerror = null; sock.onclose = null; sock.onmessage = null;
      resolve({ ws: sock, lines: [txt] });
    };
  });
}

/** 没给 ?src 时：页面由宿主托管（或 vite 代理）就先试同源 /events，再试 ws://localhost:8794/events；
 *  都不通就放打包的回放。放在 GitHub Pages 上时握手几秒内就失败，访问者很快看到回放。 */
export function autoSource(replayFile: string, speed: number, origin?: string): Source {
  const cands: string[] = [];
  const local = `ws://localhost:${DEFAULT_PORT}/events`;
  const hosted = (location.protocol === 'http:' || location.protocol === 'https:') && !/github\.io$/.test(location.hostname);
  if (hosted) cands.push(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/events`);
  // 放在公网静态页（GitHub Pages 等）上时不探测本机：新版 Chrome 会为访问 localhost 弹本地网络权限框，直接放回放
  const publicStatic = /github\.io$/.test(location.hostname);
  if (!publicStatic && !cands.includes(local)) cands.push(local);
  let inner: Source | null = null;
  const src: Source = {
    label: 'probing', kind: 'probing', speed: 1,
    pump(now, emit) {
      if (!inner) return;
      inner.pump(now, emit);
      src.connected = inner.connected; src.origin = inner.origin; src.meta = inner.meta;
    },
    close() { inner?.close(); },
  };
  const adopt = (s: Source) => { inner = s; src.label = s.label; src.kind = s.kind; src.api = s.api; src.connected = s.connected; src.speed = s.speed; src.origin = s.origin; };
  (async () => {
    for (const url of cands) {
      // 拒连、404、非 WebSocket 应答都会立刻失败；握手 3 秒不成就换下一个。握手成功后宿主忙时快照可能要十几秒
      const got = await tryWs(url, 3000);
      if (got) { adopt(wsSource(url, got)); return; }
    }
    adopt(replaySource(replayFile, speed, origin));
  })();
  return src;
}

/** 回放文件的出处：首行 meta 说了算；URL 给了 ?data= 也算；都没有就只写「回放」，不猜。 */
function originOf(meta: Meta | undefined, override?: string): string {
  const s = String(override ?? meta?.source ?? '').toLowerCase();
  if (s === 'jev' || s === 'real') return typeof meta?.me === 'string' ? '回放（真机 JEV 读数，真实接入）' : '回放（真机数据）';
  if (s === 'fixture' || s === 'offline') return '回放（离线伪读数）';
  if (s === 'mock') return '回放（模拟数据）';
  return '回放';
}

export function replaySource(file: string, speed: number, origin?: string): Source {
  const tl = new Timeline(); tl.speed = speed;
  let lines: NetEvent[] | null = null; let i = 0;
  const src: Source = {
    label: `replay ${file}`, kind: 'replay', speed, origin: originOf(undefined, origin),
    pump(now, emit) {
      if (lines) { while (i < lines.length && (tl.offset === null || tl.due(lines[i]) <= now + 1)) tl.push(lines[i++], now); }
      tl.release(now, emit);
    },
    close() {},
  };
  fetch(file).then((r) => { if (!r.ok) throw new Error(String(r.status)); return r.text(); }).then((txt) => {
    const out: NetEvent[] = [];
    for (const l of txt.split('\n')) {
      if (!l.trim()) continue;
      try {
        const e = JSON.parse(l) as NetEvent;
        if (e.type === 'meta') { src.meta = e; src.origin = originOf(e, origin); continue; }
        if (typeof e.t === 'number') out.push(e);
      } catch { /* 跳过 */ }
    }
    lines = out;
  }).catch((err) => { console.error('回放文件读取失败', err); src.origin = '回放文件读取失败'; });
  return src;
}
