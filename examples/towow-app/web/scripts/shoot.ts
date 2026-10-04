// 截图与帧率实测：用本机 Google Chrome（走 GPU），不是 SwiftShader。
// 用法：npx tsx scripts/shoot.ts <url> <输出前缀> [--at 4,9,15] [--perf 18] [--w 1600 --h 900] [--headed] [--click x,y@t]
import { chromium } from 'playwright-core';

const args = process.argv.slice(2);
const url = args[0]; const prefix = args[1];
const opt = (k: string, d: string) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const at = opt('--at', '').split(',').filter(Boolean).map(Number);
const perfWin = Number(opt('--perf', '0'));
const W = Number(opt('--w', '1600')), H = Number(opt('--h', '900'));
const headed = args.includes('--headed');
const clicks = args.filter((_, i) => args[i - 1] === '--click').map((s) => { const [xy, t] = s.split('@'); const [x, y] = xy.split(',').map(Number); return { x, y, t: Number(t) }; });
const hide = args.includes('--hide-text');
const drags = args.filter((_, i) => args[i - 1] === '--drag').map((s) => { const [xy, t] = s.split('@'); const [x1, y1, x2, y2] = xy.split(',').map(Number); return { x1, y1, x2, y2, t: Number(t) }; });
const evals = args.filter((_, i) => args[i - 1] === '--eval').map((s) => { const k = s.lastIndexOf('@'); return { code: s.slice(0, k), t: Number(s.slice(k + 1)) }; });

const browser = await chromium.launch({
  channel: 'chrome', headless: !headed,
  args: ['--use-angle=metal', '--enable-gpu', '--ignore-gpu-blocklist', '--disable-background-timer-throttling', '--disable-renderer-backgrounding', '--disable-backgrounding-occluded-windows'],
});
const page = await browser.newPage({ viewport: { width: W, height: H }, deviceScaleFactor: Number(opt('--dsf', '2')) });
page.on('console', (m) => { if (m.type() === 'error') console.log('[console]', m.text()); });
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
await page.goto(url);
const start = Date.now();
const events = [
  ...at.map((t) => ({ t, kind: 'shot' as const })),
  ...clicks.map((c) => ({ t: c.t, kind: 'click' as const, c })),
  ...evals.map((e) => ({ t: e.t, kind: 'eval' as const, e })),
  ...drags.map((d) => ({ t: d.t, kind: 'drag' as const, d })),
].sort((a, b) => a.t - b.t);
for (const ev of events) {
  const wait = ev.t * 1000 - (Date.now() - start); if (wait > 0) await page.waitForTimeout(wait);
  if (ev.kind === 'click') { await page.mouse.click(ev.c.x, ev.c.y); continue; }
  if (ev.kind === 'drag') { const d = ev.d; await page.mouse.move(d.x1, d.y1); await page.mouse.down(); for (let k = 1; k <= 20; k++) { await page.mouse.move(d.x1 + (d.x2 - d.x1) * k / 20, d.y1 + (d.y2 - d.y1) * k / 20); await page.waitForTimeout(16); } await page.mouse.up(); continue; }
  if (ev.kind === 'eval') { console.log('eval', await page.evaluate(ev.e.code)); continue; }
  if (hide) await page.addStyleTag({ content: '#caption,#stat,#panel{visibility:hidden!important}' });
  await page.screenshot({ path: `${prefix}-${String(ev.t).padStart(2, '0')}s.png` });
  console.log('shot', ev.t);
}
if (perfWin > 0) {
  await page.evaluate(() => { (window as any).__perf.frames.length = 0; (window as any).__perf.js.length = 0; });
  await page.waitForTimeout(perfWin * 1000);
  const r = await page.evaluate(() => { const p = (window as any).__perf; return { js: p.js.slice(), frames: p.frames.slice(), renderer: p.renderer, dpr: devicePixelRatio, w: innerWidth, h: innerHeight }; });
  const f = r.frames.slice().sort((a: number, b: number) => a - b);
  const med = f[Math.floor(f.length / 2)];
  const p99 = f[Math.floor(f.length * 0.99)];
  const js = r.js.slice().sort((a: number, b: number) => a - b);
  console.log(JSON.stringify({ js_ms_median: +js[js.length >> 1].toFixed(2), js_ms_p99: +js[Math.floor(js.length * 0.99)].toFixed(2), renderer: r.renderer, viewport: `${r.w}x${r.h}@${r.dpr}`, frames: f.length, seconds: perfWin, fps_avg: +(f.length / perfWin).toFixed(1), fps_median: +(1000 / med).toFixed(1), fps_1pct_low: +(1000 / p99).toFixed(1) }));
}
await browser.close();
