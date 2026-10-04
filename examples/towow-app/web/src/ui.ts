// 文字：一行字幕、左下角一行统计、点击后的侧面信息层。文字极少，只在关键时刻出现。
// 信息层的“置信度”= 决定这个机会的那一道题的读数（宿主 edge.decisive / config.hold），题面随行。
// 连着真后端时从 /api/opportunities 取（与 agent 主人经 MCP 看到的是同一份渲染）；模拟与回放从事件累积。
import type { Net, NodeRec, ConfigRec, Story, StoryStep } from './net';
import { shortLabel } from './net';
import type { SrcKind } from './source';
import { SHAPE_NAME, TIER_NAMES, MAX_TIER, Q_TEXT, Q_WORD, VALUE_LEVELS, type Shape } from './protocol';
import { T } from './style';

const STATUS: Record<string, string> = {
  sent: '已请求', granted: '已补上', denied: '被拒', escalate: '升级判断', drop: '放下', return: '退回调用方',
  near_boundary: '读数贴着判断线', refine: '细化后再判',
};
const SRC_WORD: Record<SrcKind, string> = { live: '实时', mock: '模拟数据', replay: '回放', probing: '正在连接' };
const SHAPE_PRI: Record<string, number> = { meta: 0, m2m: 1, team: 2, ring: 3, chain: 4, star: 5, relay: 6, pair: 7 };
/** 离线跑时生成器没开，方案只有占位串：照实说，不把哈希显示出来 */
const isPlaceholder = (s: string | null | undefined) => !s || /^（离线生成占位/.test(s);

/** 信息层与 ?me 侧栏共用的一项机会，全部从事件累积 */
interface Opp {
  id: string; jump: string; cfg: boolean; title: string; who: string; conf: number | null;
  q?: string; qText?: string; seen?: string; story?: string; steps?: { c: string; h: string }[]; lacks: string[]; plan?: string; value: number | null; pri: number; role?: string;
}

/** /api/opportunities 的一项（host/views.opportunities） */
interface ApiConf { kind?: string; p?: number; conf?: number; q?: string; grade?: string; cause?: unknown; needed?: unknown }
interface ApiOpp {
  id: string; shape: string; status?: string; with: { id: string; display?: string; kind?: string }[];
  form?: string; direction?: string; confidence?: ApiConf | null; value?: string | null; timing?: ApiConf | null;
  tier_seen?: string[] | null; lacks?: unknown[]; pending?: unknown[]; my_role?: string | null; roles?: Record<string, string>;
  plan?: { title?: string | null; summary?: string | null; steps?: unknown } | null;
}
interface ApiOpps { agent_id: string; n?: number; in_progress?: number; opportunities?: ApiOpp[]; error?: string }

export class UI {
  private cap = document.getElementById('caption')!;
  private stat = document.getElementById('stat')!;
  private panel = document.getElementById('panel')!;
  private capTimer = 0;
  private capQueue: { text: string; prio: number }[] = [];
  private capBusyUntil = 0;
  openId: string | null = null;
  srcKind: SrcKind = 'probing';
  connected = true;
  /** 数据出处（实时 / 回放（真机数据）/ 模拟数据 …）与回放倍速 */
  origin?: string;
  /** 首页英文版的左下角状态行（旁白的英文在 director.ts、net.ts，见 captions.ts） */
  lang: 'zh' | 'en' = 'zh';
  speed = 1;
  /** 真后端 HTTP 根；undefined 表示没有后端（模拟、回放） */
  api?: string;
  /** ?me=<agent_id> */
  me?: string;
  onClose?: () => void;
  onJump?: (id: string) => void;
  private opps = new Map<string, { t: number; data?: ApiOpps; err?: boolean; inflight?: boolean }>();
  private fetchTimer = new Map<string, number>();

  constructor(private net: Net) {
    this.panel.addEventListener('click', (e) => {
      const t = (e.target as HTMLElement).closest('[data-id]') as HTMLElement | null;
      if (t?.dataset.id) { e.stopPropagation(); this.open(t.dataset.id); this.onJump?.(t.dataset.id); }
    });
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape') this.close(); });
  }

