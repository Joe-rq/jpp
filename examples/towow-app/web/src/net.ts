// 事件 → 画面。前端自己的全部状态都从事件累积而来，不读后端内部。
// 真后端的节律：一次接入引出一阵上千条判断，然后静默二十来秒；构型偶发、成串。
// 画面照实画每一条事件的状态；只在“画多少架航班”这件事上按预算抽样，避免一秒上千架互相盖住。
import { Scene, Vector3 } from 'three';
import type { NetEvent, ConfigEv, Shape, StatsFields, Judge, Probe } from './protocol';
import { SHAPE_NAME, MAX_TIER, formLabel } from './protocol';
import { plural } from './captions';
import { Layout, hash } from './layout';
import { Ports } from './scene/ports';
import { Routes, EDGE_STATE } from './scene/routes';
import { Flights } from './scene/flights';
import { Rings, Blueprints } from './scene/fx';
import { Land } from './scene/land';
import { FLASH, FLY, RING, T } from './style';
import { shared } from './scene/common';

export interface MissRec { cat: string; status: string; from: string; t: number }
export interface NodeRec {
  id: string; idx: number; kind: 'agent' | 'config'; label: string; full: string; host?: string; city?: string; lang?: string;
  tier: number; pos: Vector3; alive: boolean; edges: Set<string>; configs: Set<string>;
  /** 别人向它要、或它向别人要的补信息，按对方 id */
  missing: Map<string, MissRec>;
  /** 它向谁解锁到了哪一层（disclose.to） */
  seen: Map<string, number>;
  joinT: number; configId?: string; real?: boolean;
}
export interface EdgeRec { a: string; b: string; slot: number; conf: number; form: string; state: string; t: number; /** 画面上的线宽读数（?me 的机会收成亮航线时比读数更粗） */ vis?: number }
/** ?me 的 agent 的候选：网络替它召回的人，逐个亮起、判过，暗下去或收成航线 */
export interface CandRec { id: string; rank: number; round: number; unsure: boolean; no: number[]; dropped: boolean; spoke?: number }
export interface ConfigRec {
  id: string; shape: Shape; members: string[]; roles: Record<string, string>; conf: number | null; stage: string;
  nodeId?: string; port?: Vector3; plan?: { title: string | null; summary: string | null; conf: number | null }; planT?: number;
  fleetT?: number; meta?: boolean;
}

/** 一对（或一个构型）上「拿不准 → 要信息 → 披露 → 再判」的一步。全部来自事件，不补不猜。 */
export interface StoryStep {
  k: 'unsure' | 'ask' | 'reply' | 'disclose' | 'rejudge';
  t: number; q?: string; p?: number | null; p0?: number | null; /** 再判后不成立时，「不成立」的把握（gone 事件带的读数；不是成立置信度） */ pNo?: number | null; cat?: string; route?: string;
  holder?: string; asker?: string; granted?: boolean; tier?: number; text?: string | null;
}
interface JRec { q: string; p: number; exit: string; batch: string | number; value?: unknown }
/** 一对 agent（或 agent 与新港、或一个构型）的判断簿：最近的读数、决定性题、补信息的经过 */
export interface Story {
  key: string; a: string; b: string;
  /** 引擎里这一对的 A、B（两两(k) 的 k.a、k.b）：题面里的「A」「B」指它们 */
  ka?: string; kb?: string;
  recent: JRec[];
  /** 决定性题：边事件带 q 时直接用；否则在这一对最近的 open/catcher act 判断里找读数相同的那道 */
  dq?: string; dqText?: string | null; dp?: number | null;
  steps: StoryStep[];
  /** 还缺的信息类别 → 状态（sent/granted/denied/near_boundary/refine） */
  lacks: Map<string, string>;
  /** 披露之后等这一对（或这个构型）的下一次读数：p0 是披露那一刻的置信度，null 表示那时关系还没成立 */
  await?: { p0: number | null };
  lastUnsure?: { q: string; p: number | null };
  /** 画面上这一对的动画排到了哪一刻：同一对的几步按因果顺序依次放 */
  animT: number;
  tierSeen?: string[] | null; hostLacks?: string[] | null;
}

export type Moment =
  | { kind: 'join'; id: string; t: number }
  | { kind: 'firstOpp'; id: string; other: string; form: string; dt: number; t: number }
  | { kind: 'config'; id: string; stage: string; t: number }
  /** 开垦：船队出发，born 时新港点亮 */
  | { kind: 'fleet'; id: string; t: number; born: number; meta: boolean }
  | { kind: 'plan'; id: string; t: number }
  | { kind: 'spotlight'; id: string; why: string; to?: string; t: number }
  /** ?me 的 agent 发出召回：网络替它在人群里找候选，镜头拉开看这一扇 */
  | { kind: 'recall'; id: string; to: string[]; t: number }
  /** 披露之后同一道题再判出了新读数 */
  | { kind: 'rejudge'; key: string; a: string; b: string; p0: number | null; p: number | null; pNo?: number | null; t: number };

/** 模拟 agent 的 label 是整句自我介绍；画面上用第一个分句，信息层再给全文。 */
export function shortLabel(s: string | null | undefined, max = 12): string {
  if (!s) return '';
  // 与后端短称同一条规则：按「，,。；;」切，取第一段，最多 max 字（拉丁文按两倍）
  let cut = s.split(/[，,。；;\n]/)[0].trim() || s;
  // 自我介绍式的开头（「我是蒋文君」「I'm Anurak」）只留名字
  cut = cut.replace(/^(我是|我叫|I am |I'm |Hi, I'm )/i, '').trim() || cut;
  const latin = /^[\x00-\x7f]*$/.test(cut);
  const lim = latin ? max * 2 : max;
  return cut.length > lim ? cut.slice(0, lim - 1) + '…' : cut;
}

