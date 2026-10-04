// 模拟事件源：同协议、真实节奏。浏览器（?src=mock）与 node（ws 服务器、jsonl 录制）共用。
// 节奏：每秒几十到几百个 judge（随 n 增长），偶尔构型结晶，偶尔新节点接入。
// 复杂度：每步只抽样，不枚举节点对，n=10000 时仍是亚二次。

import type {
  NetEvent, Snapshot, Shape, EdgeEv, ConfigEv, Stats,
} from '../protocol';
import { Rng, Heap } from './rng';
import {
  SURNAMES, GIVEN, EN_FIRST, EN_LAST, CITIES_ZH, CITIES_EN, HOSTS, OCCUPATIONS, TOPICS,
  EDGE_FORMS, QUESTIONS, MISSING, PROJECTS, PLAN_VERBS,
} from './names';

interface Agent {
  idx: number; id: string; label: string; host: string; lang: 'zh' | 'en'; city: string;
  occ: string; topics: number[]; tier: number; joined: boolean; joinT: number;
  structs: number[]; firstActT: number; alive: boolean; vec3: [number, number, number];
}
interface Struct {
  idx: number; shape: Shape; members: number[]; // agent idx；meta 的第一个成员是子构型（负数编码见 subStruct）
  roles: string[]; found: Set<number>; stage: 'hidden' | 'candidate' | 'growing' | 'judged' | 'plan';
  configId: string; nodeId?: string; conf: number; subStruct?: number; project: string; partner?: number; hits: number;
}
interface Edge { a: number; b: number; conf: number; form: string; state: 'new' | 'up' | 'down' | 'gone'; doomed: boolean; falseCfg?: string }
interface Task { t: number; seq: number; fn: () => void }

const STRUCT_MIX: [Shape, number, number, number][] = [
  // shape, 每 500 个节点的数量, 最少人数, 最多人数
  ['pair', 70, 2, 2], ['relay', 20, 2, 2], ['chain', 10, 3, 4], ['ring', 8, 3, 4],
  ['team', 15, 3, 5], ['star', 10, 4, 6], ['m2m', 5, 6, 6], ['meta', 6, 3, 4],
];

export interface MockOptions { n?: number; seed?: number; warmup?: number; reserve?: number }

export class MockNetwork {
  readonly n: number;
  t: number; // 服务器时间（秒）
  private rng: Rng;
  private agents: Agent[] = [];
  private structs: Struct[] = [];
  private edges = new Map<string, Edge>();
  private edgesOf: Map<number, Set<string>> = new Map();
  private topicJoined: number[][] = TOPICS.map(() => []);
  private joinedList: number[] = [];
  private heap = new Heap<Task>();
  private seq = 0;
  private out: NetEvent[] = [];
  private silent = false;
  private pendingJudges: { a: number; b: number; clarity: number; config?: string; q?: string }[] = [];
  private batchNo = 0; private reqNo = 0;
  private calls = 0; private questions = 0; private judgesThisSec = 0; private lastQps = 0;
  private joinLatencies: number[] = [];
  private nextBatchT = 0; private nextStatsT = 0; private nextJoinT = 0; private nextSpotT = 0;
  private probeAcc = 0; private nextLeaveT = Infinity;
  private readonly scale: number;

  constructor(opts: MockOptions = {}) {
    this.n = opts.n ?? 500;
    this.rng = new Rng(opts.seed ?? 7);
    this.scale = Math.sqrt(this.n / 500);
    this.t = Math.floor(Date.now() / 1000);
    this.buildAgents();
    this.buildStructs();
    // 先让一部分人在场，剩下的人随时间陆续接入
    const reserve = opts.reserve ?? Math.round(this.n * 0.07);
    const order = this.agents.map((a) => a.idx);
    for (let i = order.length - 1; i > 0; i--) { const j = this.rng.int(0, i); [order[i], order[j]] = [order[j], order[i]]; }
    const reserveSet = new Set(order.slice(0, reserve));
    this.silent = true;
    for (const a of this.agents) if (!reserveSet.has(a.idx)) this.join(a.idx, true);
    this.joinQueue = order.slice(0, reserve);
    // 暖机：静默跑一段，让快照里已有航线与构型
    const warm = opts.warmup ?? 110;
    this.nextBatchT = this.t; this.nextStatsT = this.t + 1; this.nextJoinT = this.t + warm + 4; this.nextSpotT = this.t + warm + 6; this.nextLeaveT = this.t + warm + 30;
    const end = this.t + warm;
    while (this.t < end) this.advance(0.25);
    this.silent = false;
    this.out = [];
  }
  private joinQueue: number[] = [];