  /** prio 1 是叙述（召回、放下、再判），2 是第一个机会：高的不被低的顶掉，同级后来者顶掉待播的 */
  caption(text: string, prio = 0) {
    if (!text) return;
    text = spaceMixed(text);
    const now = performance.now() / 1000;
    if (now < this.capBusyUntil) { // 只保留一条待播：优先级不低于它的新字幕才顶掉它
      const q = this.capQueue[0];
      if (!q || prio >= q.prio) this.capQueue = [{ text, prio }];
      return;
    }
    this.show(text);
  }
  private show(text: string) {
    const now = performance.now() / 1000;
    this.cap.textContent = text;
    this.cap.classList.remove('out'); void this.cap.offsetWidth; this.cap.classList.add('in');
    this.capBusyUntil = now + T.captionIn + T.captionHold;
    clearTimeout(this.capTimer);
    this.capTimer = window.setTimeout(() => {
      this.cap.classList.remove('in'); this.cap.classList.add('out');
      window.setTimeout(() => { const nx = this.capQueue.shift(); if (nx) this.show(nx.text); }, T.captionOut * 1000);
    }, (T.captionIn + T.captionHold) * 1000);
  }

  /** 左下角一行：数据来源 + 规模。真后端的 stats 很稀，所以按秒从前端状态重算。 */
  stats(now: number) {
    const s = this.net.stats;
    const perMin = Math.round(typeof s.qps === 'number' && this.srcKind === 'mock' ? s.qps * 60 : this.net.judgesLastMinute(now)).toLocaleString('zh-CN');
    if (this.lang === 'en') {
      if (this.srcKind === 'probing') { this.stat.textContent = 'Connecting to the network'; return; }
      let w = { live: 'Live', mock: 'Mock data', replay: 'Replay', probing: '' }[this.srcKind];
      if (this.srcKind === 'live' && !this.connected) w = 'Live, reconnecting';
      if (this.srcKind === 'replay' && this.speed !== 1) w = `${w} at ${this.speed}x`;
      this.stat.textContent = `${w}. ${this.net.agentCount().toLocaleString('en')} agents, ${perMin} judgments a minute, ${this.net.configCount().toLocaleString('en')} groups`;
      return;
    }
    let src = this.origin ?? SRC_WORD[this.srcKind];
    if (this.srcKind === 'live' && !this.connected) src = '实时，重连中';
    if (this.srcKind === 'replay' && this.speed !== 1) src = `${src}。${this.speed} 倍速`;
    if (this.srcKind === 'probing') { this.stat.textContent = '正在连接网络……'; return; }
    this.stat.textContent = `${src}。${this.net.agentCount().toLocaleString('zh-CN')} 个 agent，每分钟 ${perMin} 次判断，${this.net.configCount().toLocaleString('zh-CN')} 个构型`;
  }

  open(id: string) {
    const n = this.net.nodes.get(id); if (!n && id !== this.me) return;
    this.openId = id;
    this.render(); this.want(id);
    this.panel.classList.add('open'); document.body.classList.add('panel-open');
  }
  close() {
    // ?me 时侧栏常驻：关掉别人的信息层就回到自己的机会列表
    if (this.me) { if (this.openId !== this.me) { this.open(this.me); this.onClose?.(); } return; }
    if (!this.openId) return; this.openId = null; this.panel.classList.remove('open'); document.body.classList.remove('panel-open'); this.onClose?.();
  }
  refresh(id: string) {
    if (this.openId === id) { this.renderSoon(); this.want(id, true); }
    // 构型的信息层读的是成员的机会列表
    else if (this.openId) { const c = this.cfgOf(this.openId); if (c && (c.members.includes(id) || c.id === id)) this.renderSoon(); }
  }
  /** ?me 的 agent 被某条事件牵动 */
  mine() { if (!this.me) return; this.want(this.me, true); if (this.openId === this.me) this.renderSoon(); }
  private pend = 0;
  private renderSoon() { if (this.pend) return; this.pend = window.setTimeout(() => { this.pend = 0; this.render(); }, 250); }