export class Net {
  layout: Layout; ports: Ports; routes: Routes; flights: Flights; rings: Rings; blue: Blueprints; land: Land;
  nodes = new Map<string, NodeRec>();
  byIdx: NodeRec[] = [];
  edges = new Map<string, EdgeRec>();
  slotKey: (string | undefined)[] = [];
  configs = new Map<string, ConfigRec>();
  /** 判断簿：键是 'a|b'（按字典序）或构型 id */
  stories = new Map<string, Story>();
  /** disclose_request 去重：宿主每次 reply 单元重发布都会再发一条 */
  private drSeen = new Set<string>();
  /** 画面相对事件时间的倍速（回放 4），由事件源给 */
  speed = 1;
  stats: StatsFields = {};
  onMoment?: (m: Moment) => void;
  onStats?: () => void;
  onChange?: (id: string) => void;
  /** ?me=<id>：事件涉及这个 agent 时回调（信息层刷新它的机会列表） */
  me?: string;
  onMine?: () => void;
  readonly scale: number;
  private tmp = new Vector3(); private tmp2 = new Vector3();
  // 航班预算：每秒最多补 flightRate 架，攒满 flightCap 为止；超出的判断只闪港灯
  private flightTokens = 400; private flightRate = 400; private flightCap = 600;
  // 开垦船队：每 2.5 秒攒一次，最多攒 3 次；没有额度时新港直接点亮，不派船
  private fleetTokens = 3;
  private lastSweep = -1e9;
  private lastT = 0;
  // 每秒判断数：60 个一秒桶
  private jb = new Float64Array(60); private jbSec = new Int32Array(60).fill(-1);
  private later: { t: number; fn: () => void }[] = [];
  /** ?me 的候选（召回 → 判过 → 暗下或成线） */
  cands = new Map<string, CandRec>();
  /** 画面叙述：由前端按事件自己数出来的一句话，交给字幕 */
  onNarrate?: (text: string) => void;
  /** 旁白语言：首页 ?lang=en 时为 'en'，默认 'zh' */
  lang: 'zh' | 'en' = 'zh';
  /** 一次召回已到、正排队等镜头与光落地：接入字幕让位给叙述 */
  storyPending = false;
  private pend: { due: number; seq: number; e: NetEvent; lag: number }[] = [];
  /** 正在处理的排队事件被推迟了多少秒（算「第一个机会花了多久」时要减掉，那是事件时间，不含画面上的拆分） */
  private curLag = 0;
  private seq = 0; private burstAt = -1e9; private rankOf = new Map<string, number>();
  private recentRings: { x: number; z: number; t: number; R: number }[] = [];
  private recalls = 0;
  private glimpseBatch = { n: 0, lo: 1, hi: 0, armed: false };

  constructor(scene: Scene, nExpected: number) {
    this.layout = new Layout(nExpected);
    this.scale = this.layout.islandR / 62;
    const cap = Math.ceil(nExpected * 1.3) + 1200;
    this.land = new Land(scene, this.layout.D);
    this.layout.onIsland = (isl) => this.land.addIsland(isl);
    const per = nExpected > 3000 ? 12 : 60; const lr = this.layout.islandR * 0.6;
    this.layout.onLand = (p) => this.land.addAround(p, per, lr);
    this.routes = new Routes(scene, Math.max(6000, nExpected * 4), 140 * this.scale);
    this.ports = new Ports(scene, cap);
    this.flights = new Flights(scene, nExpected > 3000 ? 12000 : 5000, Math.min(1, Math.max(0.3, Math.sqrt(500 / nExpected))));
    this.rings = new Rings(scene, 512);
    this.blue = new Blueprints(scene, nExpected > 3000 ? 40 : 60);
  }

  nodePos(id: string): Vector3 | undefined { return this.nodes.get(id)?.pos; }
  configPos(id: string): Vector3 | undefined {
    const c = this.configs.get(id); if (!c) return undefined;
    if (c.port) return c.port;
    return this.centroid(c.members);
  }
  centroid(ids: string[]): Vector3 | undefined {
    const v = new Vector3(); let k = 0;
    for (const m of ids) { const p = this.nodes.get(m)?.pos; if (p) { v.add(p); k++; } }
    return k ? v.multiplyScalar(1 / k) : undefined;
  }
  label(id: string): string { const n = this.nodes.get(id); if (n) return n.label; const c = this.configs.get(id); return c ? this.configLabel(c) : id; }
  configLabel(c: ConfigRec): string {
    const names = c.members.map((m) => this.nodes.get(m)?.kind === 'config' ? '一个构型' : this.nodes.get(m)?.label ?? m);
    const shown = names.length > 3 ? `${names.slice(0, 2).join('、')}等 ${names.length} 方` : names.join('、');
    return `${shown}的${SHAPE_NAME[c.shape] ?? '构型'}`;
  }

  // ---------- 统计 ----------
  agentCount() { let k = 0; for (const n of this.nodes.values()) if (n.kind === 'agent' && n.alive) k++; return k; }
  configCount() { let k = 0; for (const c of this.configs.values()) if (c.stage !== 'dissolved') k++; return k; }
  /** 事件时间里最近 60 秒的判断条数：回放按倍速放，画面上的 60/speed 秒对应事件里的一分钟 */
  judgesLastMinute(now: number) {
    const s = Math.floor(now); const win = Math.max(1, Math.round(60 / this.speed)); let sum = 0;
    for (let i = 0; i < 60; i++) if (this.jbSec[i] > s - win) sum += this.jb[i];
    return win * this.speed >= 60 ? sum : sum * 60 / (win * this.speed);
  }
  private countJudge(now: number) {
    const s = Math.floor(now); const i = s % 60;
    if (this.jbSec[i] !== s) { this.jbSec[i] = s; this.jb[i] = 0; }
    this.jb[i]++;
  }

  /** 光圈：同一处 1.6 秒内不叠第二圈（几个披露同时落在一簇港上时不互相盖） */
  private ring(pos: Vector3, t: number, dur: number, R: number, kind: number) {
    const rr = this.recentRings;
    for (const r of rr) if (Math.abs(r.t - t) < 1.6 && Math.hypot(r.x - pos.x, r.z - pos.z) < (R + r.R) * 0.9) return;
    rr.push({ x: pos.x, z: pos.z, t, R }); if (rr.length > 24) rr.shift();
    this.rings.spawn(pos, t, dur, R, kind);
  }
  private launch(from: Vector3, to: Vector3, t: number, dur: number, type: number, arc: number, must = false) {
    if (!must) { if (this.flightTokens < 1) return; this.flightTokens--; }
    this.flights.launch(from, to, t, dur, type, arc);
  }