  // ---------- 构造 ----------
  private buildAgents() {
    const r = this.rng;
    const used = new Set<string>();
    for (let i = 0; i < this.n; i++) {
      const en = r.chance(0.2);
      let label = '';
      for (let k = 0; k < 20; k++) {
        label = en ? `${r.pick(EN_FIRST)} ${r.pick(EN_LAST)}` : `${SURNAMES[r.int(0, SURNAMES.length - 1)]}${r.pick(GIVEN)}`;
        if (!used.has(label)) break;
      }
      used.add(label);
      const t1 = r.int(0, TOPICS.length - 1);
      let t2 = r.int(0, TOPICS.length - 1); if (t2 === t1) t2 = (t1 + 3) % TOPICS.length;
      this.agents.push({
        idx: i, id: `a${String(i).padStart(this.n > 9999 ? 5 : 4, '0')}`, label, host: r.pick(HOSTS),
        lang: en ? 'en' : 'zh', city: en ? (r.chance(0.6) ? r.pick(CITIES_EN) : r.pick(CITIES_ZH)) : r.pick(CITIES_ZH),
        occ: r.pick(OCCUPATIONS), topics: [t1, t2], tier: 0, joined: false, joinT: 0, structs: [], firstActT: -1, alive: true,
        vec3: semanticPos(t1, t2, r),
      });
    }
  }

  private buildStructs() {
    const r = this.rng;
    const k = this.n / 500;
    const byTopic: number[][] = TOPICS.map(() => []);
    for (const a of this.agents) byTopic[a.topics[0]].push(a.idx);
    const pickMembers = (m: number): number[] | null => {
      for (let attempt = 0; attempt < 30; attempt++) {
        const pool = byTopic[r.int(0, TOPICS.length - 1)];
        const set = new Set<number>();
        for (let g = 0; g < m * 6 && set.size < m; g++) {
          const c = r.chance(0.9) ? r.pick(pool) : r.int(0, this.n - 1);
          if (this.agents[c].structs.length < 2) set.add(c);
        }
        if (set.size === m) return [...set];
      }
      return null;
    };
    for (const [shape, per500, lo, hi] of STRUCT_MIX) {
      const count = Math.max(1, Math.round(per500 * k));
      for (let i = 0; i < count; i++) {
        const m = r.int(lo, hi);
        if (shape === 'meta') {
          // 先有一个 2–3 人的团队，团队整体才匹配第四方
          const sub = pickMembers(m - 1); const partner = pickMembers(1);
          if (!sub || !partner) continue;
          const subIdx = this.addStruct('team', sub);
          const s = this.addStruct('meta', sub.concat(partner));
          this.structs[s].subStruct = subIdx; this.structs[s].partner = partner[0];
          continue;
        }
        const mem = pickMembers(m);
        if (mem) this.addStruct(shape, mem);
      }
    }
  }
  private addStruct(shape: Shape, members: number[]): number {
    const idx = this.structs.length;
    const roles = members.map((_, i) => roleFor(shape, i, members.length));
    this.structs.push({
      idx, shape, members, roles, found: new Set(), stage: 'hidden', configId: `c${String(idx).padStart(4, '0')}`,
      conf: 0, project: this.rng.pick(PROJECTS), hits: 0,
    });
    if (shape !== 'meta') for (const m of members) this.agents[m].structs.push(idx);
    return idx;
  }

  // ---------- 对外 ----------
  snapshot(): Snapshot {
    const nodes: Snapshot['nodes'] = [];
    for (const a of this.agents) if (a.joined && a.alive) nodes.push(this.nodeFields(a));
    const configs: Snapshot['configs'] = [];
    for (const s of this.structs) {
      if (s.stage === 'hidden') continue;
      configs.push(this.configFields(s));
      if (s.stage === 'plan' && s.nodeId) nodes.push({ id: s.nodeId, kind: 'config', label: this.configLabel(s), tier: 2, config: s.configId, members: this.configFields(s).members, shape: s.shape });
    }
    const edges: Snapshot['edges'] = [];
    for (const e of this.edges.values()) if (e.state !== 'gone') edges.push(this.edgeFields(e));
    return { t: this.t, type: 'snapshot', nodes, edges, configs, stats: this.statsFields() };
  }