  // ---------- 拉宿主的机会列表：只在信息层打开时拉，事件牵动时防抖重拉，不轮询 ----------
  private want(id: string, changed = false) {
    if (this.api === undefined) return;
    const agent = this.agentFor(id); if (!agent) return;
    const rec = this.opps.get(agent);
    const now = performance.now() / 1000;
    if (rec && !changed && now - rec.t < 15) return;
    if (this.fetchTimer.has(agent)) return;
    const wait = rec ? Math.max(1.5, 3 - (now - rec.t)) : 0;
    this.fetchTimer.set(agent, window.setTimeout(() => { this.fetchTimer.delete(agent); this.fetchOpps(agent); }, wait * 1000));
  }
  private async fetchOpps(agent: string) {
    const rec = this.opps.get(agent) ?? { t: 0 };
    if (rec.inflight) return;
    rec.inflight = true; this.opps.set(agent, rec);
    try {
      const r = await fetch(`${this.api}/api/opportunities/${encodeURIComponent(agent)}`);
      if (!r.ok) throw new Error(String(r.status));
      rec.data = await r.json() as ApiOpps; rec.err = false;
    } catch { rec.err = true; }
    rec.t = performance.now() / 1000; rec.inflight = false;
    this.render();
  }
  /** 一座港的信息层要读哪位 agent 的机会：agent 就是它自己；构型取第一位 agent 成员（嵌套构型往里找）。 */
  private agentFor(id: string, depth = 0): string | undefined {
    const n = this.net.nodes.get(id);
    if (!n && id === this.me) return id;
    if (!n) return undefined;
    if (n.kind === 'agent') return id;
    const c = this.cfgOf(id); if (!c || depth > 6) return undefined;
    for (const m of c.members) if (this.net.nodes.get(m)?.kind === 'agent') return m;
    for (const m of c.members) { const a = this.agentFor(m, depth + 1); if (a) return a; }
    return undefined;
  }
  private cfgOf(nodeId: string): ConfigRec | undefined {
    const n = this.net.nodes.get(nodeId);
    return this.net.configs.get(n?.configId ?? nodeId);
  }

  private render() {
    if (!this.openId) return;
    const n = this.net.nodes.get(this.openId);
    if (!n) {
      this.panel.innerHTML = `<p class="mine">我的 agent</p><h2>${esc(this.openId)}</h2><p class="sub">还没有接入这张网络。接入后，这里会常驻它的机会。</p>`;
      return;
    }
    const back = this.me && this.openId !== this.me ? `<p class="back" data-id="${esc(this.me)}">回到我的机会</p>` : '';
    this.panel.innerHTML = back + (n.kind === 'config' ? this.configHtml(n) : this.agentHtml(n));
  }

  private name(id: string) { return esc(this.net.label(id)); }
  /** 角色文字里夹着 agent id（如「中段1→a0371」）：换成名字 */
  private roleText(r: string | undefined): string | undefined {
    if (!r) return r;
    return r.replace(/\b(a\d{3,6})\b/g, (id) => (this.net.nodes.has(id) ? this.net.label(id) : id)).replace(/\d*→/g, '，交给');
  }

  private agentHtml(n: NodeRec): string {
    const where = [n.host ? `${esc(n.host)} 托管` : '', n.city ? esc(n.city) : ''].filter(Boolean).join('，');
    const desc = n.full && n.full !== n.label ? `<p class="desc">${esc(n.full)}</p>` : '';
    const mine = this.me === n.id ? '<p class="mine">我的 agent</p>' : '';
    const rec = this.api !== undefined ? this.opps.get(n.id) : undefined;
    let body: string;
    if (rec?.data?.opportunities) body = this.apiOppsHtml(n, rec.data);
    else body = this.eventOppsHtml(n) + (rec?.err ? '<p class="more">没取到宿主的机会列表，以上是画面里累积的读数。</p>' : '');
    return `
      ${mine}<h2>${esc(n.label)}</h2>
      <p class="sub">${where}${n.alive ? '' : (where ? '，' : '') + '已离开'}</p>
      ${desc}
      <h3>披露</h3>
      ${ladder(n.tier)}
      <h3>机会</h3>
      ${body}
    `;
  }