  // ---------- ?me 的编排：召回的几十个候选在同一毫秒里被判完，画面要把它拆开讲 ----------
  /** 事件入口。非 ?me 时原样处理；?me 且网络替它召回时，牵涉它的事件按候选顺序依次放，让光先落地、探针一根根射出、再一个个判。 */
  ingest(e: NetEvent, now: number) {
    const me = this.me;
    if (!me) { this.handle(e, now); return; }
    if (e.type === 'probe' && e.from === me) {
      this.burstAt = now; this.rankOf.clear(); e.to.forEach((id, i) => this.rankOf.set(id, i));
      this.storyPending = true;
      this.queue(e, now + 2.4, 2.4); return;
    }
    const c = this.candOf(e);
    if (c && now - this.burstAt < 45) { { const lag = 5.0 + 0.075 * (this.rankOf.get(c) ?? 0); this.queue(e, now + lag, lag); return; } }
    this.handle(e, now);
  }
  private queue(e: NetEvent, due: number, lag: number) {
    const p = { due, seq: this.seq++, e, lag }; const q = this.pend;
    let i = q.length; while (i > 0 && q[i - 1].due > due) i--;
    q.splice(i, 0, p);
  }
  /** 每帧：放出到点的排队事件 */
  flush(now: number) {
    while (this.pend.length && this.pend[0].due <= now) { const p = this.pend.shift()!; this.curLag = p.lag; this.handle(p.e, now); this.curLag = 0; }
  }
  private candOf(e: NetEvent): string | undefined {
    const me = this.me!; let x: string | null | undefined;
    switch (e.type) {
      case 'judge': case 'unsure_route': case 'edge': x = e.a === me ? e.b : e.b === me ? e.a : undefined; break;
      case 'disclose_request': x = e.to === me ? e.from : e.from === me ? e.to : undefined; break;
      case 'disclose': x = e.id === me ? e.to : e.to === me ? e.id : undefined; break;
      case 'spotlight': x = e.id === me ? e.to : e.to === me ? e.id : undefined; break;
    }
    return x ?? undefined;
  }
  private narrate(text: string, at: number) { this.later.push({ t: at, fn: () => this.onNarrate?.(text) }); }
  /** 召回：探针一根根从 ?me 射向候选，候选逐个亮起，海面上各留一道淡航线（之后按判断暗下去或收成亮航线） */
  private recall(A: NodeRec, e: Probe, now: number) {
    const ids: string[] = [];
    e.to.forEach((id, i) => {
      const B = this.nodes.get(id); if (!B) return;
      ids.push(id);
      const t = now + i * 0.075;
      let c = this.cands.get(id); if (!c) { c = { id, rank: i, round: 0, unsure: false, no: [], dropped: false }; this.cands.set(id, c); }
      c.round++; c.rank = i; c.dropped = false;
      const d = A.pos.distanceTo(B.pos);
      const dur = Math.min(1.5, 0.8 + d / (this.layout.D * 1.6));
      this.flights.launch(A.pos, B.pos, t, dur, FLY.probe, d * 0.08 + 4);
      if (c.spoke === undefined) { c.spoke = this.allocSlot(); }
      this.routes.set(c.spoke, A.pos, B.pos, 0.4, EDGE_STATE.up, t + 0.1, hash(this.pairKey(A.id, id)));
      this.ports.flashAt(B.idx, FLASH.member, t + dur * 0.85);
      this.ports.cand(B.idx, t + dur * 0.85);
    });
    this.storyPending = false;
    this.onMoment?.({ kind: 'recall', id: A.id, to: ids, t: now });
    const others = Math.max(0, this.agentCount() - 1);
    const first = this.recalls++ === 0;
    this.narrate(this.lang === 'en'
      ? (first ? `${A.label} joined. The network recalled ${plural(ids.length, 'candidate')} among ${plural(others, 'agent')}` : `The network recalled ${plural(ids.length, 'more candidate')} for ${A.label}`)
      : (first ? `${A.label}接入。网络在 ${others} 个 agent 里召回 ${ids.length} 个候选` : `网络又为 ${A.label}召回 ${ids.length} 个候选`), now + 0.3);
    this.later.push({ t: now + 5.6, fn: () => {
      let dropped = 0, asking = 0;
      for (const id of ids) { const c = this.cands.get(id); if (!c) continue; if (c.dropped) dropped++; else if (c.unsure) asking++; }
      this.onNarrate?.(this.lang === 'en' ? `${dropped} judged no match and dimmed; ${asking} unsure, asking the other side for information` : `${dropped} 个判「不成立」，暗了下去；${asking} 个拿不准，去向对方要信息`);
    } });
  }
  private allocSlot(): number {
    const slot = this.routes.alloc();
    const pk = this.slotKey[slot]; if (pk) { this.dropEdge(pk); this.slotKey[slot] = undefined; }
    return slot;
  }
  /** 一个候选被判「不成立」：港灯暗下去，淡航线退成灰痕 */
  private dropCand(c: CandRec, now: number) {
    if (c.dropped) return; c.dropped = true;
    const n = this.nodes.get(c.id), M = this.me ? this.nodes.get(this.me) : undefined;
    if (n) { this.ports.flashAt(n.idx, FLASH.ignore, now); this.ports.dim(n.idx, now + 0.15); this.rings.spawn(n.pos, now + 0.15, 1.1, 8 * this.scale, RING.leave); }
    if (c.spoke !== undefined && n && M) { this.routes.set(c.spoke, M.pos, n.pos, 0.5, EDGE_STATE.gone, now, hash(this.pairKey(M.id, c.id))); c.spoke = undefined; }
  }
  private markUnsure(other: string | null | undefined) {
    const c = other ? this.cands.get(other) : undefined; if (c) c.unsure = true;
  }
  /** 「看过、放下」：没出现过 new 的 gone。亮一下再熄，读数是「不成立」的把握（取这个候选两个方向的开放题里较弱的那一个）。 */
  private glimpse(a: string, b: string, now: number) {
    const A = this.nodes.get(a), B = this.nodes.get(b); if (!A || !B) return;
    const me = this.me; const mine = !!me && (a === me || b === me);
    const other = mine ? (a === me ? b : a) : '';
    const c = mine ? this.cands.get(other) : undefined;
    const from = mine && me === b ? B : A, to = mine && me === b ? A : B;
    let slot = c?.spoke; if (slot === undefined) slot = this.allocSlot();
    const seed = hash(this.pairKey(a, b));
    this.routes.set(slot, from.pos, to.pos, 0.8, EDGE_STATE.new, now, seed);
    this.routes.highlight(slot, now);
    this.ports.flashAt(A.idx, FLASH.ignore, now); this.ports.flashAt(B.idx, FLASH.ignore, now);
    if (c) {
      c.spoke = undefined; c.dropped = true;
      this.ports.dim(to.idx, now + 0.9);
      const pNo = c.no.length ? Math.min(...c.no) : null;
      const g = this.glimpseBatch; g.n++;
      if (pNo !== null) { g.lo = Math.min(g.lo, pNo); g.hi = Math.max(g.hi, pNo); }
      if (!g.armed) {
        g.armed = true;
        this.later.push({ t: now + 1.4, fn: () => {
          const rng = g.hi > 0 ? `（判「不成立」的把握 ${g.lo.toFixed(2)}${g.hi - g.lo > 0.004 ? '–' + g.hi.toFixed(2) : ''}）` : '';
          if (this.lang === 'en') {
            const r = g.hi > 0 ? ` (no-match confidence ${g.lo.toFixed(2)}${g.hi - g.lo > 0.004 ? ' to ' + g.hi.toFixed(2) : ''})` : '';
            this.onNarrate?.(`${plural(g.n, 'candidate')} reviewed and set aside${r}`);
          } else this.onNarrate?.(`${g.n} 个看过的候选放下了${rng}`);
          g.n = 0; g.lo = 1; g.hi = 0; g.armed = false;
        } });
      }
    }
    this.later.push({ t: now + 1.0, fn: () => { this.routes.set(slot!, from.pos, to.pos, 0.5, EDGE_STATE.gone, now + 1.0, seed); } });
  }
  /** ?me 的候选概况，给侧栏的一行字 */
  candStats() {
    let dropped = 0, asking = 0, held = 0;
    for (const c of this.cands.values()) {
      const e = this.me ? this.edges.get(this.pairKey(this.me, c.id)) : undefined;
      if (e && e.state !== 'gone') held++; else if (c.dropped) dropped++; else if (c.unsure) asking++;
    }
    return { total: this.cands.size, dropped, asking, held };
  }