  /** 前进 dt 秒，返回这段时间里发生的事件（按时间排序）。 */
  step(dt: number): NetEvent[] {
    const end = this.t + dt;
    while (this.t < end) this.advance(Math.min(0.05, end - this.t));
    const o = this.out; this.out = []; return o;
  }

  // ---------- 主循环 ----------
  private advance(dt: number) {
    const t0 = this.t; const t1 = t0 + dt;
    // 判断速率随时间起伏：慢正弦 + 偶发爆发
    const phase = (t1 % 90) / 90;
    const burst = Math.max(0, Math.sin(t1 * 0.7)) ** 12;
    const judgesPerSec = (70 + 140 * (0.5 + 0.5 * Math.sin(phase * Math.PI * 2)) + 180 * burst) * this.scale;
    this.probeAcc += (judgesPerSec / 7) * dt;
    while (this.probeAcc >= 1) { this.probeAcc -= 1; this.at(this.rng.range(t0, t1), () => this.probe()); }

    while (this.heap.size && this.heap.peek()!.t <= t1) {
      const task = this.heap.pop()!; this.t = Math.max(this.t, task.t); task.fn();
    }
    this.t = t1;
    if (t1 >= this.nextBatchT) { this.flushBatch(); this.nextBatchT = t1 + this.rng.range(0.12, 0.26); }
    if (t1 >= this.nextStatsT) { this.emitStats(); this.nextStatsT += 1; }
    if (!this.silent && t1 >= this.nextJoinT && this.joinQueue.length) {
      this.join(this.joinQueue.shift()!, false);
      this.nextJoinT = t1 + this.rng.range(9, 20) / Math.max(1, this.scale * 0.6);
    }
    if (!this.silent && t1 >= this.nextLeaveT) { this.leaveOne(); this.nextLeaveT = t1 + this.rng.range(25, 50) / this.scale; }
  }

  private at(t: number, fn: () => void) { this.heap.push({ t, seq: this.seq++, fn }); }
  private emit(e: NetEvent) { if (!this.silent) this.out.push(e); }

  // ---------- 接入 ----------
  private join(i: number, quiet: boolean) {
    const a = this.agents[i];
    a.joined = true; a.joinT = this.t;
    this.joinedList.push(i);
    for (const tp of a.topics) this.topicJoined[tp].push(i);
    if (quiet) return;
    this.emit({ t: this.t, type: 'node_join', ...this.nodeFields(a) });
    this.emit({ t: this.t + 0.1, type: 'invalidate', cause: 'join', n_judgments: this.rng.int(20, 90), ids: [a.id] });
    // 新人接入后几秒内连发几轮召回，结构伙伴大概率被召回
    for (let w = 0; w < 3; w++) this.at(this.t + 2.5 + w * 1.0, () => this.probe(i, 0.85));
  }

  // ---------- 召回与判断 ----------
  private probe(fromIdx?: number, partnerBias = 0.035) {
    const r = this.rng;
    const L = this.joinedList; if (L.length < 10) return;
    const from = fromIdx ?? L[r.int(0, L.length - 1)];
    const a = this.agents[from]; if (!a.alive) return;
    const k = r.int(4, 9);
    const to = new Set<number>();
    if (r.chance(partnerBias)) {
      for (const si of a.structs) {
        const s = this.structs[si];
        for (const m of s.members) if (m !== from && this.agents[m].joined && this.agents[m].alive) { to.add(m); break; }
      }
    }
    for (let g = 0; to.size < k && g < k * 4; g++) {
      const pool = r.chance(0.5) ? this.topicJoined[r.chance(0.8) ? a.topics[0] : a.topics[1]] : L;
      const c = pool[r.int(0, pool.length - 1)];
      if (c !== from && this.agents[c].alive) to.add(c);
    }
    const toArr = [...to];
    this.emit({ t: this.t, type: 'probe', from: a.id, to: toArr.map((x) => this.agents[x].id), stage: r.chance(0.85) ? 'recall' : 'operator' });
    // 信号到达后再入批
    this.at(this.t + r.range(0.9, 1.5), () => { for (const b of toArr) this.pendingJudges.push({ a: from, b, clarity: 0 }); });
  }