  /** 宿主渲染的机会：顺序照宿主（按价值档位，不按置信度——不同题的读数不能互比）。 */
  private apiOppsHtml(n: NodeRec, d: ApiOpps): string {
    const items = d.opportunities ?? [];
    if (!items.length) return `<p class="empty">${d.in_progress ? `有 ${d.in_progress} 个机会正在判断。` : '还在被判断，暂时没有成形的机会。'}</p>`;
    const shown = items.slice(0, 10).map((o) => {
      const p = o.confidence?.p ?? o.confidence?.conf;
      const others = o.with.map((w) => esc(this.net.nodes.get(w.id)?.label ?? shortLabel(w.display) ?? w.id)).join('、');
      const title = o.shape === 'pair' || o.shape === 'relay' ? (o.form ?? SHAPE_NAME[o.shape as Shape]) : `${o.with.length + 1} 方${SHAPE_NAME[o.shape as Shape] ?? o.shape}`;
      const unsure = o.confidence?.kind === 'unsure';
      const jump = this.net.nodes.has(o.id) ? o.id : o.with[0]?.id ?? '';
      const lacks = [...(o.lacks ?? []), ...(o.pending ?? [])].map(txt).filter(Boolean).slice(0, 2);
      const seen = o.tier_seen?.length ? `这一对已释放的层（双方合计）：${o.tier_seen.map(tierName).join('、')}` : '';
      const meta = [o.my_role ? `我在里面是${esc(o.my_role)}` : '', o.direction && o.direction !== '方向未定' ? esc(o.direction) : '', o.value ? `价值${esc(o.value)}` : ''].filter(Boolean).join('，');
      return `
        <div class="opp" data-id="${esc(jump)}">
          <div class="row"><span class="form">${esc(title)}</span><span class="conf${unsure ? ' unsure' : ''}">${typeof p === 'number' ? p.toFixed(2) : unsure ? '拿不准' : '—'}</span></div>
          <div class="who">${others}</div>
          ${typeof p === 'number' ? `<div class="bar"><i style="width:${Math.round(p * 100)}%"></i></div>` : ''}
          ${o.confidence?.q ? `<div class="q">${esc(o.confidence.q)}</div>` : ''}
          ${seen || meta ? `<div class="seen">${[seen, meta].filter(Boolean).join('；')}</div>` : ''}
          ${lacks.length ? `<div class="miss">还缺：${lacks.map(esc).join('；')}</div>` : ''}
          ${o.plan?.title ? `<div class="plan-line">方案：${esc(o.plan.title)}</div>` : o.plan ? '<div class="seen">方案已写好</div>' : ''}
        </div>`;
    }).join('');
    return `<p class="note">置信度是决定这个机会的那一道题的读数，题面写在下面；不同题的读数不互相比较。</p>${shown}
      ${items.length > 10 ? `<p class="more">另有 ${items.length - 10} 个机会</p>` : ''}
      ${d.in_progress ? `<p class="more">另有 ${d.in_progress} 个正在判断</p>` : ''}`;
  }

  /** 模拟与回放：从事件累积。置信度 = 决定性那道题的读数，题面、已看到的层、补信息的经过和还缺什么都随行。 */
  private eventOppsHtml(n: NodeRec): string {
    const opps = this.oppsOf(n);
    const max = this.me === n.id ? 12 : 8;
    const lost = [...n.edges].filter((k) => this.net.edges.get(k)?.state === 'gone').length;
    const cs = this.me === n.id && this.net.cands.size ? this.net.candStats() : null;
    const tally = cs ? `<p class="tally">网络替我召回了 ${cs.total} 个候选：<b>${cs.held}</b> 个成了机会，<em>${cs.asking}</em> 个在要信息，${cs.dropped} 个判了「不成立」</p>` : '';
    return `${tally}${opps.length ? '<p class="note">置信度是决定这个机会的那一道题的读数；不同题的读数不互相比较。</p>' : ''}
      ${opps.length ? opps.slice(0, max).map((o) => this.oppHtml(o)).join('') : '<p class="empty">还在被判断，暂时没有成形的机会。</p>'}
      ${opps.length > max ? `<p class="more">另有 ${opps.length - max} 个机会</p>` : ''}
      ${lost ? `<p class="more">${lost} 条关系被否掉，留在海面上的灰痕里</p>` : ''}`;
  }

