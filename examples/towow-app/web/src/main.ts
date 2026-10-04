import {
  HalfFloatType, PerspectiveCamera, Scene, Vector3, WebGLRenderer, NoToneMapping, SRGBColorSpace,
} from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { EffectComposer, RenderPass, EffectPass, BloomEffect, VignetteEffect, KernelSize } from 'postprocessing';
import { Net } from './net';
import { Director } from './director';
import { UI } from './ui';
import { mockSource, wsSource, replaySource, autoSource, type Source } from './source';
import { shared } from './scene/common';
import { HEX } from './style';

const qs = new URLSearchParams(location.search);
const N = Math.max(50, Math.min(20000, Number(qs.get('n')) || 500));
const srcParam = qs.get('src') || 'auto';
const meParam = qs.get('me') || undefined;
const heavy = N > 3000;

// ---------- 渲染器 ----------
const canvas = document.getElementById('gl') as HTMLCanvasElement;
const renderer = new WebGLRenderer({ canvas, antialias: false, powerPreference: 'high-performance', stencil: false, depth: true });
const dprCap = heavy ? 1.25 : 1.5;
renderer.setPixelRatio(Math.min(devicePixelRatio, dprCap));
renderer.toneMapping = NoToneMapping;
renderer.outputColorSpace = SRGBColorSpace;
renderer.setClearColor(HEX.sea);

const scene = new Scene();
const camera = new PerspectiveCamera(38, innerWidth / innerHeight, 2, 20000);
const controls = new OrbitControls(camera, canvas);
controls.enableDamping = true; controls.dampingFactor = 0.08;
controls.maxPolarAngle = Math.PI * 0.47; controls.minDistance = 30;

const net = new Net(scene, N);
controls.maxDistance = net.layout.D * 4;
shared.uFogNear.value = net.layout.D * 1.4; shared.uFogFar.value = net.layout.D * 4.2;

const composer = new EffectComposer(renderer, { frameBufferType: HalfFloatType });
composer.addPass(new RenderPass(scene, camera));
const bloom = new BloomEffect({ mipmapBlur: true, intensity: 1.15, luminanceThreshold: 0.2, luminanceSmoothing: 0.35, radius: 0.72, kernelSize: KernelSize.MEDIUM });
bloom.resolution.scale = 0.5;
composer.addPass(new EffectPass(camera, bloom, new VignetteEffect({ offset: 0.32, darkness: 0.62 })));

function resize() {
  // 画布宽度由 CSS 决定：侧栏打开时画面让出右侧一列，侧栏不压在画面上
  const w = canvas.clientWidth || innerWidth, h = canvas.clientHeight || innerHeight;
  renderer.setSize(w, h, false); composer.setSize(w, h);
  camera.aspect = w / h; camera.updateProjectionMatrix();
  // 1 单位世界长度在视深 1 处的像素数
  shared.uPxScale.value = (h * renderer.getPixelRatio() * 0.5) * camera.projectionMatrix.elements[5];
}
addEventListener('resize', resize); new ResizeObserver(() => resize()).observe(canvas); resize();

// ---------- 时钟、导演、文字 ----------
const t0 = performance.now() / 1000;
const clock = () => performance.now() / 1000 - t0;
const director = new Director(camera, controls, net, clock);
director.place();
if (qs.get('cruise')) director.cruiseOnly = true;
const ui = new UI(net);
director.caption = (s, p) => ui.caption(s, p);
net.onNarrate = (s) => ui.caption(s, 1);
net.onMoment = (m) => director.moment(m, clock());
net.onChange = (id) => ui.refresh(id);
/** 「我的 agent」：镜头以它为中心，侧栏常驻它的机会。?me= 或回放 meta 里的 me 都走这里，只生效一次 */
let me: string | undefined;
function enableMe(id: string) {
  if (me) return;
  me = id; net.me = id; director.me = id; ui.me = id;
  net.onMine = () => ui.mine();
  document.body.classList.add('me');
  ui.open(id);
}
director.onMode = (m) => document.body.classList.toggle('free', m === 'free');

// ---------- 事件源 ----------
// 回放默认 4 倍速：10 分钟的真实运行 2 分半看完。?data=jev|fixture|mock 可以替没写 meta 行的文件声明出处。
let source: Source;
const speed = Math.max(0.25, Math.min(64, Number(qs.get('speed')) || 4));
// ?replay=nature|full 直接放对应回放（不先探测后端）；?file= 仍可指定任意文件
const REPLAYS: Record<string, string> = { nature: 'replay-nature.jsonl', full: 'replay.jsonl' };
const replayParam = qs.get('replay') || undefined;
const replayFile = qs.get('file') || (replayParam && REPLAYS[replayParam]) || 'replay.jsonl';
const dataOrigin = qs.get('data') || undefined;
if (srcParam === 'auto' && replayParam) source = replaySource(replayFile, speed, dataOrigin);
else if (srcParam === 'auto') source = autoSource(replayFile, speed, dataOrigin);
else if (srcParam === 'mock') source = mockSource(N, Number(qs.get('seed')) || 7);
else if (srcParam === 'replay') source = replaySource(replayFile, speed, dataOrigin);
else source = wsSource(srcParam);
// 数据来源写在左下角那一行最前面：实时 / 模拟数据 / 回放
let lastStat = -1;
function syncSource(now: number) {
  ui.srcKind = source.kind; ui.connected = source.connected ?? true; ui.origin = source.origin; ui.speed = source.speed;
  net.speed = source.speed;
  if (ui.api === undefined && source.kind === 'live') { ui.api = source.api ?? ''; if (ui.openId) ui.open(ui.openId); }
  if (now - lastStat > 1) { lastStat = now; ui.stats(now); }
}
if (meParam) enableMe(meParam);

