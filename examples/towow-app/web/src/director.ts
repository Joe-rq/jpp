// 导演镜头：一镜到底，不切镜头。节奏只靠一个连续镜头里的推、拉、环绕、跟随与变速。
// 机位与注视点都用临界阻尼弹簧追目标，任何时候都不瞬移。
// 真后端的事件成阵到达：一阵里冒出几十个值得看的时刻，然后静默。导演把它们排成队，
// 每个镜头至少停 5 秒，按优先级和新近程度一个个拍；队里空了就慢慢巡航，久了绕一座真实存在的港看看。
import { PerspectiveCamera, Vector3 } from 'three';
import type { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { Net, Moment } from './net';
import { CAM } from './style';
import { Q_WORD } from './protocol';
import { Q_EN, formEn, shapeEn, plural, listEn, cap1 } from './captions';

type ShotKind = 'cruise' | 'home' | 'revisit' | 'push' | 'orbit' | 'follow' | 'pull';
interface Shot {
  kind: ShotKind; prio: number; start: number; min: number; max: number;
  target: () => Vector3 | undefined; dist: number; elev: number; azSpeed: number;
  subject?: string; onFramed?: () => void; framed?: boolean; close: boolean; omega?: number;
}
interface Cand { prio: number; t: number; ttl: number; make: (now: number) => Shot | undefined; key: string; subject: string }

class Spring {
  v = new Vector3();
  step(x: Vector3, target: Vector3, omega: number, dt: number) {
    // 临界阻尼：x'' = -2ωx' - ω²(x - target)
    const ax = -2 * omega * this.v.x - omega * omega * (x.x - target.x);
    const ay = -2 * omega * this.v.y - omega * omega * (x.y - target.y);
    const az = -2 * omega * this.v.z - omega * omega * (x.z - target.z);
    this.v.x += ax * dt; this.v.y += ay * dt; this.v.z += az * dt;
    x.addScaledVector(this.v, dt);
  }
  reset() { this.v.set(0, 0, 0); }
}

const fmt = (p: number | null | undefined) => (typeof p === 'number' ? p.toFixed(2) : null);

export class Director {
  mode: 'director' | 'free' = 'director';
  private shot: Shot;
  private cands: Cand[] = [];
  private az = 0;
  private look = new Vector3();
  private sPos = new Spring(); private sLook = new Spring();
  private lastUser = -1e9;
  private dragging = false;
  private closeCount = 0;
  private omega: number = CAM.omegaDirector;
  private desired = new Vector3(); private lookTarget = new Vector3();
  /** 每个对象最近一次被拍的时刻：同一对象 45 秒内不重复拍 */
  private shotAt = new Map<string, number>();
  /** 已经出过字幕的“第一个机会”：spotlight 与本地推断的同一时刻只说一次 */
  private firstSaid = new Set<string>();
  private lastMeta = -1e9; private lastGrant = -1e9;
  private lastCandT = -1e9;
  /** 旁白语言：首页 ?lang=en 时为 'en'，默认 'zh'（中文路径输出不变） */
  lang: 'zh' | 'en' = 'zh';
  caption?: (text: string, prio?: number) => void;
  onMode?: (m: 'director' | 'free') => void;
  /** 当前被镜头关注的对象（供信息层或调试用） */
  subject?: string;
  /** ?me=<agent_id>：镜头以它为中心，只追与它有关的时刻 */
  me?: string;
  cruiseOnly = false;

  constructor(private cam: PerspectiveCamera, private controls: OrbitControls, private net: Net, private clock: () => number) {
    this.shot = this.cruise(0);
    controls.addEventListener('start', () => { this.mode = 'free'; this.dragging = true; this.lastUser = this.clock(); this.onMode?.('free'); });
    controls.addEventListener('end', () => { this.dragging = false; this.lastUser = this.clock(); });
    this.look.copy(controls.target);
  }

  /** 镜头尺度跟着群岛实际铺开的范围走（人少时收拢） */
  get D() { return this.net.layout.R; }

  private cruise(now: number): Shot {
    if (this.me) {
      const n = this.net.nodes.get(this.me);
      if (n) return { kind: 'home', prio: 0, start: now, min: 6, max: 14, target: () => n.pos, dist: Math.max(300 * this.net.scale, this.D * 0.62), elev: 50, azSpeed: 2.2, close: false, subject: this.me };
    }
    const c = new Vector3(0, 0, 0);
    return { kind: 'cruise', prio: 0, start: now, min: 6, max: 9, target: () => c, dist: this.D * 1.75, elev: CAM.cruiseElev, azSpeed: CAM.cruiseAzSpeed, close: false };
  }

  /** 静默久了：绕一座真实存在的新港（或机会最多的港）慢慢看一圈，不出字幕，不造事件。 */
  private revisit(now: number): Shot | undefined {
    let best: { p: Vector3; id: string; k: number } | undefined;
    for (const c of this.net.configs.values()) {
      if (!c.port || c.stage === 'dissolved' || now - (this.shotAt.get(c.id) ?? -1e9) < 60) continue;
      const k = c.members.length + (c.plan ? 2 : 0) + Math.random();
      if (!best || k > best.k) best = { p: c.port, id: c.id, k };
    }
    if (!best) return undefined;
    this.shotAt.set(best.id, now);
    const p = best.p;
    return { kind: 'revisit', prio: 0, start: now, min: 8, max: 12, target: () => p, dist: 420 * this.net.scale, elev: 44, azSpeed: 3, close: false, subject: best.id };
  }

  private mine(id: string, to?: string): boolean {
    const me = this.me; if (!me) return true;
    if (id === me || to === me) return true;
    const c = this.net.configs.get(id);
    return !!c && c.members.includes(me);
  }

  /** 网络里发生了值得一看的事。 */
  moment(m: Moment, now: number) {
    if (this.cruiseOnly) return;
    const net = this.net;
    const s = net.scale;
    if (m.kind === 'join') {
      if (!this.mine(m.id)) return;
      if (m.id === this.me) { this.shot = this.cruise(now); }
      // 接入特写有保底：25 秒内没拍过接入，这一次优先级高过方案，可以打断非方案镜头
      const due = now - this.lastJoinShot > 25;
      this.offer({ key: 'join:' + m.id, subject: m.id, prio: due ? 6 : 2, t: now, ttl: 6, make: (t) => {
        const n = net.nodes.get(m.id); if (!n) return undefined;
        this.lastJoinShot = t;
        const en = this.lang === 'en';
        const where = en ? (n.city ? `joined from ${n.city}` : 'joined the network') : (n.city ? `从${n.city}接入` : '接入网络');
        return { kind: 'push', prio: 2, start: t, min: 7, max: 10, target: () => n.pos, dist: 200 * s, elev: 34, azSpeed: 3, close: true, subject: m.id, omega: 2.1,
          onFramed: () => { if (m.id === this.me && net.storyPending) return; this.caption?.(en ? `${n.label}'s ${n.host ?? 'agent'} ${where}` : `${n.label}的 ${n.host ?? 'agent'} ${where}`); } };
      } });
    } else if (m.kind === 'recall') {
      // 网络替 ?me 在人群里召回候选：镜头拉开，让探针从它射向四面八方的候选，再一个个暗下或留下
      if (!this.mine(m.id)) return;
      this.offer({ key: 'recall:' + m.t, subject: m.id + '#recall', prio: 7, t: now, ttl: 6, make: (t) => {
        const me = net.nodes.get(m.id); if (!me) return undefined;
        const cen = new Vector3(); let k = 0;
        for (const id of m.to) { const p = net.nodePos(id); if (p) { cen.add(p); k++; } }
        if (k) cen.multiplyScalar(1 / k);
        const mid = new Vector3().lerpVectors(me.pos, k ? cen : me.pos, 0.55);
        return { kind: 'pull', prio: 7, start: t, min: 7, max: 9, target: () => mid, dist: this.D * 1.25, elev: 58, azSpeed: 4, close: false, subject: m.id + '#recall', omega: 1.5 };
      } });
    } else if (m.kind === 'firstOpp') {
      // ?me 的机会收成亮航线：镜头拉到这条线的两端之间，航线与两头的港一起入画
      if (m.id === this.me && !this.firstSaid.has(m.id + '#shot')) {
        this.firstSaid.add(m.id + '#shot');
        this.offer({ key: 'opp:' + m.other, subject: m.other + '#opp', prio: 7, t: now, ttl: 6, make: (t) => {
          const a = net.nodes.get(m.id), b = net.nodes.get(m.other); if (!a || !b) return undefined;
          const mid = a.pos.clone().lerp(b.pos, 0.5); const span = a.pos.distanceTo(b.pos);
          return { kind: 'push', prio: 7, start: t, min: 6.5, max: 8.5, target: () => mid, dist: Math.max(260 * s, span * 1.5), elev: 46, azSpeed: 3, close: true, subject: m.other + '#opp', omega: 1.6 };
        } });
      }
      // ?me 的 agent 的第一个机会常出在别的镜头里（补信息的镜头正拍着对方）：字幕照出，不等镜头
      if ((this.shot.subject === m.id || m.id === this.me) && this.mode === 'director' && !this.firstSaid.has(m.id)) {
        this.firstSaid.add(m.id);
this.caption?.(this.firstLine(m.dt, m.other, m.form), 2);
        if (this.shot.subject === m.id) this.shot.max = Math.max(this.shot.max, now - this.shot.start + 4);
      }
    } else if (m.kind === 'fleet') {
      if (!this.mine(m.id)) return;
      const c = net.configs.get(m.id); if (!c) return;
      const from = net.centroid(c.members)?.clone(); const to = c.port?.clone();
      if (!from || !to) return;
      const t0 = m.t; const dur = Math.max(0.5, m.born - m.t);
      const tgt = new Vector3();
      this.offer({ key: 'fleet:' + m.id, subject: m.id, prio: 5, t: now, ttl: 2.5, make: (t) => ({
        kind: 'follow', prio: 5, start: t, min: 9, max: 13,
        target: () => { const k = Math.min(1, Math.max(0, (this.clock() - t0) / (dur + 0.2))); const e = k * k * (3 - 2 * k); return tgt.lerpVectors(from, to, e); },
        dist: Math.max(230 * s, from.distanceTo(to) * 2.2 + 120 * s), elev: 40, azSpeed: 5, close: true, subject: m.id,
        onFramed: () => this.caption?.(this.configLine(m.id)),
      }) });
    } else if (m.kind === 'config') {
      if (!this.mine(m.id)) return;
      const c = net.configs.get(m.id); if (!c) return;
      if (m.stage === 'judged' && c.members.length > 2) this.offerConfig(m.id, 3, now, 8, false);
    } else if (m.kind === 'plan') {
      if (!this.mine(m.id)) return;
      if (this.shot.subject === m.id && this.mode === 'director') {
        this.caption?.(this.planLine(m.id));
        this.shot.max = Math.max(this.shot.max, now - this.shot.start + 5);
        // 字幕之后拉远一点，把新港放回周边网络里
        this.pullAfter = now + 3.5;
      }
    } else if (m.kind === 'rejudge') {
      // 正在拍的那一对再判出了新读数：补一行字幕，把「要信息」的结果说完
      const mineRe = !!this.me && (m.a === this.me || m.b === this.me);
      const onShot = this.grantKey === m.key && this.shot.kind === 'push' && now - this.shot.start < this.shot.max;
      if (this.mode === 'director' && (onShot || mineRe)) {
        if (this.lang === 'en') {
          const body = typeof m.p === 'number'
            ? (typeof m.p0 === 'number' ? `re-judged with the extra layer, confidence moved from ${m.p0.toFixed(2)} to ${m.p.toFixed(2)}` : `re-judged with the extra layer, the match holds at confidence ${m.p.toFixed(2)}`)
            : `re-judged with the extra layer, no match${typeof m.pNo === 'number' ? ` (no-match confidence ${m.pNo.toFixed(2)})` : ''}`;
          this.caption?.(mineRe ? `${this.short(m.a === this.me ? m.b : m.a)}: ${body}` : cap1(body), 1);
        } else {
          const who = mineRe ? `与${this.short(m.a === this.me ? m.b : m.a)}，` : '';
          this.caption?.(who + (typeof m.p === 'number'
            ? (typeof m.p0 === 'number' ? `多了这一层再判：置信度 ${m.p0.toFixed(2)} → ${m.p.toFixed(2)}` : `多了这一层再判：关系成立，置信度 ${m.p.toFixed(2)}`)
            : `多了这一层再判：这段关系不成立${typeof m.pNo === 'number' ? `（判「不成立」的把握 ${m.pNo.toFixed(2)}）` : ''}`), 1);
        }
        if (onShot) { this.shot.max = Math.max(this.shot.max, now - this.shot.start + 4); this.grantKey = ''; }
      }
    } else if (m.kind === 'spotlight') {
      if (!this.mine(m.id, m.to)) return;
      switch (m.why) {
        case 'join_first_opp': {
          if (this.firstSaid.has(m.id)) return;
          this.offer({ key: 'first:' + m.id, subject: m.id, prio: 2, t: now, ttl: 12, make: (t) => {
            const n = net.nodes.get(m.id); if (!n) return undefined;
            return { kind: 'push', prio: 2, start: t, min: 6, max: 9, target: () => n.pos, dist: 220 * s, elev: 38, azSpeed: 3.5, close: true, subject: m.id,
              onFramed: () => { if (this.firstSaid.has(m.id)) return; this.firstSaid.add(m.id); const o = this.firstOther(m.id); this.caption?.(this.lang === 'en' ? (o ? `${n.label} found its first opportunity: with ${this.short(o.id)}${o.form === '不成立' ? '' : `, ${formEn(o.form)}`}` : `${n.label} found its first opportunity`) : (o ? `${n.label}找到第一个机会：与${this.short(o.id)}${o.form === '不成立' ? '' : `，${o.form}`}` : `${n.label}找到了第一个机会`)); } };
          } });
          break;
        }
        case 'config_formed': this.offerConfig(m.id, 3, now, 12, true); break;
        case 'meta_formed':
          // 再组合在真数据里很频繁：40 秒内只拍一次
          if (now - this.lastMeta < 40) return;
          this.lastMeta = now; this.offerConfig(m.id, 4, now, 12, true); break;
        case 'plan_ready': {
          if (this.shot.subject === m.id) return; // 跟随船队的镜头会在方案到达时出字
          this.offer({ key: 'plan:' + m.id, subject: m.id, prio: 4, t: now, ttl: 12, make: (t) => {
            const p = net.configPos(m.id); if (!p) return undefined;
            return { kind: 'orbit', prio: 4, start: t, min: 7, max: 10, target: () => p, dist: 240 * s, elev: 40, azSpeed: CAM.orbitAzSpeed, close: true, subject: m.id,
              onFramed: () => this.caption?.(this.planLine(m.id)) };
          } });
          break;
        }
        case 'disclose_granted': {
          // 产品的核心机制：拿不准 → 要信息 → 对方多给一层 → 再判。队里只留最新的一次；
          // 构型成串时它会被挤掉，所以有保底：30 秒没拍过，这一次的优先级高过构型
          const to = m.to;
          const key = to ? net.pairKey(m.id, to) : '';
          const due = now - this.lastGrant > 30;
          this.cands = this.cands.filter((c) => !c.key.startsWith('grant:'));
          this.offer({ key: 'grant:' + key, subject: m.id, prio: due ? 5 : 2, t: now, ttl: 10, make: (t) => {
            this.lastGrant = t;
            const a = net.nodes.get(m.id); if (!a) return undefined;
            const b = to ? net.nodes.get(to) : undefined;
            const mid = b ? a.pos.clone().lerp(b.pos, 0.5) : a.pos;
            const span = b ? a.pos.distanceTo(b.pos) : 0;
            this.grantKey = key;
            return { kind: 'push', prio: 3, start: t, min: 7, max: 10, target: () => mid, dist: Math.max(240 * s, span * 1.6), elev: 40, azSpeed: 3, close: true, subject: m.id,
              onFramed: () => this.caption?.(this.grantLine(m.id, to)) };
          } });
          break;
        }
        default: {
          // 协议外的提示：只当一个普通候选
          this.offer({ key: 'spot:' + m.id, subject: m.id, prio: 1, t: now, ttl: 6, make: (t) => {
            const p = net.nodePos(m.id) ?? net.configPos(m.id); if (!p) return undefined;
            return { kind: 'push', prio: 1, start: t, min: 5, max: 8, target: () => p, dist: 260 * s, elev: 42, azSpeed: 4, close: true, subject: m.id };
          } });
        }
      }
    }
  }
  private pullAfter = -1;
  private grantKey = '';

  /** 补信息被批准的那一刻：谁拿不准哪道题、读数多少、向谁要了什么、对方多给了一层。全部取自这一对的判断簿。 */
  private grantLine(holder: string, asker?: string): string {
    const net = this.net;
    const h = this.short(holder);
    const en = this.lang === 'en';
    if (!asker) return en ? `${h} agreed to share more` : `${h}同意补充信息`;
    const a = this.short(asker);
    const st = net.stories.get(net.pairKey(holder, asker));
    let cat: string | undefined; let tier: number | undefined; let unsure: { q?: string; p?: number | null } | undefined;
    if (st) for (let i = st.steps.length - 1; i >= 0; i--) {
      const x = st.steps[i];
      if (x.k === 'disclose' && x.holder === holder && tier === undefined) tier = x.tier;
      if (x.k === 'reply' && x.granted && x.holder === holder && !cat) cat = x.cat;
      if (x.k === 'unsure' && cat) { unsure = x; break; }
    }
    // 字幕只有一行：请求方的名字和题、要的类别、对方给到哪一层；持有方由镜头交代
    if (en) {
      const q = unsure ? (Q_EN[unsure.q ?? ''] ?? unsure.q) : '';
      const head = unsure ? `${a} was unsure about \u201c${q}\u201d${typeof unsure.p === 'number' ? ` (${unsure.p.toFixed(2)})` : ''} and asked for` : `${a} asked ${h} for`;
      return `${head} ${cat ? `\u201c${cat}\u201d` : 'more information'}; they shared ${typeof tier === 'number' ? `t${tier}` : 'one more layer'}`;
    }
    const head = unsure ? `${a}拿不准「${Q_WORD[unsure.q ?? ''] ?? unsure.q}」${typeof unsure.p === 'number' ? `（${unsure.p.toFixed(2)}）` : ''}，要` : `${a}向${h}要`;
    return `${head}${cat ? `「${cat}」` : '更多信息'}，对方给了${typeof tier === 'number' ? ` t${tier}` : '一层'}`;
  }
  /** 字幕里的短名：agent 用节点标签的整名；新港说成「一个某某构型」 */
  private short(id: string): string {
    const n = this.net.nodes.get(id);
    if (this.lang === 'en') {
      const cid = n?.kind === 'config' ? n.configId ?? id : !n && this.net.configs.has(id) ? id : undefined;
      if (cid !== undefined) { const c = this.net.configs.get(cid); return c ? `a ${shapeEn(c.shape)}` : 'a group'; }
      return this.net.label(id);
    }
    if (n?.kind === 'config') { const c = this.net.configs.get(n.configId ?? id); return c ? `${this.net.shapeName(c.shape)}构型` : '一个构型'; }
    return this.net.label(id); // 节点标签本身已是后端短称的规则（第一分句，最多 12 字），整名放进字幕，不再二次截断
  }
  private lastJoinShot = -1e9;

  /** 「第 N 秒找到第一个机会」一行 */
  private firstLine(dt: number, other: string, form: string): string {
    if (this.lang === 'en') {
      const t = dt < 0.1 ? 'under 0.1 seconds' : dt < 10 ? `${dt.toFixed(1)} seconds` : plural(Math.round(dt), 'second');
      return `First opportunity found in ${t}: with ${this.short(other)}${form === '不成立' ? '' : `, ${formEn(form)}`}`;
    }
    return `${dt < 0.1 ? '不到 0.1 秒就' : dt < 10 ? `${dt.toFixed(1)} 秒就` : `${Math.round(dt)} 秒后`}找到第一个机会：与${this.short(other)}${form === '不成立' ? '' : `，${form}`}`;
  }

  private offerConfig(id: string, prio: number, now: number, ttl: number, caption: boolean) {
    const net = this.net; const s = net.scale;
    this.offer({ key: 'cfg:' + id, subject: id, prio, t: now, ttl, make: (t) => {
      const c = net.configs.get(id); if (!c || c.stage === 'dissolved') return undefined;
      const p = net.configPos(id); if (!p) return undefined;
      let r = 0; for (const m of c.members) { const q = net.nodePos(m); if (q) r = Math.max(r, q.distanceTo(p)); }
      return { kind: 'orbit', prio, start: t, min: 6, max: 9, target: () => p, dist: Math.max(180 * s, r * 2.6), elev: 38, azSpeed: CAM.orbitAzSpeed, close: true, subject: id,
        onFramed: caption ? () => this.caption?.(this.configLine(id)) : undefined };
    } });
  }

  private firstOther(id: string): { id: string; form: string } | undefined {
    const n = this.net.nodes.get(id); if (!n) return undefined;
    let best: { id: string; form: string; t: number } | undefined;
    for (const k of n.edges) {
      const e = this.net.edges.get(k); if (!e || e.state === 'gone') continue;
      if (!best || e.t < best.t) best = { id: e.a === id ? e.b : e.a, form: e.form, t: e.t };
    }
    return best;
  }

  private offer(c: Cand) {
    const now = c.t;
    if (this.cands.some((x) => x.key === c.key)) return;
    if (this.shot.subject === c.subject && this.shot.kind !== 'home') return;
    // 刚拍过的对象不再排队（接入与方案除外）
    if (!c.key.startsWith('join:') && !c.key.startsWith('fleet:') && now - (this.shotAt.get(c.subject) ?? -1e9) < 45) return;
    this.cands.push(c); this.lastCandT = now;
    if (this.cands.length > 24) {
      // 队满：丢掉优先级最低、最旧的
      let worst = 0;
      for (let i = 1; i < this.cands.length; i++) { const a = this.cands[i], w = this.cands[worst]; if (a.prio < w.prio || (a.prio === w.prio && a.t < w.t)) worst = i; }
      this.cands.splice(worst, 1);
    }
  }

  configLine(id: string): string {
    const c = this.net.configs.get(id); if (!c) return '';
    if (this.lang === 'en') {
      const names = c.members.map((m) => this.net.nodes.get(m)?.kind === 'config' ? 'an existing group' : this.short(m));
      const many = names.length > 3;
      const shown = many ? `${names.slice(0, 2).join(', ')} and ${names.length - 2} others` : listEn(names);
      const sh = shapeEn(c.shape);
      const word = c.meta ? 'combine into a larger group' : c.shape === 'pair' ? 'complement each other' : c.shape === 'relay' ? 'form a referral link'
        : many ? `form a ${sh}` : `form a ${names.length}-party ${sh}`;
      const p = fmt(c.conf);
      return `${shown} ${word}${p ? `, confidence ${p}` : ''}`;
    }
    // 字幕只有一行：名字取短名
    const names = c.members.map((m) => this.net.nodes.get(m)?.kind === 'config' ? '一个已成的构型' : this.short(m));
    const many = names.length > 3;
    const shown = many ? `${names.slice(0, 2).join('、')}等 ${names.length} 方` : names.join('、');
    const shapeName = this.net.shapeName(c.shape);
    const shapeWord = c.meta ? '再组合成更大的构型' : c.shape === 'pair' ? '结成互补' : c.shape === 'relay' ? '形成转介'
      : many ? `组成${shapeName}` : `组成 ${names.length} 方${shapeName}`;
    const p = fmt(c.conf);
    return `${shown}${shapeWord}${p ? `，置信度 ${p}` : ''}`;
  }

  planLine(id: string): string {
    const c = this.net.configs.get(id); if (!c) return '';
    const p = fmt(c.plan?.conf ?? c.conf);
    if (this.lang === 'en') {
      if (c.plan?.title) return `Plan ready: ${c.plan.title}${p ? `, confidence ${p}` : ''}`;
      return `${this.configLine(id).replace(/, confidence .*$/, '')}, cooperation plan written${p ? `, confidence ${p}` : ''}`;
    }
    if (c.plan?.title) return `方案落定：${c.plan.title}${p ? `，置信度 ${p}` : ''}`;
    // fixture 判断下生成器不开，方案只有结构没有标题：照实说
    return `${this.configLine(id).replace(/，置信度.*$/, '')}，合作方案写好了${p ? `，置信度 ${p}` : ''}`;
  }

  update(now: number, dt: number) {
    this.cands = this.cands.filter((c) => now - c.t < c.ttl);
    if (this.mode === 'free') {
      if (!this.dragging && now - this.lastUser > CAM.idleReturn) {
        this.mode = 'director'; this.omega = CAM.omegaReturn; this.onMode?.('director');
        this.sPos.reset(); this.sLook.reset(); this.look.copy(this.controls.target);
        this.az = Math.atan2(this.cam.position.z - this.look.z, this.cam.position.x - this.look.x);
        this.shot = this.cruise(now);
      } else { this.controls.update(); return; }
    }
    // ?me 的 agent 晚于页面接入：一出现就把基准镜头换成以它为中心
    if (this.me && this.shot.kind === 'cruise' && this.net.nodes.has(this.me)) this.shot = this.cruise(now);
    const age = now - this.shot.start;
    // 选下一个镜头
    let best: Cand | undefined;
    for (const c of this.cands) if (!best || c.prio > best.prio || (c.prio === best.prio && c.t > best.t)) best = c;
    const base = this.shot.kind === 'cruise' || this.shot.kind === 'home' || this.shot.kind === 'revisit';
    const urgent = !!best && best.prio >= 6 && (this.shot.kind !== 'follow' || age >= 6);
    const canSwitch = urgent || age >= this.shot.min || (best && best.prio > this.shot.prio + 1 && base);
    const maxClose = this.me ? 1 : 2;
    if ((canSwitch && best && (best.prio > this.shot.prio || age >= this.shot.min)) || age >= this.shot.max) {
      let next: Shot | undefined;
      if (!best || (this.closeCount >= maxClose && !urgent)) {
        if (base && age < this.shot.max) next = undefined;
        else {
          // 队空且已巡航过一轮：绕一座真实存在的港；否则回到巡航（?me 时回到它身边）
          const quiet = now - this.lastCandT > 16;
          next = (!this.me && quiet && this.shot.kind === 'cruise') ? this.revisit(now) ?? this.cruise(now) : this.cruise(now);
        }
        if (next) this.closeCount = 0;
      } else {
        next = best.make(now); this.cands = this.cands.filter((c) => c !== best);
        if (next) { this.closeCount++; if (next.subject) this.shotAt.set(next.subject, now); }
        else if (now - best.t < best.ttl) this.cands.push(best); // 对象还没就位（比如构型事件晚到）：留到期满再试
      }
      if (next) {
        // 从当前方位角继续转，不甩镜头
        const tg = next.target();
        if (tg) this.az = Math.atan2(this.cam.position.z - tg.z, this.cam.position.x - tg.x);
        this.shot = next; this.omega = next.omega ?? CAM.omegaDirector;
      }
    }
    if (this.pullAfter > 0 && now > this.pullAfter) { this.shot.dist *= 1.6; this.shot.elev = 46; this.pullAfter = -1; }
    const tg = this.shot.target() ?? this.lookTarget;
    this.subject = this.shot.subject;
    this.az += (this.shot.azSpeed * Math.PI / 180) * dt;
    const el = this.shot.elev * Math.PI / 180;
    // 竖屏时水平视野窄，镜头整体退后一些
    const dist = this.shot.dist * Math.pow(Math.min(2.2, Math.max(1, 1.78 / this.cam.aspect)), 0.7);
    this.desired.set(
      tg.x + Math.cos(this.az) * Math.cos(el) * dist,
      tg.y + Math.sin(el) * dist,
      tg.z + Math.sin(this.az) * Math.cos(el) * dist,
    );
    this.lookTarget.copy(tg);
    const sub = Math.min(dt, 0.05);
    this.sPos.step(this.cam.position, this.desired, this.omega, sub);
    this.sLook.step(this.look, this.lookTarget, this.omega * 1.8, sub);
    this.cam.lookAt(this.look);
    this.controls.target.copy(this.look);
    if (!this.shot.framed && this.shot.onFramed && this.cam.position.distanceTo(this.desired) < dist * 0.35) {
      this.shot.framed = true; this.shot.onFramed();
    }
  }

  /** 首帧：把机位直接放在巡航位置的高处，开场从高空推入。 */
  place() {
    this.az = 0.6;
    this.cam.position.set(Math.cos(this.az) * this.D * 1.2, this.D * 2.4, Math.sin(this.az) * this.D * 1.2);
    this.look.set(0, 0, 0);
    this.cam.lookAt(this.look);
  }
}