  private oppHtml(o: Opp): string {
    const p = o.conf;
    return `
        <div class="opp" data-id="${esc(o.jump)}">
          <div class="row"><span class="form">${esc(o.title)}</span><span class="conf">${p === null ? '—' : p.toFixed(2)}</span></div>
          <div class="who">${o.who}${o.role ? `，在里面是${esc(o.role)}` : ''}</div>
          ${p === null ? '' : `<div class="bar"><i style="width:${Math.round(p * 100)}%"></i></div>`}
          ${o.qText ? `<div class="q">${esc(o.qText)}</div>` : ''}
          ${o.seen ? `<div class="seen">${o.seen}</div>` : ''}
          ${o.steps?.length ? `<ol class="steps">${o.steps.map((x) => `<li class="${x.c}">${x.h}</li>`).join('')}</ol>` : o.story ? `<div class="story">${o.story}</div>` : ''}
          ${o.lacks.length ? `<div class="miss">还缺：${o.lacks.map(esc).join('；')}</div>` : ''}
          ${o.plan ? `<div class="plan-line">${esc(o.plan)}</div>` : ''}
        </div>`;
  }

  /** 一位 agent 的全部机会：构型在前，再按价值档位、形状、id（与宿主 views.opportunities 同一排法）。 */
  private oppsOf(n: NodeRec): Opp[] {
    const net = this.net; const out: Opp[] = [];
    for (const cid of n.configs) {
      const c = net.configs.get(cid); if (!c || c.stage === 'dissolved') continue;
      const others = c.members.filter((m) => m !== n.id);
      const st = net.stories.get(c.id);
      out.push({
        id: c.id, jump: c.nodeId ?? others[0] ?? n.id, cfg: !(c.shape === 'pair' || c.shape === 'relay'), title: shapeTitle(c),
        who: others.map((o) => this.name(o)).join('、'), conf: c.conf, q: 'hold', qText: Q_TEXT.hold,
        steps: st ? this.storySteps(st, n.id, 5) : undefined, lacks: st ? lacksOf(st) : [],
        plan: c.plan ? (c.plan.title ? `方案：${c.plan.title}` : '方案已写好') : undefined,
        value: st ? net.valueOf(st) : null, pri: SHAPE_PRI[c.meta ? 'meta' : c.shape] ?? 9, role: this.roleText(c.roles[n.id]),
      });
    }
    for (const k of n.edges) {
      const e = net.edges.get(k); if (!e || e.state === 'gone') continue;
      const o = e.a === n.id ? e.b : e.a;
      const st = net.stories.get(net.pairKey(n.id, o));
      const theirs = net.nodes.get(o)?.seen.get(n.id) ?? 0;   // 对方向这边解锁到的层
      const mine = n.seen.get(o) ?? 0;                        // 这边向对方解锁到的层
      const seen = st?.tierSeen?.length ? `这一对已释放的层（双方合计）：${st.tierSeen.map(tierName).join('、')}` : `看到对方的 ${tiers(theirs)}；对方看到这边的 ${tiers(mine)}`;
      out.push({
        id: k, jump: o, cfg: false, title: e.form === '不成立' ? '形式题判「不成立」' : e.form, who: this.name(o), conf: e.conf,
        q: st?.dq, qText: st?.dq ? (st.dqText ? this.fixNames(st.dqText, st) : this.qText(st, st.dq)) : undefined,
        seen, steps: st ? this.storySteps(st, n.id, this.me === n.id ? 7 : 5) : undefined, lacks: st ? (st.hostLacks ?? lacksOf(st)) : [],
        value: st ? net.valueOf(st) : null, pri: SHAPE_PRI[/^经/.test(e.form) ? 'relay' : 'pair'],
      });
    }
    out.sort((a, b) => (Number(b.cfg) - Number(a.cfg)) || ((b.value ?? -1) - (a.value ?? -1)) || (a.pri - b.pri) || (a.id < b.id ? -1 : 1));
    for (const o of out) if (typeof o.value === 'number' && VALUE_LEVELS[o.value]) o.title += `，价值${VALUE_LEVELS[o.value]}`;
    return out;
  }