  handle(e: NetEvent, now: number) {
    switch (e.type) {
      case 'snapshot': this.snapshot(e, now); break;
      case 'node_join': this.addNode(e, now, false); break;
      case 'node_leave': {
        const n = this.nodes.get(e.id); if (!n) break;
        n.alive = false; this.ports.leave(n.idx, now);
        if (n.kind === 'config' && n.configId) this.blue.dissolve(n.configId, now);
        this.ring(n.pos, now, 1.2, 10 * this.scale, RING.leave);
        for (const k of n.edges) { const ed = this.edges.get(k); if (ed && ed.state !== 'gone') this.edge(ed.a, ed.b, ed.form, null, 'gone', now); }
        break;
      }
      case 'disclose': {
        const n = this.nodes.get(e.id); if (!n) break;
        const tier = Math.max(0, Math.min(MAX_TIER, e.tier | 0));
        let at = now;
        if (e.to) {
          n.seen.set(e.to, Math.max(n.seen.get(e.to) ?? 0, tier));
          const st = this.story(e.id, e.to);
          st.steps.push({ k: 'disclose', t: now, holder: e.id, asker: e.to, tier });
          // 披露之后等这一对的下一次读数（边事件）；两人同在的构型也等它的下一次整体读数
          const ed = this.edges.get(this.pairKey(e.id, e.to));
          st.await = { p0: ed && ed.state !== 'gone' ? ed.conf : null };
          for (const cid of n.configs) {
            const c = this.configs.get(cid);
            if (!c || !c.members.includes(e.to)) continue;
            const cs = this.story(c.members[0], c.members[1] ?? c.members[0], cid);
            cs.steps.push({ k: 'disclose', t: now, holder: e.id, asker: e.to, tier }); cs.await = { p0: c.conf }; this.trim(cs);
          }
          this.trim(st);
          at = this.animAt(st, now, 0.8);
          this.onChange?.(e.to);
        }
        // 港灯层级只升不降：world_update 带来的 tier 0 不把已经解锁过的港调暗
        if (tier > n.tier) { n.tier = tier; this.ports.setTier(n.idx, tier); }
        this.ports.flashAt(n.idx, FLASH.disclose, at);
        this.ring(n.pos, at, 0.8, 7 * this.scale, RING.join);
        this.onChange?.(n.id);
        break;
      }
      case 'probe': {
        const A = this.nodes.get(e.from); if (!A) break;
        if (this.me && e.from === this.me) { this.recall(A, e, now); break; }
        for (const id of e.to) {
          const B = this.nodes.get(id); if (!B) continue;
          const d = A.pos.distanceTo(B.pos);
          const dur = Math.min(T.flightMax, T.flightMin + d / (this.layout.D * 1.4));
          this.launch(A.pos, B.pos, now, dur, FLY.probe, d * T.arcK + 6);
        }
        break;
      }
      case 'judge': {
        this.countJudge(now);
        this.noteJudge(e, now);
        const A = this.nodes.get(e.a);
        const B = e.b ? this.nodes.get(e.b) : e.config ? this.configNode(e.config) : undefined;
        const type = e.exit === 'act' ? FLASH.act : e.exit === 'unsure' ? FLASH.unsure : FLASH.ignore;
        if (A) this.ports.flashAt(A.idx, type, now);
        if (B) this.ports.flashAt(B.idx, e.exit === 'unsure' ? FLASH.ignore : type, now);
        if (A && B && e.exit === 'act') {
          const d = A.pos.distanceTo(B.pos);
          this.launch(B.pos, A.pos, now, 0.55 + d / (this.layout.D * 3), FLY.act, d * 0.12 + 3);
        }
        // ?me 的候选：开放题判「不成立」且没有在向对方要信息 → 暗下去
        if (this.me && e.q === 'open' && e.exit === 'ignore' && (e.a === this.me || e.b === this.me)) {
          const c = this.cands.get(e.a === this.me ? (e.b ?? '') : e.a);
          if (c) { c.no.push(e.p); if (!c.unsure) this.dropCand(c, now); }
        }
        break;
      }
      case 'unsure_route': {
        if (this.me && e.route !== 'return' && e.route !== 'drop') this.markUnsure(e.a === this.me ? e.b : e.b === this.me ? e.a : null);
        const asks = Array.isArray(e.ask_to) ? e.ask_to : [e.ask_to];
        const st = this.storyFor(e.a, e.b ?? null, e.config ?? null, e.q);
        if (st) {
          const q = e.q ?? '';
          const p = typeof e.p === 'number' ? e.p : null;   // 拿不准那道题的读数要后端给（NEEDS-BACKEND）；同批里同名题有两个方向，前端不猜
          const last = st.steps[st.steps.length - 1];
          const dup = last && last.k === 'unsure' && last.q === q && last.cat === e.missing && last.route === e.route;
          if (!dup) {
            st.steps.push({ k: 'unsure', t: now, q, p, cat: e.missing, route: e.route });
            st.lastUnsure = { q, p };
            const prev = st.lacks.get(e.missing);
            if (prev !== 'granted') st.lacks.set(e.missing, e.route === 'disclose_request' ? (prev === 'denied' ? 'denied' : 'sent') : e.route);
            this.trim(st);
            const at = this.animAt(st, now, 0.6);
            const A0 = this.nodes.get(e.a); if (A0) this.ports.flashAt(A0.idx, FLASH.unsure, at);
            this.onChange?.(e.a); if (e.b) this.onChange?.(e.b);
          }
        }
        for (const h of asks) {
          const holder = this.nodes.get(h); if (!holder) continue;
          const other = h === e.a ? e.b : e.a; if (!other) continue;
          const status = e.route === 'disclose_request' ? 'sent' : e.route;
          const prev = holder.missing.get(other);
          if (!prev || prev.status === 'sent' || status === 'sent') holder.missing.set(other, { cat: e.missing, status: prev && prev.status !== 'sent' && status === 'sent' ? prev.status : status, from: other, t: now });
        }
        const A = this.nodes.get(e.a);
        if (A && e.route === 'drop') this.ports.flashAt(A.idx, FLASH.ignore, now);
        break;
      }
      case 'disclose_request': {
        // 宿主每次 reply 重发布都会再发一条同样的结果：只认每个 (持有方, 请求方, 类别, 状态) 的第一条
        if (this.me && e.status === 'sent') this.markUnsure(e.to === this.me ? e.from : e.from === this.me ? e.to : null);
        const dk = `${e.to}|${e.from}|${e.category}|${e.status}`;
        if (this.drSeen.has(dk)) break;
        this.drSeen.add(dk);
        const to = this.nodes.get(e.to), from = this.nodes.get(e.from);
        if (to) { to.missing.set(e.from, { cat: e.category, status: e.status, from: e.from, t: now }); this.onChange?.(to.id); }
        const st = this.story(e.to, e.from);
        if (e.status === 'sent') st.steps.push({ k: 'ask', t: now, holder: e.to, asker: e.from, cat: e.category, text: e.purpose });
        else st.steps.push({ k: 'reply', t: now, holder: e.to, asker: e.from, cat: e.category, granted: e.status === 'granted' });
        if (e.status !== 'sent' || st.lacks.get(e.category) !== 'granted') st.lacks.set(e.category, e.status);
        this.trim(st);
        this.onChange?.(e.from);
        if (!to || !from) break;
        // ?me 视角只画牵涉「我」的信使，别人之间的往来只闪港灯，海面上不横七竖八
        if (this.me && e.to !== this.me && e.from !== this.me) break;
        const d = to.pos.distanceTo(from.pos);
        // 请求的信号弹先飞到，回信才起飞：离线数据里整条链几十毫秒就走完，画面按因果顺序排开
        if (e.status === 'sent') this.launch(from.pos, to.pos, this.animAt(st, now, T.signal), T.signal, FLY.signal, Math.min(d * 0.16, 60 * this.scale) + 8);
        else this.launch(to.pos, from.pos, this.animAt(st, now, T.reply), T.reply, e.status === 'granted' ? FLY.granted : FLY.denied, Math.min(d * 0.14, 50 * this.scale) + 6, e.status === 'granted');
        break;
      }
      case 'edge': {
        if (e.state !== 'gone') {
          const st = this.story(e.a, e.b);
          if (!st.ka) { st.ka = e.a; st.kb = e.b; }
          if (e.q) { st.dq = e.q; st.dqText = e.q_text ?? null; st.dp = e.conf; }
          else if (typeof e.conf === 'number') {
            const hit = this.findDecisive(st, e.conf);
            if (hit) { st.dq = hit.q; st.dqText = null; st.dp = e.conf; } else { st.dp = e.conf; }
          }
          if (e.tier_seen) st.tierSeen = e.tier_seen;
          if (e.lacks) st.hostLacks = e.lacks;
          if (st.await) this.rejudge(st, e.conf, true, now);
        } else if (typeof e.conf === 'number') {
          // gone 带读数 = 重判后不再成立；不带读数是单元被移除（比如新港离开），不算再判
          const st = this.stories.get(this.pairKey(e.a, e.b));
          if (st?.await) this.rejudge(st, null, false, now, e.conf);
        }
        this.edge(e.a, e.b, e.form, e.conf, e.state, now);
        break;
      }
      case 'config': this.upsertConfig(e, now, false); break;
      case 'config_grow': {
        const c = this.configs.get(e.id); const n = e.add ? this.nodes.get(e.add) : undefined;
        if (n) { this.ports.flashAt(n.idx, FLASH.member, now); this.ring(n.pos, now, 1.2, 9 * this.scale, RING.member); }
        if (c && n && e.add && !c.members.includes(e.add)) c.members.push(e.add);
        break;
      }
      case 'plan': {
        const c = this.configs.get(e.config); if (!c) break;
        const had = !!c.plan;
        c.plan = { title: e.title ?? c.plan?.title ?? null, summary: e.summary ?? c.plan?.summary ?? null, conf: e.conf ?? c.plan?.conf ?? null };
        if (c.stage !== 'dissolved') c.stage = 'plan';
        if (!had) this.onMoment?.({ kind: 'plan', id: c.id, t: now });
        this.onChange?.(c.nodeId ?? c.id);
        break;
      }
      case 'invalidate': {
        // 受影响的港灯同时暗一下：读作“这些判断要重算”
        const ids = e.ids ?? (e.id ? [e.id] : []);
        for (const id of ids.slice(0, 500)) { const n = this.nodes.get(id); if (n) this.ports.flashAt(n.idx, FLASH.invalidate, now); }
        // 模拟源只给出变化的那一个点：把它的邻居一起算进来
        if (ids.length === 1) {
          const n = this.nodes.get(ids[0]);
          if (n) for (const k of n.edges) { const ed = this.edges.get(k); if (!ed) continue; const o = this.nodes.get(ed.a === n.id ? ed.b : ed.a); if (o) this.ports.flashAt(o.idx, FLASH.invalidate, now); }
        }
        break;
      }
      case 'stats': { const { t: _t, type: _ty, ...s } = e; this.stats = { ...this.stats, ...s }; this.onStats?.(); break; }
      case 'spotlight': this.onMoment?.({ kind: 'spotlight', id: e.id, why: e.why, to: e.to, t: now }); break;
      case 'batch': break;
    }
    if (this.me && this.onMine && this.involves(e, this.me)) this.onMine();
  }

