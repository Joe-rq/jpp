// 港灯：每个 agent 与每座新港（构型节点）是一个点精灵。全部状态在 GPU 属性里，
// 每帧只上传本帧被事件改过的属性。
import { AdditiveBlending, BufferAttribute, BufferGeometry, Points, ShaderMaterial, Scene, Vector3 } from 'three';
import { C } from '../style';
import { GLSL_COMMON, shared, col, uploadSlots } from './common';

export class Ports {
  points: Points;
  readonly cap: number;
  count = 0;
  pos: Float32Array;          // xyz
  private base: Float32Array; // kind, tier, opp, bornT
  private flash: Float32Array;// tFlash, type, tState, state(1 活, 2 离开)
  private dirtyPos = false; private dirtyBase = false; private dirtyFlash = false;
  selected = { value: -1 };
  private fSlots = new Set<number>(); private bSlots = new Set<number>(); private pSlots = new Set<number>();

  constructor(scene: Scene, cap: number) {
    this.cap = cap;
    this.pos = new Float32Array(cap * 3);
    this.base = new Float32Array(cap * 4);
    this.flash = new Float32Array(cap * 4);
    for (let i = 0; i < cap; i++) { this.flash[i * 4] = -100; this.base[i * 4 + 3] = -100; }
    const g = new BufferGeometry();
    g.setAttribute('position', new BufferAttribute(this.pos, 3));
    g.setAttribute('aBase', new BufferAttribute(this.base, 4));
    g.setAttribute('aFlash', new BufferAttribute(this.flash, 4));
    g.setDrawRange(0, 0);
    const m = new ShaderMaterial({
      uniforms: {
        ...shared, uSel: this.selected,
        cSodium: col(C.sodium), cHi: col(C.sodiumHi), cVer: col(C.vermilion), cBlue: col(C.blueprint), cAsh: col(C.ash), cFlight: col(C.flight),
      },
      vertexShader: GLSL_COMMON + /* glsl */ `
        attribute vec4 aBase; attribute vec4 aFlash;
        uniform float uSel;
        uniform vec3 cSodium, cHi, cVer, cBlue, cAsh, cFlight;
        varying vec3 vCol; varying float vCore; varying float vRing; varying float vKind; varying float vSettle;
        void main() {
          float kind = aBase.x, tier = aBase.y, opp = aBase.z, born = aBase.w;
          float age = uTime - born;
          // 接入：光落下 1.4s 后港灯才点亮
          float lit = kind > 0.5 ? easeOut((age - 0.0) / 0.7) : easeOut((age - 2.35) / 0.5);
          float st = aFlash.w; float tSt = uTime - aFlash.z;
          float leaving = (st > 1.5 && st < 2.5) ? clamp(tSt / 1.2, 0.0, 1.0) : 0.0;
          // 判过「不成立」的候选：暗成灰点，留在海上
          float dimK = (st > 2.5 && st < 3.5) ? easeOut(tSt / 0.9) : 0.0;
          // 被召回、等着判的候选：亮着，比周围大一圈，直到判完
          float candK = st > 3.5 ? easeOut(tSt / 0.4) : 0.0;
          vec4 mv = modelViewMatrix * vec4(position, 1.0);
          gl_Position = projectionMatrix * mv;
          float sizeW = kind > 0.5 ? 6.0 + 2.0 * tier + 2.2 * sqrt(opp) : 3.6 + 1.6 * tier + 1.0 * sqrt(opp);
          // 闪光
          float tf = uTime - aFlash.x; float ft = aFlash.y;
          float tau = ft == 1.0 ? 0.15 : ft == 2.0 ? 0.1 : ft == 3.0 ? 0.2 : ft == 7.0 ? 0.5 : 0.25;
          float f = tf >= 0.0 ? exp(-tf / tau) : 0.0;
          vec3 fc = ft == 1.0 ? cHi : ft == 2.0 ? cAsh : ft == 3.0 ? cVer : ft == 4.0 ? cFlight * 0.3 : ft == 5.0 ? cBlue : ft == 6.0 ? cHi : cBlue;
          float fAmp = ft == 2.0 ? 0.35 : ft == 4.0 ? -0.5 : 1.4;
          // 新港只在点亮那一刻是蓝图白，几秒后回到暖色（冷白只属于开垦）
          float settle = kind > 0.5 ? smoothstep(4.0, 12.0, age) : 1.0;
          vec3 baseC = kind > 0.5 ? mix(mix(cHi, cBlue, 0.55), cHi, settle) : mix(cSodium, cHi, tier / 2.0);
          float bright = (kind > 0.5 ? mix(1.4, 0.95, settle) : 0.9 + 0.45 * tier + 0.08 * min(opp, 8.0));
          vec3 c = baseC * bright * lit;
          c = mix(c, cAsh * 0.6, leaving);
          c = mix(c, cAsh * 0.5, dimK * 0.88);
          c = mix(c, cHi * 1.7, candK * 0.65);
          c += fc * f * fAmp * lit;
          float pu = pulseAt(position);
          c += cHi * pu * 1.6 * lit;
          float sel = abs(float(gl_VertexID) - uSel) < 0.5 ? 1.0 : 0.0;
          float px = sizeW * (1.0 + 0.6 * f * step(0.0, fAmp) + 0.5 * sel) * (1.0 - 0.3 * dimK + 0.55 * candK) * uPxScale / -mv.z;
          gl_PointSize = clamp(px, 2.0, kind > 0.5 ? 64.0 : 34.0) * (lit > 0.0 ? 1.0 : 0.0) * (1.0 - leaving * 0.5);
          vCol = max(c, vec3(0.0)) * fogK(-mv.z);
          vCore = clamp(px / 6.0, 0.25, 1.0);
          vRing = sel;
          vKind = kind; vSettle = settle;
          if (st > 1.5 && st < 2.5 && tSt > 30.0) gl_PointSize = 0.0;
        }`,
      fragmentShader: /* glsl */ `
        varying vec3 vCol; varying float vCore; varying float vRing; varying float vKind; varying float vSettle;
        void main() {
          vec2 d = gl_PointCoord - 0.5; float r = length(d) * 2.0;
          if (r > 1.0) discard;
          float core = smoothstep(0.32, 0.0, r);
          float halo = exp(-r * r * 6.0) * 0.55;
          float ring = 0.0;
          if (vKind > 0.5) ring += smoothstep(0.05, 0.0, abs(r - 0.72)) * mix(0.8, 0.4, vSettle);
          ring += vRing * smoothstep(0.06, 0.0, abs(r - 0.92)) * 1.2;
          float a = core * vCore + halo + ring;
          gl_FragColor = vec4(vCol * a, 1.0);
        }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    this.points = new Points(g, m);
    this.points.frustumCulled = false;
    scene.add(this.points);
  }

  add(p: Vector3, kind: number, tier: number, now: number): number {
    const i = this.count < this.cap ? this.count++ : this.cap - 1;
    this.pos.set([p.x, p.y, p.z], i * 3);
    this.base.set([kind, tier, 0, now], i * 4);
    this.flash.set([-100, 0, now, 1], i * 4);
    this.dirtyPos = this.dirtyBase = this.dirtyFlash = true;
    this.pSlots.add(i); this.bSlots.add(i); this.fSlots.add(i);
    return i;
  }
  /** 快照里已经在的港：不播放接入动画。 */
  addSilent(p: Vector3, kind: number, tier: number, now: number) { return this.add(p, kind, tier, now - 10); }
  flashAt(i: number, type: number, now: number) { this.flash[i * 4] = now; this.flash[i * 4 + 1] = type; this.dirtyFlash = true; this.fSlots.add(i); }
  setTier(i: number, tier: number) { this.base[i * 4 + 1] = tier; this.dirtyBase = true; this.bSlots.add(i); }
  setOpp(i: number, n: number) { this.base[i * 4 + 2] = n; this.dirtyBase = true; this.bSlots.add(i); }
  /** 判「不成立」：暗成灰点（now 可以是将来的时刻） */
  dim(i: number, now: number) { this.flash[i * 4 + 2] = now; this.flash[i * 4 + 3] = 3; this.dirtyFlash = true; this.fSlots.add(i); }
  /** 被召回的候选：亮起并保持，直到判过（dim）或成线 */
  cand(i: number, now: number) { this.flash[i * 4 + 2] = now; this.flash[i * 4 + 3] = 4; this.dirtyFlash = true; this.fSlots.add(i); }
  /** 候选收成机会后回到普通状态 */
  live(i: number, now: number) { this.flash[i * 4 + 2] = now; this.flash[i * 4 + 3] = 1; this.dirtyFlash = true; this.fSlots.add(i); }
  leave(i: number, now: number) { this.flash[i * 4 + 2] = now; this.flash[i * 4 + 3] = 2; this.dirtyFlash = true; this.fSlots.add(i); }
  revive(i: number, p: Vector3, tier: number, now: number) {
    this.pos.set([p.x, p.y, p.z], i * 3); this.base[i * 4 + 1] = tier; this.base[i * 4 + 3] = now;
    this.flash[i * 4 + 2] = now; this.flash[i * 4 + 3] = 1; this.dirtyPos = this.dirtyBase = this.dirtyFlash = true;
    this.pSlots.add(i); this.bSlots.add(i); this.fSlots.add(i);
  }
  get(i: number, out: Vector3) { return out.set(this.pos[i * 3], this.pos[i * 3 + 1], this.pos[i * 3 + 2]); }

  upload() {
    const g = this.points.geometry;
    const grew = g.drawRange.count !== this.count;
    if (this.dirtyPos) { uploadSlots(g.getAttribute('position') as BufferAttribute, this.pSlots, grew && this.pSlots.size > 96); this.dirtyPos = false; this.pSlots.clear(); }
    if (this.dirtyBase) { uploadSlots(g.getAttribute('aBase') as BufferAttribute, this.bSlots, false); this.dirtyBase = false; this.bSlots.clear(); }
    if (this.dirtyFlash) { uploadSlots(g.getAttribute('aFlash') as BufferAttribute, this.fSlots, false); this.dirtyFlash = false; this.fSlots.clear(); }
    g.setDrawRange(0, this.count);
  }
}
