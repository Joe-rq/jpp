// 空中的航班：试探信号、判断回程、补信息的信号弹、开垦船队、接入时落下的光。
// 环形池，每架航班 TRAIL 个顶点；位置全部在顶点着色器里按时间算，JS 只在起飞时写一次。
// 同一份几何画两遍：一遍是细光迹（线段），一遍是航班头部的光点（船队与接入每个点都画）。
import {
  AdditiveBlending, BufferAttribute, BufferGeometry, LineSegments, Points, ShaderMaterial, Scene, Vector3,
} from 'three';
import { C } from '../style';
import { GLSL_COMMON, shared, col } from './common';

const TRAIL = 12;

const VERT = (lines: boolean) => GLSL_COMMON + /* glsl */ `
  attribute vec3 aTo; attribute vec4 aTm; attribute float aTrail;
  uniform vec3 cFlight, cSodium, cHi, cVer, cAsh, cBlue;
  uniform float uProbeGain;
  varying vec3 vCol;
  vec3 colorOf(float kind) {
    return kind == 0.0 ? cFlight * 0.8 : kind == 1.0 ? cSodium * 1.1 : kind == 2.0 ? cVer * 1.3 : kind == 3.0 ? cHi : kind == 4.0 ? cAsh : kind == 5.0 ? cBlue * 1.3 : cHi * 1.6;
  }
  void main() {
    float kind = aTm.z;
    float gap = kind == 5.0 ? 0.02 : kind == 6.0 ? 0.03 : kind == 2.0 ? 0.007 : kind == 0.0 ? 0.006 : 0.009;
    float s0 = (uTime - aTm.x) / aTm.y - aTrail * gap;
    float inside = step(0.0, s0) * step(s0, 1.0);
    ${lines ? '' : `
    bool head = aTrail < 0.5 || kind == 5.0 || kind == 6.0;
    if (inside < 0.5 || !head) { gl_Position = vec4(2.0, 2.0, 2.0, 1.0); gl_PointSize = 0.0; vCol = vec3(0.0); return; }`}
    float s = clamp(s0, 0.0, 1.0);
    vec3 A = position; vec3 B = aTo; vec3 P;
    if (kind == 6.0) {
      P = mix(A, B, s * s); // 接入：从高空斜落，越落越快
    } else if (kind == 5.0) {
      float e = easeInOut(s); // 船队：贴着海面走
      vec3 d = B - A; vec3 perp = normalize(vec3(-d.z, 0.0, d.x) + 1e-5);
      vec3 ctrl = (A + B) * 0.5 + perp * length(d) * 0.12;
      float iu = 1.0 - e; P = iu * iu * A + 2.0 * iu * e * ctrl + e * e * B; P.y = 0.8;
    } else {
      float e = kind == 0.0 ? s : easeInOut(s);
      vec3 ctrl = (A + B) * 0.5 + vec3(0.0, aTm.w, 0.0);
      float iu = 1.0 - e; P = iu * iu * A + 2.0 * iu * e * ctrl + e * e * B;
    }
    vec4 mv = modelViewMatrix * vec4(P, 1.0);
    gl_Position = projectionMatrix * mv;
    float tr = 1.0 - aTrail / ${TRAIL - 1}.0;
    float fadeIn = smoothstep(0.0, 0.06, s0), fadeOut = 1.0 - smoothstep(0.92, 1.0, s0);
    ${lines ? `
    vCol = colorOf(kind) * tr * inside * fadeIn * fadeOut * fogK(-mv.z) * (kind == 0.0 ? 0.55 * uProbeGain : 0.8);` : `
    float sizeW = kind == 0.0 ? 1.8 : kind == 1.0 ? 1.9 : kind == 2.0 ? 3.4 : kind == 5.0 ? 2.6 : kind == 6.0 ? 4.0 : 2.0;
    float px = sizeW * uPxScale / -mv.z * (0.4 + 0.6 * tr);
    float maxPx = kind == 0.0 ? 4.0 : kind == 6.0 ? 14.0 : 9.0;
    gl_PointSize = clamp(px, 1.5, maxPx);
    float sub = min(1.0, px / 1.5);
    vCol = colorOf(kind) * tr * fadeIn * fadeOut * sub * fogK(-mv.z);`}
  }`;