  private flushBatch() {
    const r = this.rng;
    const jobs = this.pendingJudges; if (!jobs.length) return;
    this.pendingJudges = [];
    const bid = `b${++this.batchNo}`;
    const nq = jobs.length * r.int(2, 4);
    this.calls++; this.questions += nq;
    this.emit({ t: this.t, type: 'batch', id: bid, n_states: jobs.length, n_questions: nq, latency_ms: Math.round(r.range(60, 180)), merged_from: r.int(1, Math.min(9, jobs.length)) });
    jobs.forEach((j, k) => this.judge(j, bid, this.t + k * 0.0006));
  }

  private trueRel(a: number, b: number): number {
    const A = this.agents[a], B = this.agents[b];
    for (const si of A.structs) if (B.structs.includes(si)) return 0.86;
    if (A.topics[0] === B.topics[0]) return 0.3;
    return A.topics.some((x) => B.topics.includes(x)) ? 0.16 : 0.06;
  }

  private judge(j: { a: number; b: number; clarity: number }, bid: string, t: number) {
    const r = this.rng;
    const A = this.agents[j.a], B = this.agents[j.b];
    if (!A.alive || !B.alive) return;
    const truth = this.trueRel(j.a, j.b);
    const tierInfo = (A.tier + B.tier) / 6;
    const noise = 0.23 * (1 - 0.65 * Math.max(j.clarity, tierInfo));
    const p = clamp01(truth + r.gauss() * noise);
    const exit = p >= 0.7 ? 'act' : p <= 0.48 ? 'ignore' : 'unsure';
    this.judgesThisSec++;
    this.emit({ t, type: 'judge', a: A.id, b: B.id, q: r.pick(QUESTIONS), p: round2(p), exit, batch: bid });
    if (exit === 'act') this.onAct(j.a, j.b, p, t);
    else if (exit === 'unsure') this.onUnsure(j.a, j.b, t);
    else {
      const e = this.edges.get(key(j.a, j.b));
      if (e && e.state !== 'gone' && e.doomed) this.at(t + r.range(0.3, 1.2), () => this.edgeDown(e));
    }
  }

  private onAct(a: number, b: number, p: number, t: number) {
    const r = this.rng;
    const k = key(a, b);
    let e = this.edges.get(k);
    const truth = this.trueRel(a, b);
    const A = this.agents[a];
    if (A.firstActT < 0 && !this.silent && A.joinT > 0 && t - A.joinT < 60) {
      A.firstActT = t; this.joinLatencies.push(t - A.joinT); if (this.joinLatencies.length > 40) this.joinLatencies.shift();
    }
    if (!e || e.state === 'gone') {
      e = { a, b, conf: round2(p * 0.85), form: this.formFor(a, b), state: 'new', doomed: truth < 0.5 && r.chance(0.75) };
      this.edges.set(k, e); this.link(a, k); this.link(b, k);
      this.emit({ t, type: 'edge', ...this.edgeFields(e) });
      if (e.doomed) this.at(t + r.range(6, 40), () => this.recheck(e!));
      // 难负例偶尔先被当成构型候选，等关系被否掉时解散：失败也画出来
      if (e.doomed && !this.silent && r.chance(0.08)) {
        e.falseCfg = `x${++this.falseNo}`;
        this.emit({ t: t + 0.02, type: 'config', id: e.falseCfg, shape: 'pair', members: [this.agents[a].id, this.agents[b].id], roles: { [this.agents[a].id]: '发起方', [this.agents[b].id]: '接收方' }, conf: round2(p * 0.7), stage: 'candidate' });
      }
    } else {
      const nc = round2(Math.min(0.97, e.conf + (truth > 0.5 ? 0.08 : 0.02)));
      if (nc > e.conf) { e.conf = nc; e.state = 'up'; this.emit({ t, type: 'edge', ...this.edgeFields(e) }); }
    }
    if (truth > 0.5) this.structProgress(a, b, t);
  }

