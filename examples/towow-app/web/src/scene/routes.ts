// 海上航线：确认过的关系。实例化带状网格贴在海面上，宽窄照置信度，
// 船光在两条车道上相向而行；被否掉的航线退成灰痕，留在海面上慢慢褪去。
import {
  AdditiveBlending, BufferAttribute, InstancedBufferAttribute, InstancedBufferGeometry, Mesh, ShaderMaterial, Scene, Vector3,
} from 'three';
import { C, T } from '../style';
import { GLSL_COMMON, shared, col, uploadSlots } from './common';

const SEG = 20;
export const EDGE_STATE = { new: 0, up: 1, down: 2, gone: 3 } as const;

export class Routes {
  mesh: Mesh;
  readonly cap: number;
  private a: Float32Array; private b: Float32Array; private s: Float32Array; private s2: Float32Array;
  private free: number[] = [];
  private top = 0;
  private dirty = false;
  private slots = new Set<number>();
  private goneQueue: { slot: number; t: number }[] = [];

  constructor(scene: Scene, cap: number, lenRef = 120) {
    this.cap = cap;
    const g = new InstancedBufferGeometry();
    const n = (SEG + 1) * 2;
    const u = new Float32Array(n), side = new Float32Array(n), dummy = new Float32Array(n * 3);
    for (let i = 0; i <= SEG; i++) { u[i * 2] = u[i * 2 + 1] = i / SEG; side[i * 2] = -1; side[i * 2 + 1] = 1; }
    const idx: number[] = [];
    for (let i = 0; i < SEG; i++) { const k = i * 2; idx.push(k, k + 1, k + 2, k + 1, k + 3, k + 2); }
    g.setIndex(idx);
    g.setAttribute('position', new BufferAttribute(dummy, 3));
    g.setAttribute('aU', new BufferAttribute(u, 1));
    g.setAttribute('aSide', new BufferAttribute(side, 1));
    this.a = new Float32Array(cap * 3); this.b = new Float32Array(cap * 3);
    this.s = new Float32Array(cap * 4); this.s2 = new Float32Array(cap * 2);
    g.setAttribute('iA', new InstancedBufferAttribute(this.a, 3));
    g.setAttribute('iB', new InstancedBufferAttribute(this.b, 3));
    g.setAttribute('iS', new InstancedBufferAttribute(this.s, 4)); // conf, state, tChange, seed
    g.setAttribute('iS2', new InstancedBufferAttribute(this.s2, 2)); // prevConf, highlightT
    g.instanceCount = 0;
    const m = new ShaderMaterial({
      uniforms: { ...shared, uLenRef: { value: lenRef }, cSodium: col(C.sodium), cHi: col(C.sodiumHi), cAsh: col(C.ash), cBlue: col(C.blueprint), uGone: { value: T.goneTrace } },
      vertexShader: GLSL_COMMON + /* glsl */ `
        attribute float aU; attribute float aSide;
        attribute vec3 iA; attribute vec3 iB; attribute vec4 iS; attribute vec2 iS2;
        varying float vU; varying float vSide; varying float vLen; varying vec4 vS; varying float vFog; varying float vThin; varying float vHl;
        void main() {
          float conf = iS.x, st = iS.y, tc = iS.z, seed = iS.w;
          float k = easeOut((uTime - tc) / 0.6);
          float c = mix(iS2.x, conf, k);
          vec3 A = iA; vec3 B = iB; A.y = 0.4; B.y = 0.4;
          vec3 d = B - A; float len = length(d.xz);
          vec3 perp = normalize(vec3(-d.z, 0.0, d.x) + 1e-5);
          vec3 ctrl = (A + B) * 0.5 + perp * len * 0.42 * (seed - 0.5);
          float u = aU; float iu = 1.0 - u;
          vec3 P = iu * iu * A + 2.0 * iu * u * ctrl + u * u * B;
          vec3 tg = normalize(2.0 * iu * (ctrl - A) + 2.0 * u * (B - ctrl) + 1e-5);
          vec3 side = normalize(vec3(-tg.z, 0.0, tg.x));
          float w = st > 2.5 ? 0.3 : mix(0.3, 1.7, c * c);
          vec4 mvC = modelViewMatrix * vec4(P, 1.0);
          float pxPerUnit = uPxScale / -mvC.z;
          float minW = 1.0 / pxPerUnit;
          vThin = clamp(w / minW, 0.15, 1.0);
          float ww = max(w, minW);
          vec4 mv = modelViewMatrix * vec4(P + side * aSide * ww * 0.5, 1.0);
          gl_Position = projectionMatrix * mv;
          vU = u; vSide = aSide; vLen = len; vS = vec4(c, st, tc, seed); vFog = fogK(-mv.z);
          vHl = exp(-max(0.0, uTime - iS2.y) / 0.5) + pulseAt(P) * 0.7;
        }`,
      fragmentShader: GLSL_COMMON + /* glsl */ `
        uniform vec3 cSodium, cHi, cAsh, cBlue; uniform float uGone; uniform float uLenRef;
        varying float vU; varying float vSide; varying float vLen; varying vec4 vS; varying float vFog; varying float vThin; varying float vHl;
        void main() {
          float c = vS.x, st = vS.y, age = uTime - vS.z, seed = vS.w;
          float edge = 1.0 - vSide * vSide;
          vec3 col; float a;
          if (st > 2.5) {
            // 灰痕：不再有船，慢慢褪去
            float fade = 1.0 - clamp(age / uGone, 0.0, 1.0);
            col = cAsh; a = 0.22 * fade * fade * edge;
          } else {
            // 新航线从 a 画到 b
            float drawn = smoothstep(vU - 0.08, vU, age / 0.6);
            col = mix(cSodium, cHi, c * 0.6);
                        float confirmed = max(step(0.5, st) * step(st, 1.5), step(0.8, c));
            // 候选航线只是海面上一道很淡的痕；确认后才变实、有船
            a = (0.018 + (0.03 + 0.13 * c) * confirmed) * edge * drawn;
            // 两条车道相向而行的船光，密度与速度随置信度
            float lane = vSide > 0.0 ? 1.0 : -1.0;
            float spacing = max(16.0, 90.0 - 60.0 * c);
            float speed = 4.0 + 18.0 * c;
            float x = vU * vLen / spacing - lane * uTime * speed / spacing + seed * 7.0 + (lane > 0.0 ? 0.5 : 0.0);
            float fx = fract(x) * spacing; float ship = smoothstep(2.2, 0.0, abs(fx - spacing * 0.5));
            float laneMask = smoothstep(0.15, 0.6, abs(vSide));
                        a += ship * laneMask * (0.2 + 0.75 * c) * drawn * confirmed;
            // 长航线按长度摊薄亮度，避免远距离的线压过近处的网
            a *= clamp(uLenRef / max(vLen, 1.0), 0.3, 1.0);
            // 关系变化时整条亮一下（up 暖白，构型高亮冷白）
            float pulse = age < 0.0 ? 0.0 : exp(-age / 0.4) * (st < 1.5 ? 1.0 : 0.3);
            col += cHi * pulse; a += pulse * 0.6 * edge;
            col = mix(col, cBlue, vHl * 0.8); a += vHl * 0.8 * edge;
          }
          gl_FragColor = vec4(col * a * vFog * vThin, 1.0);
        }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    this.mesh = new Mesh(g, m);
    this.mesh.frustumCulled = false;
    this.mesh.renderOrder = 1;
    scene.add(this.mesh);
  }

  alloc(): number {
    if (this.free.length) return this.free.pop()!;
    if (this.top < this.cap) return this.top++;
    // 满了：强制回收最早的灰痕
    const g = this.goneQueue.shift();
    return g ? g.slot : this.cap - 1;
  }

  set(slot: number, A: Vector3, B: Vector3, conf: number, state: number, now: number, seed: number) {
    const prev = this.s[slot * 4 + 1] === EDGE_STATE.gone && state !== EDGE_STATE.gone ? 0 : this.s[slot * 4];
    this.a.set([A.x, A.y, A.z], slot * 3); this.b.set([B.x, B.y, B.z], slot * 3);
    this.s2[slot * 2] = state === EDGE_STATE.new ? conf : prev;
    this.s.set([conf, state, now, seed], slot * 4);
    if (state === EDGE_STATE.gone) this.goneQueue.push({ slot, t: now });
    this.dirty = true; this.slots.add(slot);
  }
  highlight(slot: number, now: number) { this.s2[slot * 2 + 1] = now; this.dirty = true; this.slots.add(slot); }
  conf(slot: number) { return this.s[slot * 4]; }
  state(slot: number) { return this.s[slot * 4 + 1]; }

  /** 灰痕褪尽后回收槽位；返回被回收的槽位，供上层删除键。 */
  reclaim(now: number): number[] {
    const out: number[] = [];
    while (this.goneQueue.length && now - this.goneQueue[0].t > T.goneTrace) {
      const g = this.goneQueue.shift()!;
      if (this.s[g.slot * 4 + 1] === EDGE_STATE.gone && this.s[g.slot * 4 + 2] === g.t) {
        this.s[g.slot * 4] = 0; this.free.push(g.slot); out.push(g.slot); this.dirty = true; this.slots.add(g.slot);
      }
    }
    return out;
  }

  upload() {
    if (!this.dirty) return;
    const g = this.mesh.geometry as InstancedBufferGeometry;
    const full = g.instanceCount !== this.top && this.slots.size > 96;
    for (const k of ['iA', 'iB', 'iS', 'iS2']) uploadSlots(g.getAttribute(k) as InstancedBufferAttribute, this.slots, full);
    g.instanceCount = this.top;
    this.dirty = false; this.slots.clear();
  }
}