export class Flights {
  points: Points; lines: LineSegments;
  readonly cap: number;
  private from: Float32Array; private to: Float32Array; private tm: Float32Array;
  private head = 0;
  private lo = Infinity; private hi = -1;

  /** probeGain：节点越多，同时在飞的试探越多，单条光迹相应变淡，整体亮度不随规模膨胀。 */
  constructor(scene: Scene, cap: number, probeGain = 1) {
    this.cap = cap;
    const nv = cap * TRAIL;
    this.from = new Float32Array(nv * 3); this.to = new Float32Array(nv * 3); this.tm = new Float32Array(nv * 4);
    const trail = new Float32Array(nv);
    for (let i = 0; i < nv; i++) { trail[i] = i % TRAIL; this.tm[i * 4] = -1000; this.tm[i * 4 + 1] = 1; }
    const g = new BufferGeometry();
    g.setAttribute('position', new BufferAttribute(this.from, 3));
    g.setAttribute('aTo', new BufferAttribute(this.to, 3));
    g.setAttribute('aTm', new BufferAttribute(this.tm, 4)); // t0, dur, kind, arcH
    g.setAttribute('aTrail', new BufferAttribute(trail, 1));
    const uniforms = {
      ...shared, uProbeGain: { value: probeGain }, cFlight: col(C.flight), cSodium: col(C.sodium), cHi: col(C.sodiumHi), cVer: col(C.vermilion), cAsh: col(C.ash), cBlue: col(C.blueprint),
    };
    const pm = new ShaderMaterial({
      uniforms, vertexShader: VERT(false),
      fragmentShader: /* glsl */ `
        varying vec3 vCol;
        void main() {
          vec2 d = gl_PointCoord - 0.5; float r = length(d) * 2.0; if (r > 1.0) discard;
          gl_FragColor = vec4(vCol * exp(-r * r * 4.0), 1.0);
        }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    this.points = new Points(g, pm);
    this.points.frustumCulled = false; this.points.renderOrder = 2;
    scene.add(this.points);

    // 光迹：同一份顶点，按相邻尾迹点连成线段
    const lg = new BufferGeometry();
    for (const k of ['position', 'aTo', 'aTm', 'aTrail']) lg.setAttribute(k, g.getAttribute(k));
    const idx = new Uint32Array(cap * (TRAIL - 1) * 2);
    let w = 0;
    for (let f = 0; f < cap; f++) for (let k = 0; k < TRAIL - 1; k++) { idx[w++] = f * TRAIL + k; idx[w++] = f * TRAIL + k + 1; }
    lg.setIndex(new BufferAttribute(idx, 1));
    const lm = new ShaderMaterial({
      uniforms, vertexShader: VERT(true),
      fragmentShader: /* glsl */ `varying vec3 vCol; void main() { gl_FragColor = vec4(vCol, 1.0); }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    this.lines = new LineSegments(lg, lm);
    this.lines.frustumCulled = false; this.lines.renderOrder = 2;
    scene.add(this.lines);
  }

  launch(A: Vector3, B: Vector3, t0: number, dur: number, kind: number, arcH: number) {
    const f = this.head; this.head = (this.head + 1) % this.cap;
    for (let k = 0; k < TRAIL; k++) {
      const v = f * TRAIL + k;
      this.from[v * 3] = A.x; this.from[v * 3 + 1] = A.y; this.from[v * 3 + 2] = A.z;
      this.to[v * 3] = B.x; this.to[v * 3 + 1] = B.y; this.to[v * 3 + 2] = B.z;
      this.tm[v * 4] = t0; this.tm[v * 4 + 1] = dur; this.tm[v * 4 + 2] = kind; this.tm[v * 4 + 3] = arcH;
    }
    this.lo = Math.min(this.lo, f); this.hi = Math.max(this.hi, f);
  }

  upload() {
    if (this.hi < 0) return;
    const g = this.points.geometry;
    const start = this.lo * TRAIL, count = (this.hi - this.lo + 1) * TRAIL;
    for (const [k, s] of [['position', 3], ['aTo', 3], ['aTm', 4]] as const) {
      const a = g.getAttribute(k) as BufferAttribute;
      a.clearUpdateRanges(); a.addUpdateRange(start * s, count * s); a.needsUpdate = true;
    }
    this.lo = Infinity; this.hi = -1;
  }
}