  private recheck(e: Edge) {
    if (e.state === 'gone') return;
    // 难负例：补了信息、或新的判断后，发现不成立
    this.pendingJudges.push({ a: e.a, b: e.b, clarity: 0.9 });
  }
  private edgeDown(e: Edge) {
    if (e.state === 'gone') return;
    e.conf = round2(e.conf * 0.55);
    e.state = e.conf < 0.3 ? 'gone' : 'down';
    this.emit({ t: this.t, type: 'edge', ...this.edgeFields(e) });
    if (e.falseCfg) {
      const A = this.agents[e.a].id, B = this.agents[e.b].id;
      this.emit({ t: this.t + 0.02, type: 'config', id: e.falseCfg, shape: 'pair', members: [A, B], roles: { [A]: '发起方', [B]: '接收方' }, conf: e.conf, stage: 'dissolved' });
      e.falseCfg = undefined;
    }
    if (e.state === 'down') this.at(this.t + this.rng.range(3, 8), () => { e.state = 'gone'; e.conf = 0; this.emit({ t: this.t, type: 'edge', ...this.edgeFields(e) }); });
  }

  private onUnsure(a: number, b: number, t: number) {
    const r = this.rng;
    const A = this.agents[a], B = this.agents[b];
    const askIdx = A.tier <= B.tier ? a : b; const other = askIdx === a ? b : a;
    const missing = r.pick(MISSING);
    const roll = r.next();
    const relevant = this.trueRel(a, b) > 0.5;
    const route = roll < (relevant ? 0.7 : 0.22) ? 'disclose_request' : roll < 0.6 ? 'escalate' : roll < 0.88 ? 'drop' : 'return';
    this.emit({ t: t + 0.01, type: 'unsure_route', a: A.id, b: B.id, missing, ask_to: [this.agents[askIdx].id], route });
    if (route === 'disclose_request') {
      const rid = `rq${++this.reqNo}`;
      const Ask = this.agents[askIdx];
      this.emit({ t: t + 0.02, type: 'disclose_request', id: rid, to: Ask.id, from: this.agents[other].id, category: missing, purpose: `判断与${this.agents[other].label}是否成立`, status: 'sent' });
      const grant = Ask.tier < 2 && r.chance(relevant ? 0.85 : 0.15);
      this.at(t + r.range(1.4, 3.8), () => {
        this.emit({ t: this.t, type: 'disclose_request', id: rid, to: Ask.id, from: this.agents[other].id, category: missing, purpose: `判断与${this.agents[other].label}是否成立`, status: grant ? 'granted' : 'denied' });
        if (!grant) return;
        Ask.tier = Math.min(2, Ask.tier + 1);
        this.emit({ t: this.t + 0.05, type: 'disclose', id: Ask.id, to: this.agents[other].id, tier: Ask.tier, added_chars: r.int(120, 900), reason: missing });
        const deg = this.edgesOf.get(askIdx)?.size ?? 0;
        this.emit({ t: this.t + 0.08, type: 'invalidate', cause: 'disclose', n_judgments: 4 + deg * r.int(2, 6), ids: [Ask.id] });
        this.pendingJudges.push({ a, b, clarity: 0.75 });
      });
    } else if (route === 'escalate') {
      this.at(t + r.range(0.5, 1.5), () => this.pendingJudges.push({ a, b, clarity: 0.6 }));
    }
  }

  // ---------- 构型 ----------
  private structProgress(a: number, b: number, t: number) {
    const A = this.agents[a];
    for (const si of A.structs) {
      const s = this.structs[si];
      if (!s.members.includes(b)) continue;
      const before = s.found.size;
      s.hits++;
      s.found.add(a); s.found.add(b);
      this.advanceStruct(s, t, before);
    }
  }