  /** 决定性题的题面：拿不准时去要信息的请求里带着这一对的原题（purpose），题短名对得上就用原题，A、B 换成名字。 */
  private qText(st: Story, q: string): string {
    if (q === 'catcher' || q === 'open') {
      for (let i = st.steps.length - 1; i >= 0; i--) {
        const s = st.steps[i];
        if (s.k === 'ask' && s.text && qOfPurpose(s.text) === q) return this.subAB(s.text, st);
      }
    }
    return Q_TEXT[q] ?? q;
  }
  /** 题面里的人名是后端短称，与画面上的节点标签可能对不上：两个名字里有一个正是这一对里某个节点的标签，另一个就是剩下那个节点，改成它的标签；猜不出来就保持原样。 */
  private fixNames(t: string, st: Story): string {
    const la = this.net.label(st.ka ?? st.a), lb = this.net.label(st.kb ?? st.b);
    const m = /^读 (.+?) 的世界，.+?（或 .+? 身边的人）能对 (.+?) 现在/.exec(t); if (!m) return t;
    const X = m[1], Y = m[2]; const known = [la, lb];
    const xi = known.indexOf(X), yi = known.indexOf(Y);
    let rx = X, ry = Y;
    if (xi >= 0 && yi < 0) ry = known[1 - xi]; else if (yi >= 0 && xi < 0) rx = known[1 - yi];
    if (rx === X && ry === Y) return t;
    return t.split(X).join('\u0001').split(Y).join('\u0002').split('\u0001').join(rx).split('\u0002').join(ry);
  }
  private subAB(t: string, st: Story) {
    if (!st.ka || !st.kb) return t;
    const A = this.net.label(st.ka), B = this.net.label(st.kb);
    return t.replace(/(^|[^A-Za-z])A(?=[^A-Za-z]|$)/g, (_m, p: string) => p + A).replace(/(^|[^A-Za-z])B(?=[^A-Za-z]|$)/g, (_m, p: string) => p + B);
  }