  // ---------- 判断簿 ----------
  pairKey(a: string, b: string) { return a < b ? `${a}|${b}` : `${b}|${a}`; }
  story(a: string, b: string, key = this.pairKey(a, b)): Story {
    let st = this.stories.get(key);
    if (!st) { st = { key, a, b, recent: [], steps: [], lacks: new Map(), animT: 0 }; this.stories.set(key, st); }
    return st;
  }
  /** 一道题落在哪本簿上：带 config 的进构型；两方构型的 hold 题进 cfg:a+b；其余进这一对。 */
  private storyFor(a: string, b: string | null, config: string | null, q?: string): Story | undefined {
    if (config) { const c = this.configs.get(config); return this.story(c?.members[0] ?? a, c?.members[1] ?? (b ?? a), config); }
    if (!b) return undefined;
    if (q === 'hold') {
      const cid = `cfg:${[a, b].sort().join('+')}`;
      if (this.configs.has(cid)) return this.story(a, b, cid);
    }
    return this.story(a, b);
  }
  private static KEEP_Q = new Set(['open', 'catcher', 'hold', 'value', 'timing', 'tighter']);
  private noteJudge(e: Judge, now: number) {
    if (!Net.KEEP_Q.has(e.q)) return;
    const st = this.storyFor(e.a, e.b ?? null, e.config ?? null, e.q); if (!st) return;
    if (!st.ka && e.b && !e.config) { st.ka = e.a; st.kb = e.b; }
    st.recent.push({ q: e.q, p: e.p, exit: e.exit, batch: e.batch, value: e.value });
    if (st.recent.length > 24) st.recent.splice(0, st.recent.length - 24);
  }
  /** 披露之后的下一次读数 = 再判（边或构型的置信度，前后都是决定性题的读数） */
  private rejudge(st: Story, p: number | null, exitAct: boolean, now: number, pNo: number | null = null) {
    if (!st.await) return;
    const p0 = st.await.p0; st.await = undefined;
    st.steps.push({ k: 'rejudge', t: now, p0, p, pNo });
    this.trim(st);
    const at = this.animAt(st, now, 0.5);
    const A = this.nodes.get(st.a), B = this.nodes.get(st.b);
    if (A) this.ports.flashAt(A.idx, exitAct ? FLASH.act : FLASH.ignore, at);
    if (B) this.ports.flashAt(B.idx, exitAct ? FLASH.act : FLASH.ignore, at);
    this.onMoment?.({ kind: 'rejudge', key: st.key, a: st.a, b: st.b, p0, p, pNo, t: now });
    this.onChange?.(st.a); this.onChange?.(st.b);
  }
  /** 边的读数 = 决定性题的读数：在这一对最近的 open/catcher act 判断里找读数一致的那一道。 */
  private findDecisive(st: Story, conf: number): JRec | undefined {
    for (let i = st.recent.length - 1; i >= 0; i--) {
      const r = st.recent[i];
      if ((r.q === 'open' || r.q === 'catcher') && r.exit === 'act' && Math.abs(r.p - conf) < 0.0006) return r;
    }
    return undefined;
  }
  /** 这一对的价值档位：与它的 open/catcher 同一批的 value 题（两方构型的整体判断也问 value，用批次分开） */
  valueOf(st: Story): number | null {
    const batches = new Set(st.recent.filter((r) => r.q === 'open' || r.q === 'catcher' || r.q === 'hold').map((r) => r.batch));
    for (let i = st.recent.length - 1; i >= 0; i--) {
      const r = st.recent[i];
      if (r.q === 'value' && batches.has(r.batch) && typeof r.value === 'number') return r.value;
    }
    return null;
  }
  private trim(st: Story) { if (st.steps.length > 16) st.steps.splice(0, st.steps.length - 16); }
  /** 同一对的动画按顺序排：返回这一步开始的时刻，并把游标推后 dur。积压超过 5 秒就从现在重新排。 */
  private animAt(st: Story, now: number, dur: number): number {
    const start = st.animT > now && st.animT - now < 5 ? st.animT : now;
    st.animT = start + dur;
    return start;
  }