  private advanceStruct(s: Struct, t: number, before: number) {
    const r = this.rng;
    const total = s.shape === 'meta' ? 0 : s.members.length;
    if (s.shape === 'meta') return;
    if (s.stage === 'plan') return;
    const foundIds = [...s.found];
    s.conf = round2(0.45 + 0.4 * (s.found.size / total) + r.range(-0.04, 0.06));
    if (s.stage === 'hidden') {
      s.stage = total > 2 ? 'candidate' : 'judged';
      this.emit({ t, type: 'config', ...this.configFields(s) });
    } else if (s.found.size > before) {
      const add = foundIds.find((x) => !this.lastMembers(s).includes(x));
      s.stage = 'growing';
      if (add !== undefined) this.emit({ t, type: 'config_grow', id: s.configId, add: this.agents[add].id, tighter_p: round2(r.range(0.66, 0.93)) });
      this.emit({ t: t + 0.01, type: 'config', ...this.configFields(s) });
    }
    if (s.found.size >= total && s.stage !== 'judged' && total > 2) {
      s.stage = 'judged';
      this.at(t + 0.6, () => { this.emit({ t: this.t, type: 'config', ...this.configFields(s) }); this.emit({ t: this.t, type: 'spotlight', id: s.configId, why: 'config_formed' }); });
    }
    if (s.found.size >= total && s.hits >= (total === 2 ? 5 : total + 2)) this.at(t + r.range(4, 7), () => this.toPlan(s));
  }
  private lastSeen = new Map<number, number[]>();
  private lastMembers(s: Struct) { const prev = this.lastSeen.get(s.idx) ?? []; this.lastSeen.set(s.idx, [...s.found]); return prev; }

  private nextPlanT = 0;
  private falseNo = 0;
  private toPlan(s: Struct) {
    if (s.stage === 'plan') return;
    const r = this.rng;
    // 写方案要调用生成器，慢且贵：方案按生成层的吞吐排队，n=500 时大约 12–25 秒出一份
    const feedsMeta = this.structs.some((m) => m.shape === 'meta' && m.subStruct === s.idx);
    if (!this.silent && !feedsMeta && this.t < this.nextPlanT) { this.at(this.nextPlanT + r.range(0, 2), () => this.toPlan(s)); return; }
    if (!this.silent) this.nextPlanT = this.t + r.range(12, 25) / this.scale;
    s.stage = 'plan';
    s.conf = round2(Math.min(0.95, s.conf + r.range(0.02, 0.08)));
    s.nodeId = `n${s.configId}`;
    this.emit({ t: this.t, type: 'config', ...this.configFields(s) });
    this.emit({ t: this.t + 0.02, type: 'spotlight', id: s.configId, why: 'plan_ready' });
    // 组合封闭：构型被提升为节点
    this.at(this.t + 3.2, () => {
      this.emit({ t: this.t, type: 'node_join', id: s.nodeId!, kind: 'config', label: this.configLabel(s), tier: 2, config: s.configId, members: this.configFields(s).members, shape: s.shape });
    });
    this.at(this.t + 4.4, () => {
      this.emit({ t: this.t, type: 'plan', config: s.configId, title: this.planTitle(s), summary: this.planSummary(s), conf: s.conf });
      // 子构型定案后，meta 构型开始找第四方
      for (const m of this.structs) if (m.shape === 'meta' && m.subStruct === s.idx) this.startMeta(m, s);
    });
    // 生活继续：新的需要会冒出来，补一个尚未被发现的结构
    this.at(this.t + r.range(20, 60), () => this.spawnStruct(s.shape === 'meta' ? 'team' : s.shape, s.members.length));
  }