  /** 「拿不准 → 要信息 → 披露 → 再判」的经过，压成一行。me 是读这行的那一方。 */
  storyLine(st: Story, me?: string, max = 5): string {
    const { frags, more } = this.storyFrags(st, me, max);
    return frags.length ? (more ? '…→ ' : '') + frags.join(' → ') : '';
  }
  /** 同一份经过拆成一步一行，侧栏里画成竖向的小时间线；c：u 朱（拿不准）、b 钠灯（多给一层与再判）、m 略去 */
  storySteps(st: Story, me?: string, max = 5): { c: string; h: string }[] {
    const { frags, more } = this.storyFrags(st, me, max);
    const out = frags.map((h) => ({ c: h.startsWith('<em>') ? 'u' : h.startsWith('<b>') ? 'b' : '', h: h.replace(/<\/?(em|b)>/g, '') }));
    if (more) out.unshift({ c: 'm', h: '更早的几步略去' });
    return out;
  }
  private storyFrags(st: Story, me: string | undefined, max: number): { frags: string[]; more: boolean } {
    // 两方的一对用「这边 / 对方」；构型人多，用短名
    const pair = !this.net.configs.has(st.key);
    const short = (id: string) => { const l = this.net.label(id); return esc(l.length > 7 ? l.slice(0, 6) + '…' : l); };
    const who = (id?: string) => (me && pair ? (id === me ? '这边' : '对方') : short(id ?? ''));
    const frags: string[] = [];
    const steps = st.steps;
    // 连续几次披露并成一句：谁多给了、到了哪几层
    let dis: { holders: string[]; tiers: number[] } | null = null;
    const flush = () => {
      if (!dis) return;
      const hs = [...new Set(dis.holders)]; const ts = [...new Set(dis.tiers)].sort();
      const whoTxt = me && pair && hs.length > 1 ? '双方' : hs.map(who).join('、');
      frags.push(`<b>${whoTxt}多给了一层（${ts.map((t) => 't' + t).join('、')}）</b>`);
      dis = null;
    };
    for (let i = 0; i < steps.length; i++) {
      const s: StoryStep = steps[i]; let f = '';
      if (s.k === 'disclose') { dis = dis ?? { holders: [], tiers: [] }; dis.holders.push(s.holder ?? ''); dis.tiers.push(s.tier ?? 1); continue; }
      flush();
      switch (s.k) {
        case 'unsure': {
          const w = Q_WORD[s.q ?? ''] ?? s.q ?? '';
          const p = typeof s.p === 'number' ? ` ${s.p.toFixed(2)}` : '';
          f = s.route === 'near_boundary' ? `「${w}」读数贴着判断线${p}` : s.route === 'refine' ? (steps[i + 1]?.k === 'rejudge' ? `「${w}」细化后再判` : '') : `<em>拿不准「${w}」${p}</em>`;
          break;
        }
        case 'ask': {
          const nx = steps[i + 1];
          if (nx && nx.k === 'ask' && nx.cat === s.cat && nx.holder === s.asker) { f = `双方互要「${esc(s.cat ?? '')}」`; i++; }
          else f = me && s.holder === me ? `对方来要「${esc(s.cat ?? '')}」` : `向${who(s.holder)}要「${esc(s.cat ?? '')}」`;
          break;
        }
        case 'reply': {
          const nx = steps[i + 1];
          if (s.granted && nx && nx.k === 'disclose' && nx.holder === s.holder) break; // 紧接着的披露会说
          f = s.granted ? `${who(s.holder)}给了「${esc(s.cat ?? '')}」` : `${who(s.holder)}没给「${esc(s.cat ?? '')}」`;
          break;
        }
        case 'rejudge': f = typeof s.p === 'number'
          ? `<b>再判 ${typeof s.p0 === 'number' ? s.p0.toFixed(2) + ' → ' : '成立，'}${s.p.toFixed(2)}</b>`
          : `再判后不再成立${typeof s.pNo === 'number' ? `（判「不成立」的把握 ${s.pNo.toFixed(2)}）` : ''}`; break;
      }
      if (f && frags[frags.length - 1] !== f) frags.push(f);
    }
    flush();
    return { frags: frags.slice(-max), more: frags.length > max };
  }

  private configHtml(n: NodeRec): string {
    const c = this.cfgOf(n.id);
    if (!c) return `<h2>${esc(n.label)}</h2><p class="sub">构型节点</p>`;
    const agent = this.agentFor(n.id);
    const api = agent ? this.opps.get(agent)?.data?.opportunities?.find((o) => o.id === c.id) : undefined;
    const p = api?.confidence?.p ?? api?.confidence?.conf ?? c.conf;
    const members = c.members.map((m) => `<div class="mem" data-id="${esc(m)}"><span>${this.name(m)}</span><span class="role">${esc(this.roleText(c.roles[m] || api?.roles?.[m]) ?? '')}</span></div>`).join('');
    const st = this.net.stories.get(c.id);
    const lacks = api ? [...(api.lacks ?? []), ...(api.pending ?? [])].map(txt).filter(Boolean).slice(0, 3) : [];
    const missing = api ? lacks.map((x) => esc(x)) : (st ? lacksOf(st).map(esc) : []);
    const plan = api?.plan ?? c.plan;
    const summary = plan?.summary && !isPlaceholder(plan.summary) ? plan.summary : null;
    const planHtml = plan?.title || summary
      ? `<h3>方案</h3>${plan?.title ? `<p class="plan-title">${esc(plan.title)}</p>` : ''}${summary ? `<p class="plan">${esc(summary)}</p>` : ''}`
      : plan ? '<h3>方案</h3><p class="empty">方案已写好；这次运行没有开生成器，所以没有标题和正文。</p>' : '<p class="empty">方案还在写。</p>';
    const story = st ? this.storyLine(st, undefined, 6) : '';
    return `
      <h2>${esc(n.label)}</h2>
      <p class="sub">${shapeTitle(c)}${typeof p === 'number' ? `，置信度 ${p.toFixed(2)}` : ''}</p>
      ${typeof p === 'number' ? `<div class="bar"><i style="width:${Math.round(p * 100)}%"></i></div>` : ''}
      <p class="q">${esc(api?.confidence?.q ?? Q_TEXT.hold)}</p>
      ${story ? `<p class="story">${story}</p>` : ''}
      ${planHtml}
      <h3>成员</h3>
      ${members}
      ${missing.length ? `<h3>还缺</h3>${missing.map((m) => `<p class="miss">${m}</p>`).join('')}` : ''}
      <p class="more">这座港本身也是一个节点，可以再和别的港组成更大的构型。</p>
    `;
  }