  private involves(e: NetEvent, me: string): boolean {
    switch (e.type) {
      case 'edge': return e.a === me || e.b === me;
      case 'config': return e.members.includes(me) || (this.configs.get(e.id)?.members.includes(me) ?? false);
      case 'config_grow': return e.add === me || (this.configs.get(e.id)?.members.includes(me) ?? false);
      case 'plan': return this.configs.get(e.config)?.members.includes(me) ?? false;
      case 'disclose_request': return e.to === me || e.from === me;
      case 'disclose': return e.id === me || e.to === me;
      case 'unsure_route': return e.a === me || e.b === me;
      case 'node_join': return e.id === me || (e.members?.includes(me) ?? false);
      case 'node_leave': return e.id === me;
      case 'snapshot': return true;
      default: return false;
    }
  }

  private snapshot(e: Extract<NetEvent, { type: 'snapshot' }>, now: number) {
    // 语义坐标的比例按快照里全部 agent 一次定下
    this.layout.calibrate(e.nodes.filter((n) => n.kind === 'agent' && n.vec3).map((n) => n.vec3!));
    for (const n of e.nodes) if (n.kind === 'agent') this.addNode({ ...n, t: e.t, type: 'node_join' }, now, true);
    for (const c of e.configs) this.upsertConfig({ ...c, t: e.t, type: 'config' }, now, true);
    // 构型节点按嵌套深度依次建：成员港先在，新港才有位置
    let rest = e.nodes.filter((n) => n.kind === 'config');
    for (let pass = 0; rest.length && pass < 12; pass++) {
      const next: typeof rest = [];
      for (const n of rest) {
        const ms = n.members ?? this.configs.get(n.config ?? n.id)?.members ?? [];
        const ready = pass >= 11 || ms.every((m) => this.nodes.has(m) || !e.nodes.some((x) => x.id === m));
        if (ready) this.addNode({ ...n, t: e.t, type: 'node_join' }, now, true); else next.push(n);
      }
      rest = next;
    }
    for (const ed of e.edges) this.edge(ed.a, ed.b, ed.form, ed.conf, ed.state ?? 'up', now - 5);
    this.stats = { ...this.stats, ...e.stats }; this.onStats?.();
  }

  private configNode(cid: string): NodeRec | undefined {
    const n = this.nodes.get(cid); if (n) return n;
    const c = this.configs.get(cid); return c?.nodeId ? this.nodes.get(c.nodeId) : undefined;
  }

  private isMeta(c: ConfigRec) {
    return c.shape === 'meta' || c.members.some((m) => this.configs.has(m) || this.nodes.get(m)?.kind === 'config');
  }