  private startMeta(m: Struct, sub: Struct) {
    const r = this.rng;
    const partner = this.agents[m.partner!];
    if (!partner.joined) return;
    let tries = 0;
    const attempt = () => {
      tries++;
      const p = clamp01(0.82 + r.gauss() * 0.1);
      const bid = `b${++this.batchNo}`;
      this.emit({ t: this.t, type: 'batch', id: bid, n_states: 2, n_questions: 6, latency_ms: Math.round(r.range(80, 160)), merged_from: 1 });
      this.emit({ t: this.t, type: 'judge', a: partner.id, config: sub.nodeId, q: '团队整体是否匹配', p: round2(p), exit: p > 0.72 ? 'act' : 'unsure', batch: bid });
      if (p <= 0.72 && tries < 3) { this.at(this.t + r.range(3, 6), attempt); return; }
      this.emit({ t: this.t + 0.02, type: 'edge', a: sub.nodeId!, b: partner.id, dir: 'b>a', form: '投资', conf: round2(p), state: 'new' });
      m.stage = 'judged'; m.conf = round2(p);
      this.emit({ t: this.t + 0.05, type: 'config', ...this.configFields(m, sub) });
      this.at(this.t + r.range(4, 6), () => {
        m.stage = 'plan'; m.nodeId = `n${m.configId}`;
        this.emit({ t: this.t, type: 'config', ...this.configFields(m, sub) });
        this.emit({ t: this.t + 0.02, type: 'spotlight', id: m.configId, why: 'meta_formed' });
        this.at(this.t + 3.2, () => this.emit({ t: this.t, type: 'node_join', id: m.nodeId!, kind: 'config', label: `${sub.project}与${partner.label}`, tier: 2, config: m.configId, members: this.configFields(m, sub).members, shape: 'meta' }));
        this.at(this.t + 4.4, () => this.emit({ t: this.t, type: 'plan', config: m.configId, title: `${partner.label}整体投资「${sub.project}」团队`, summary: `团队作为一个整体与${partner.label}对接，单个成员与其两两都不相关。`, conf: m.conf }));
      });
    };
    this.at(this.t + r.range(3, 8), attempt);
  }

  private leaveOne() {
    const r = this.rng; const L = this.joinedList;
    for (let g = 0; g < 20; g++) {
      const k = r.int(0, L.length - 1); const a = this.agents[L[k]];
      if (a.structs.length || !a.alive) continue;
      L.splice(k, 1);
      for (const tp of a.topics) { const arr = this.topicJoined[tp]; const q = arr.indexOf(a.idx); if (q >= 0) arr.splice(q, 1); }
      a.joined = false; a.tier = 0; a.firstActT = -1;
      for (const ek of this.edgesOf.get(a.idx) ?? []) { const e = this.edges.get(ek); if (e && e.state !== 'gone') { e.state = 'gone'; e.conf = 0; } }
      this.edgesOf.delete(a.idx);
      this.emit({ t: this.t, type: 'node_leave', id: a.id });
      this.emit({ t: this.t + 0.05, type: 'invalidate', cause: 'leave', n_judgments: r.int(10, 60), ids: [a.id] });
      this.joinQueue.push(a.idx);
      return;
    }
  }

  private spawnStruct(shape: Shape, m: number) {
    const r = this.rng; const L = this.joinedList;
    const seed = L[r.int(0, L.length - 1)];
    const pool = this.topicJoined[this.agents[seed].topics[0]];
    const set = new Set<number>([seed]);
    for (let g = 0; g < m * 8 && set.size < m; g++) {
      const c = r.chance(0.75) ? pool[r.int(0, pool.length - 1)] : L[r.int(0, L.length - 1)];
      if (this.agents[c].alive && this.agents[c].structs.length < 2) set.add(c);
    }
    if (set.size === m) this.addStruct(shape, [...set]);
  }

  private emitStats() {
    const s = this.statsFields();
    this.lastQps = this.judgesThisSec; this.judgesThisSec = 0;
    this.emit({ t: this.t, type: 'stats', ...s, qps: this.lastQps });
  }

