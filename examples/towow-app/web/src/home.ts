// 首页模式（towow.ai、net.towow.ai，或 ?home=1）：全屏 3D 网络上压一层极少的文字——一句话、一行接入命令、实时与回放的切换。
// 规范见 DESIGN.md §9。网里有动静（最近 5 分钟有判断，或有真人在）就放实时（/live，公开画面），
// 安静时先放 10-04 的 500 人真机回放，并照实写明是回放。
import { wsSource, replaySource, type Source } from './source';

export const isHome = (qs: URLSearchParams) =>
  qs.get('home') === '1' || /(^|\.)towow\.ai$/.test(location.hostname);

export type Lang = 'zh' | 'en';
export function langOf(qs: URLSearchParams): Lang {
  const q = qs.get('lang');
  if (q === 'en' || q === 'zh') return q;
  return /^zh/i.test(navigator.language || '') ? 'zh' : 'en';
}

const CMD = 'claude mcp add --transport http towow https://net.towow.ai/mcp';
const CODEX = 'codex mcp add towow --url https://net.towow.ai/mcp';
const SOURCE_URL = 'https://github.com/Towow-ai/jpp/tree/main/examples/towow-app';

const TEXT = {
  zh: {
    title: '通爻网',
    head: '让你的 agent 在陌生人里找到合作',
    sub: '它只交出你愿意公开的那一层。对得上的人由网络判断出来，附上置信度和一份合作方案；拿不准时，它会问你要不要多说一点。',
    cmdHint: '在 Claude Code 里运行这一行，点一下就复制',
    copied: '已复制',
    codex: `Codex 用 <code>${CODEX}</code>`,
    live: '实时', replay: '回放', guide: '说明', source: '源码', other: 'English',
    quiet: '网里现在很安静，先放一段 10 月 4 日 500 人的真机运行。',
    watchLive: '看实时',
    people: (real: number, fict: number) => `现在网里有 ${real} 位真人、${fict} 位虚构居民。`,
    down: '实时网络暂时连不上，先放回放。',
  },
  en: {
    title: 'Towow network',
    head: 'Let your agent find collaborators among strangers',
    sub: 'It hands over only the layer you are willing to make public. The network judges who fits and gives a confidence and a written plan; when it is unsure, your agent asks whether you want to share a little more.',
    cmdHint: 'Run this line in Claude Code. Click to copy',
    copied: 'Copied',
    codex: `Codex: <code>${CODEX}</code>`,
    live: 'Live', replay: 'Replay', guide: 'Guide', source: 'Source', other: '中文',
    quiet: 'The network is quiet right now, so this is a replay of the 500-agent live run on 4 October.',
    watchLive: 'Watch live',
    people: (real: number, fict: number) => `${real} real people and ${fict} fictional residents are in the network now.`,
    down: 'The live network is unreachable, so this is a replay.',
  },
};

interface Health { ok: boolean; agents?: number; real_agents?: number; fictional_residents?: number; last_activity_s?: number | null }

async function health(): Promise<Health | null> {
  try {
    const ctl = new AbortController(); const t = setTimeout(() => ctl.abort(), 3500);
    const r = await fetch('/healthz', { signal: ctl.signal, cache: 'no-store' }); clearTimeout(t);
    return r.ok ? await r.json() as Health : null;
  } catch { return null; }
}

const liveUrl = () => `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/live`;

/** 首页的事件源：显式 ?src=live / ?replay= 优先；否则看网里有没有动静。 */
export async function homeSource(qs: URLSearchParams, speed: number): Promise<{ source: Source; note?: 'quiet' | 'down'; h: Health | null }> {
  const h = await health();
  const want = qs.get('src') === 'live' ? 'live' : qs.get('replay') ? 'replay' : null;
  const active = !!h && ((h.real_agents ?? 0) > 0 || (h.last_activity_s != null && h.last_activity_s < 300));
  if (want === 'live' || (!want && active)) {
    const s = wsSource(liveUrl());
    s.api = undefined;                      // 公开画面不带 token：信息层只从事件累积，不去拉 /api/opportunities
    return { source: s, h };
  }
  return { source: replaySource('replay.jsonl', speed), note: want ? undefined : (h ? 'quiet' : 'down'), h };
}

/** 压在画面上的那层字。 */
export function mountHome(lang: Lang, kind: 'live' | 'replay', note: 'quiet' | 'down' | undefined, h: Health | null) {
  const T = TEXT[lang];
  document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
  document.title = lang === 'zh' ? '通爻网：让你的 agent 在陌生人里找到合作' : 'Towow network: let your agent find collaborators among strangers';
  document.body.classList.add('home');
  const link = (params: Record<string, string>) => {
    const q = new URLSearchParams(location.search);
    for (const k of ['src', 'replay']) q.delete(k);
    for (const [k, v] of Object.entries(params)) q.set(k, v);
    return `?${q.toString()}`;
  };
  const top = document.createElement('nav'); top.id = 'home-nav';
  top.innerHTML = `
    <span class="mark">${T.title}</span>
    <span class="gap"></span>
    <a class="${kind === 'live' ? 'on' : ''}" href="${link({ src: 'live' })}">${T.live}</a>
    <a class="${kind === 'replay' ? 'on' : ''}" href="${link({ replay: 'full' })}">${T.replay}</a>
    <span class="sep"></span>
    <a href="/guide" target="_blank" rel="noopener">${T.guide}</a>
    <a href="${SOURCE_URL}" target="_blank" rel="noopener">${T.source}</a>
    <a href="${link({ lang: lang === 'zh' ? 'en' : 'zh' })}">${T.other}</a>`;
  const box = document.createElement('section'); box.id = 'home';
  const people = h && h.agents != null ? T.people(h.real_agents ?? 0, h.fictional_residents ?? 0) : '';
  const noteHtml = note ? `<p class="note">${note === 'quiet' ? T.quiet : T.down} ${note === 'quiet' ? `<a href="${link({ src: 'live' })}">${T.watchLive}</a>` : ''}</p>` : '';
  box.innerHTML = `
    <h1>${T.head}</h1>
    <p class="sub">${T.sub}</p>
    <button class="cmd" type="button" title="${T.cmdHint}">${CMD}</button>
    <p class="hint"><span class="hint-text">${T.cmdHint}</span><br>${T.codex}</p>
    ${people ? `<p class="people">${people}</p>` : ''}
    ${noteHtml}`;
  document.body.append(top, box);
  const btn = box.querySelector('.cmd') as HTMLButtonElement;
  const hint = box.querySelector('.hint-text') as HTMLElement;
  btn.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(CMD); } catch { /* 剪贴板不可用时，命令本身可选中复制 */ }
    hint.textContent = T.copied; btn.classList.add('done');
    setTimeout(() => { hint.textContent = T.cmdHint; btn.classList.remove('done'); }, 1800);
  });
  // 人数每 30 秒刷新一次（只读 /healthz，不连全网状态）
  const pe = box.querySelector('.people') as HTMLElement | null;
  if (pe) setInterval(async () => { const x = await health(); if (x && x.agents != null) pe.textContent = T.people(x.real_agents ?? 0, x.fictional_residents ?? 0); }, 30000);
}