  private addNode(e: Extract<NetEvent, { type: 'node_join' }>, now: number, silent: boolean) {
    const old = this.nodes.get(e.id);
    if (old) {
      if (old.alive && silent) return; // 重连后的快照：已经在的港不动
      old.alive = true; old.joinT = silent ? old.joinT : now;
      this.ports.revive(old.idx, old.pos, old.tier, now);
      if (!silent && old.kind === 'agent') this.joinFx(old, now);
      return;
    }
    let pos: Vector3; let configId: string | undefined; let born = silent ? now - 10 : now;
    let c: ConfigRec | undefined;
    if (e.kind === 'config') {
      configId = e.config ?? (this.configs.has(e.id) ? e.id : [...this.configs.values()].find((x) => x.nodeId === e.id)?.id);
      c = configId ? this.configs.get(configId) : undefined;
      if (!c && e.members?.length) {
        configId = configId ?? e.id;
        c = { id: configId, shape: (e.shape ?? 'pair') as Shape, members: [...e.members], roles: {}, conf: null, stage: 'judged' };
        this.configs.set(configId, c);
      } else if (c && e.shape && !c.shape) c.shape = e.shape;
      const pts = c ? c.members.map((m) => this.nodes.get(m)?.pos).filter(Boolean) as Vector3[] : [];
      pos = c?.port ?? (pts.length ? this.layout.portPos(c!.id, pts) : this.layout.agentPos(e.id, null, e.vec3));
      if (c) {
        c.nodeId = e.id; c.port = pos; c.meta = this.isMeta(c);
        // 开垦：真后端在构型提升为节点时才有新港，船队此刻出发，新港在船队抵达时点亮
        if (!silent && c.fleetT === undefined) born = this.fleet(c, pts, pos, now);
      }
    } else {
      pos = this.layout.agentPos(e.id, e.city, e.vec3);
    }
    const kind = e.kind === 'config' ? 1 : 0;
    const tier = Math.min(MAX_TIER, e.tier ?? 0);
    // 构型港的“层级”位用来分大小：两方的互补、转介是小港，三方以上与再组合是大港
    const small = !!c && !c.meta && (c.shape === 'pair' || c.shape === 'relay');
    const idx = this.ports.add(pos, kind, kind ? (small ? 0 : MAX_TIER) : tier, born);
    const full = e.kind === 'config' && c ? this.configLabel(c) : (e.label || e.id);
    const rec: NodeRec = {
      id: e.id, idx, kind: e.kind, label: e.kind === 'config' && c ? full : shortLabel(full), full,
      host: e.host_agent ?? undefined, city: e.city ?? undefined, lang: e.lang ?? undefined, tier, pos,
      alive: true, edges: new Set(), configs: new Set(), missing: new Map(), seen: new Map(), joinT: silent ? -1 : now, configId, real: e.real,
    };
    this.nodes.set(e.id, rec); this.byIdx[idx] = rec;
    if (e.kind === 'config') {
      if (!silent) {
        this.ring(pos, born, T.ignite * 2.4, 22 * this.scale, RING.ignite);
        this.ports.flashAt(idx, FLASH.ignite, born);
      }
      // 蓝图只给三方以上与再组合：两方的拓扑就是海面上那一条航线，再画一张会把画面盖满（真数据里两方构型占多数）
      // 冷白描线只给真的派了船队的那一次开垦；没派船（额度用完）的新港直接以常驻亮度挂上蓝图
      if (c && !small) this.blue.add(c.id, pos, c.meta && c.shape === 'meta' ? 'meta' : c.shape, c.members.length, silent || !(c.fleetT && c.fleetT > 0) ? now - 30 : born, this.scale * Math.sqrt(this.layout.R0 / this.layout.D));
      // 构型再组合：全片唯一的破例，一道光从新港扫过整个网络。真后端里再组合很频繁，75 秒内只扫一次
      if (c && c.meta && !silent && born - this.lastSweep > 75) { this.lastSweep = born; shared.uPulse.value.set(pos.x, pos.z, born + 0.3, this.layout.D * 0.55); }
      if (c) for (const m of c.members) { const mn = this.nodes.get(m); if (mn) { mn.configs.add(c.id); this.updateOpp(mn); } }
    } else if (!silent) this.joinFx(rec, now);
  }

  /** 派船队；返回新港点亮的时刻。没有船队额度时直接点亮。 */
  private fleet(c: ConfigRec, pts: Vector3[], to: Vector3, now: number): number {
    if (this.fleetTokens < 1 || !pts.length) { c.fleetT = -1; return now; }
    this.fleetTokens--;
    c.fleetT = now;
    for (const p of pts) for (let s = 0; s < 6; s++) this.launch(p, to, now + s * 0.16, T.fleet, FLY.fleet, 0, true);
    this.highlightInternal(c, now);
    const born = now + T.fleet;
    this.onMoment?.({ kind: 'fleet', id: c.id, t: now, born, meta: !!c.meta });
    return born;
  }

  private joinFx(n: NodeRec, now: number) {
    const sky = this.tmp.set(n.pos.x - 60 * this.scale, n.pos.y + 420 * this.scale, n.pos.z + 90 * this.scale);
    this.flights.launch(sky, n.pos, now, T.joinFall, FLY.join, 0);
    this.ring(n.pos, now + T.joinFall - 0.05, T.ripple * 1.6, 18 * this.scale, RING.join);
    this.onMoment?.({ kind: 'join', id: n.id, t: now });
  }