// ---------- 点击拾取 ----------
let down: { x: number; y: number } | null = null;
canvas.addEventListener('pointerdown', (e) => { down = { x: e.clientX, y: e.clientY }; });
canvas.addEventListener('pointerup', (e) => {
  if (!down || Math.hypot(e.clientX - down.x, e.clientY - down.y) > 5) { down = null; return; }
  down = null;
  const hit = net.pick(e.clientX, e.clientY, canvas.clientWidth, canvas.clientHeight, (v: Vector3) => v.project(camera));
  if (hit) { ui.open(hit.id); net.ports.selected.value = hit.idx; } else { ui.close(); }
});
// ?me 的 agent 自己始终带一圈白环，画面里一眼认得出「我」在哪
let meIdx = -1;
ui.onClose = () => { net.ports.selected.value = meIdx; };
ui.onJump = (id) => { const n = net.nodes.get(id); if (n) net.ports.selected.value = n.idx; };

// ---------- 帧率记录（?fps=1 显示；Playwright 读 window.__perf） ----------
const perf = { frames: [] as number[], js: [] as number[], renderer: '' };
(window as unknown as { __perf: typeof perf }).__perf = perf;
{
  const gl = renderer.getContext();
  const ext = gl.getExtension('WEBGL_debug_renderer_info');
  perf.renderer = ext ? String(gl.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : String(gl.getParameter(gl.RENDERER));
}
const fpsEl = qs.get('fps') ? document.getElementById('fps') : null;
if (fpsEl) fpsEl.style.display = 'block';

// 调试与截图脚本用的入口
(window as unknown as { __debug: unknown }).__debug = {
  net, ui, director, source,
  openBusiest(kind: 'agent' | 'config' = 'agent') {
    let best: { id: string; idx: number; n: number } | undefined;
    for (const n of net.nodes.values()) {
      if (n.kind !== kind || !n.alive) continue;
      const k = n.edges.size + n.configs.size * 3 + n.missing.size;
      if (!best || k > best.n) best = { id: n.id, idx: n.idx, n: k };
    }
    if (best) { ui.open(best.id); net.ports.selected.value = best.idx; }
    return best?.id;
  },
};

// ---------- 主循环 ----------
let last = clock();
let fpsAcc = 0, fpsN = 0;
// 质量分级：连续 2 秒中位帧时间超过 22ms 就降一档像素比（最低 0.75），不回升
let qAcc: number[] = []; let dpr = renderer.getPixelRatio();
function adapt(dt: number) {
  qAcc.push(dt); if (qAcc.length < 120) return;
  const med = qAcc.sort((a, b) => a - b)[60]; qAcc = [];
  if (med > 0.022 && dpr > 0.75) { dpr = Math.max(0.75, dpr - 0.25); renderer.setPixelRatio(dpr); resize(); }
}
function frame() {
  const tj = performance.now();
  const now = clock();
  const dt = Math.min(0.1, now - last); last = now;
  shared.uTime.value = now;
  // 回放 meta 带 me 时，要赶在第一批事件放出之前切成「我的 agent」视角，导演才不会为别人的接入花镜头
  if (!me && source.meta && typeof source.meta.me === 'string') enableMe(source.meta.me);
  source.pump(now, (e) => net.ingest(e, now));
  net.flush(now);
  if (me && meIdx < 0) { const n = net.nodes.get(me); if (n) { meIdx = n.idx; if (net.ports.selected.value < 0) net.ports.selected.value = meIdx; } }
  syncSource(now);
  net.reclaim(now);
  director.update(now, dt);
  net.upload();
  composer.render(dt);
  if (!qs.get('fixdpr')) adapt(dt);
  perf.frames.push(dt * 1000); if (perf.frames.length > 3000) perf.frames.splice(0, 1000);
  if (fpsEl) { fpsAcc += dt; fpsN++; if (fpsAcc > 0.5) { fpsEl.textContent = `${Math.round(fpsN / fpsAcc)} fps  n=${N}  ${source.label}`; fpsAcc = 0; fpsN = 0; } }
  perf.js.push(performance.now() - tj); if (perf.js.length > 3000) perf.js.splice(0, 1000);
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);