  private missingFor(n: NodeRec, others: string[]): string {
    const out: string[] = [];
    for (const o of others) {
      const m = n.missing.get(o); if (m) out.push(`还缺：${esc(m.cat)}（${STATUS[m.status] ?? m.status}）`);
      const om = this.net.nodes.get(o)?.missing.get(n.id); if (om) out.push(`对方还缺：${esc(om.cat)}（${STATUS[om.status] ?? om.status}）`);
    }
    return out.slice(0, 2).join('<br>');
  }
}

function txt(x: unknown): string {
  if (typeof x === 'string') return x;
  if (x && typeof x === 'object') { const o = x as Record<string, unknown>; return String(o.needed ?? o.cause ?? o.cat ?? o.category ?? ''); }
  return '';
}
function tierName(t: string) { const i = Number(String(t).replace(/^t/, '')); return TIER_NAMES[i] ? `${t} ${TIER_NAMES[i]}` : t; }

function shapeTitle(c: ConfigRec) {
  const k = c.members.length;
  if (c.meta) return `${k} 方再组合`;
  if (c.shape === 'pair') return '互补';
  if (c.shape === 'relay') return '转介';
  return `${k} 方${SHAPE_NAME[c.shape] ?? '构型'}`;
}

function ladder(tier: number): string {
  // 三级台阶 t0/t1/t2：已到的层实线，未到的层虚线；每级右侧写这一层给出的是什么
  const w = 300, h = 88, sw = 52, sh = 26;
  let p = ''; let labels = '';
  for (let i = 0; i <= MAX_TIER; i++) {
    const x = 4 + i * sw, y = h - 10 - i * sh;
    const on = i <= tier;
    p += `<line x1="${x}" y1="${y}" x2="${x + sw}" y2="${y}" class="${on ? 'on' : 'off'}"/>`;
    if (i > 0) p += `<line x1="${x}" y1="${y}" x2="${x}" y2="${y + sh}" class="${on ? 'on' : 'off'}"/>`;
    labels += `<div class="tl ${on ? 'on' : ''}" style="left:${x + sw + 10}px;top:${y - 9}px">${TIER_NAMES[i]}</div>`;
  }
  return `<div class="ladder"><svg viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" aria-hidden="true">${p}</svg>${labels}</div>`;
}

function esc(s: string) { return String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]!)); }

/** 中文与拉丁字母、数字之间加空格。 */
function spaceMixed(s: string) {
  return s.replace(/([一-鿿])([A-Za-z0-9])/g, '$1 $2').replace(/([A-Za-z0-9])([一-鿿])/g, '$1 $2');
}

function tiers(t: number) { return Array.from({ length: Math.max(0, Math.min(MAX_TIER, t)) + 1 }, (_, i) => `t${i}`).join('+'); }
function lacksOf(st: Story): string[] {
  const out: string[] = [];
  for (const [cat, status] of st.lacks) if (status !== 'granted' && !/^不缺信息/.test(cat)) out.push(`${cat}（${STATUS[status] ?? status}）`);
  return out.slice(-3);
}
function qOfPurpose(t: string) { return /^读 [AB] 的世界/.test(t) ? 'open' : /^把 [AB] 当作来信的人/.test(t) ? 'catcher' : /按这个形状合作/.test(t) ? 'hold' : ''; }