  private edge(a: string, b: string, form: string | null, conf: number | null, state: string, now: number) {
    const A = this.nodes.get(a), B = this.nodes.get(b);
    if (!A || !B) return;
    const k = a < b ? `${a}|${b}` : `${b}|${a}`;
    let ed = this.edges.get(k);
    const st = EDGE_STATE[state as keyof typeof EDGE_STATE] ?? 1;
    const f = formLabel(form ?? ed?.form);
    if (!ed) {
      if (state === 'gone') { this.glimpse(a, b, now); return; }
      // ?me 的机会：沿用召回时画好的那条线，收成亮航线（读数不变，只是画得更粗更亮，并闪一下）
      const mine = !!this.me && (a === this.me || b === this.me);
      const cs = mine ? this.cands.get(a === this.me ? b : a) : undefined;
      let slot: number;
      if (cs && cs.spoke !== undefined) { slot = cs.spoke; cs.spoke = undefined; } else { slot = this.allocSlot(); }
      this.slotKey[slot] = undefined;
      ed = { a, b, slot, conf: conf ?? 0.5, form: f, state, t: now, vis: mine ? 1 : undefined };
      this.edges.set(k, ed); this.slotKey[slot] = k;
      A.edges.add(k); B.edges.add(k);
      this.routes.set(slot, A.pos, B.pos, ed.vis ?? ed.conf, mine || state === 'up' ? EDGE_STATE.up : EDGE_STATE.new, now, hash(k));
      if (mine) {
        this.routes.highlight(slot, now);
        const cn = a === this.me ? B : A; this.ports.live(cn.idx, now);
        this.ring(A.pos, now, 1.4, 12 * this.scale, RING.member); this.ring(B.pos, now, 1.4, 12 * this.scale, RING.member);
        this.later.push({ t: now + 0.6, fn: () => this.routes.highlight(slot, now + 0.6) });
      }
      // 新接入者的第一个机会
      for (const n of [A, B]) if (n.joinT > 0 && (now - this.curLag - n.joinT) * this.speed < 120 && ![...n.edges].some((x) => x !== k && this.edges.get(x)?.state !== 'gone')) {
        const other = n === A ? B : A;
        this.onMoment?.({ kind: 'firstOpp', id: n.id, other: other.id, form: f, dt: (now - this.curLag - n.joinT) * this.speed, t: now });
      }
    } else {
      if (conf !== null) ed.conf = conf;
      ed.state = state; ed.form = f; ed.t = now;
      this.routes.set(ed.slot, A.pos, B.pos, ed.vis !== undefined && state !== 'gone' ? Math.max(ed.vis, ed.conf) : ed.conf, st, now, hash(k));
    }
    if (state === 'up' || state === 'new') { this.ports.flashAt(A.idx, FLASH.act, now); this.ports.flashAt(B.idx, FLASH.act, now); }
    this.updateOpp(A); this.updateOpp(B);
  }
  private dropEdge(k: string) {
    const ed = this.edges.get(k); if (!ed) return;
    this.edges.delete(k);
    this.nodes.get(ed.a)?.edges.delete(k); this.nodes.get(ed.b)?.edges.delete(k);
  }
  /** 每帧：回收褪尽的航线、补航班与船队额度、执行到点的延迟动作。 */
  reclaim(now: number) {
    for (const slot of this.routes.reclaim(now)) { const k = this.slotKey[slot]; if (k) { this.dropEdge(k); this.slotKey[slot] = undefined; } }
    const dt = Math.max(0, Math.min(0.5, now - this.lastT)); this.lastT = now;
    this.flightTokens = Math.min(this.flightCap, this.flightTokens + dt * this.flightRate);
    this.fleetTokens = Math.min(3, this.fleetTokens + dt / 2.5);
    if (this.later.length) {
      const due = this.later.filter((x) => x.t <= now); this.later = this.later.filter((x) => x.t > now);
      for (const x of due) x.fn();
    }
  }

  private updateOpp(n: NodeRec) {
    let c = 0;
    for (const id of n.configs) if (this.configs.get(id)?.stage !== 'dissolved') c++;
    for (const k of n.edges) if (this.edges.get(k)?.state !== 'gone') c++;
    this.ports.setOpp(n.idx, c);
    this.onChange?.(n.id);
  }

  private upsertConfig(e: ConfigEv, now: number, silent: boolean) {
    if (!silent && e.stage !== 'dissolved' && typeof e.conf === 'number') { const cs = this.stories.get(e.id); if (cs?.await) this.rejudge(cs, e.conf, true, now); }
    let c = this.configs.get(e.id);
    const prevStage = c?.stage;
    const shape = (e.shape ?? c?.shape ?? 'pair') as Shape;
    if (!c) { c = { id: e.id, shape, members: [...e.members], roles: { ...e.roles }, conf: e.conf, stage: e.stage }; this.configs.set(e.id, c); }
    else {
      if (e.members.length) c.members = [...e.members];
      c.roles = { ...e.roles }; if (e.conf !== null) c.conf = e.conf; c.shape = shape;
      // 方案已到的构型，宿主仍会按 judged 重发：不把阶段退回去
      if (!(c.stage === 'plan' && (e.stage === 'judged' || e.stage === 'candidate'))) c.stage = e.stage;
    }
    c.meta = this.isMeta(c);
    for (const m of c.members) { const n = this.nodes.get(m); if (n) { n.configs.add(c.id); this.updateOpp(n); } }
    if (silent) {
      if (e.stage === 'plan' || e.plan_title) { c.plan = { title: e.plan_title ?? null, summary: null, conf: null }; c.planT = now - 60; c.fleetT = -1; }
      return;
    }
    if (c.nodeId) this.onChange?.(c.nodeId);
    if (e.stage === prevStage && e.stage !== 'growing') return;
    if (e.stage === 'candidate' || e.stage === 'judged' || e.stage === 'growing') {
      for (const m of c.members) {
        const n = this.nodes.get(m); if (!n) continue;
        this.ports.flashAt(n.idx, FLASH.member, now);
        if (e.stage !== 'growing') this.ring(n.pos, now, 1.4, 9 * this.scale, RING.member);
      }
      this.highlightInternal(c, now);
      this.onMoment?.({ kind: 'config', id: c.id, stage: e.stage, t: now });
    } else if (e.stage === 'plan') {
      // 模拟源：先定案、后提升为节点；船队此刻出发，node_join 到时新港已就位
      const pts = c.members.map((m) => this.nodes.get(m)?.pos).filter(Boolean) as Vector3[];
      c.port = this.layout.portPos(c.id, pts); c.planT = now;
      if (c.fleetT === undefined) this.fleet(c, pts, c.port, now);
    } else if (e.stage === 'dissolved') {
      if (c.nodeId) this.blue.dissolve(c.id, now);
      for (const m of c.members) { const n = this.nodes.get(m); if (n) { n.configs.delete(c.id); this.ports.flashAt(n.idx, FLASH.ignore, now); this.updateOpp(n); } }
    }
  }

  private highlightInternal(c: ConfigRec, now: number) {
    for (let i = 0; i < c.members.length; i++) for (let j = i + 1; j < c.members.length; j++) {
      const a = c.members[i], b = c.members[j];
      const ed = this.edges.get(a < b ? `${a}|${b}` : `${b}|${a}`);
      if (ed) this.routes.highlight(ed.slot, now);
    }
  }

  /** 屏幕拾取：投影所有港，取离点击最近的。只在点击时跑一次。 */
  pick(x: number, y: number, w: number, h: number, project: (v: Vector3) => Vector3): NodeRec | undefined {
    let best: NodeRec | undefined; let bd = 18 * 18;
    for (const n of this.byIdx) {
      if (!n || !n.alive) continue;
      const p = project(this.tmp2.copy(n.pos));
      if (p.z > 1) continue;
      const sx = (p.x * 0.5 + 0.5) * w, sy = (-p.y * 0.5 + 0.5) * h;
      const d = (sx - x) ** 2 + (sy - y) ** 2;
      const bias = n.kind === 'config' ? 0.5 : 1;
      if (d * bias < bd) { bd = d * bias; best = n; }
    }
    return best;
  }

  upload() { this.land.flush(); this.ports.upload(); this.routes.upload(); this.flights.upload(); this.rings.upload(); }

  shapeName(s: Shape) { return SHAPE_NAME[s] ?? '构型'; }
}