  // ---------- 字段 ----------
  private nodeFields(a: Agent) {
    return { id: a.id, kind: 'agent' as const, label: a.label, host_agent: a.host, lang: a.lang, city: a.city, tier: a.tier, vec3: a.vec3 };
  }
  private edgeFields(e: Edge): Omit<EdgeEv, 't' | 'type'> {
    return { a: this.agents[e.a].id, b: this.agents[e.b].id, dir: 'both', form: e.form, conf: e.conf, state: e.state };
  }
  private configFields(s: Struct, sub?: Struct): Omit<ConfigEv, 't' | 'type'> {
    if (s.shape === 'meta') {
      const subS = sub ?? this.structs[s.subStruct!];
      const p = this.agents[s.partner!].id;
      return { id: s.configId, shape: 'meta', members: [subS.nodeId ?? subS.configId, p], roles: { [subS.nodeId ?? subS.configId]: '团队', [p]: '投资方' }, conf: s.conf, stage: s.stage as ConfigEv['stage'] };
    }
    const members = s.members.filter((m) => s.found.has(m));
    const roles: Record<string, string> = {};
    s.members.forEach((m, i) => { if (s.found.has(m)) roles[this.agents[m].id] = s.roles[i]; });
    return { id: s.configId, shape: s.shape, members: members.map((m) => this.agents[m].id), roles, conf: s.conf, stage: s.stage as ConfigEv['stage'] };
  }
  private statsFields(): Omit<Stats, 't' | 'type'> {
    const configs = this.structs.filter((s) => s.stage !== 'hidden').length;
    const lat = [...this.joinLatencies].sort((x, y) => x - y);
    return {
      agents: this.joinedList.length, configs, calls: this.calls, questions: this.questions,
      cost_usd: round2(this.questions * 0.00004), qps: this.lastQps, cache_hit: round2(0.31 + 0.1 * Math.sin(this.t / 50)),
      p50_join_s: lat.length ? round2(lat[lat.length >> 1]) : 0,
    };
  }
  private formFor(a: number, b: number) {
    const A = this.agents[a], B = this.agents[b];
    for (const si of A.structs) if (B.structs.includes(si)) {
      const sh = this.structs[si].shape;
      return sh === 'relay' ? '转介' : sh === 'ring' ? '互换' : sh === 'team' ? '共创' : sh === 'star' ? '供需' : '互补';
    }
    return this.rng.pick(EDGE_FORMS);
  }
  private configLabel(s: Struct) {
    return s.shape === 'pair' || s.shape === 'relay' ? `${this.agents[s.members[0]].label}与${this.agents[s.members[1]].label}` : s.project;
  }
  private planTitle(s: Struct) {
    const names = s.members.map((m) => this.agents[m].label);
    if (s.shape === 'pair') return `${names[0]}与${names[1]}结对：${this.agents[s.members[1]].occ}帮${this.agents[s.members[0]].occ}`;
    if (s.shape === 'relay') return `${names[1]}把${names[0]}转介给身边能帮上的人`;
    return `「${s.project}」${PLAN_VERBS[s.shape]}`;
  }
  private planSummary(s: Struct) {
    const parts = s.members.map((m, i) => `${this.agents[m].label}（${s.roles[i]}）`);
    return `${parts.join('、')}。每人补上一块，缺一块就不成。`;
  }
  private link(i: number, k: string) { let s = this.edgesOf.get(i); if (!s) { s = new Set(); this.edgesOf.set(i, s); } s.add(k); }
}

function roleFor(shape: Shape, i: number, n: number): string {
  switch (shape) {
    case 'pair': return i === 0 ? '发起方' : '接收方';
    case 'relay': return i === 0 ? '需要方' : '转介人';
    case 'chain': return i === 0 ? '上游' : i === n - 1 ? '下游' : '中段';
    case 'ring': return `第${i + 1}环`;
    case 'team': return i === 0 ? '发起人' : '成员';
    case 'star': return i === 0 ? '中心' : '外围';
    case 'm2m': return i < n / 2 ? '供给方' : '需求方';
    default: return i === n - 1 ? '投资方' : '团队成员';
  }
}
function key(a: number, b: number) { return a < b ? `${a}|${b}` : `${b}|${a}`; }
function clamp01(x: number) { return Math.max(0, Math.min(1, x)); }
function round2(x: number) { return Math.round(x * 100) / 100; }

// 语义布局：每个话题是一片群岛，主话题占大头，次话题把人往另一片岛拉，跨话题的人落在两岛之间
const TOPIC_C: [number, number][] = TOPICS.map((_, i) => {
  const a = i * 2.39996 + 0.4; const r = 0.16 + 0.8 * Math.sqrt((i + 0.5) / TOPICS.length);
  return [Math.cos(a) * r, Math.sin(a) * r];
});
function semanticPos(t1: number, t2: number, r: Rng): [number, number, number] {
  const w = r.chance(0.82) ? r.range(0.0, 0.12) : r.range(0.3, 0.55);
  const [x1, z1] = TOPIC_C[t1], [x2, z2] = TOPIC_C[t2];
  const s = 0.055;
  return [round3(x1 * (1 - w) + x2 * w + r.gauss() * s), round3(Math.abs(r.gauss()) * 0.3), round3(z1 * (1 - w) + z2 * w + r.gauss() * s)];
}
function round3(x: number) { return Math.round(x * 1000) / 1000; }
