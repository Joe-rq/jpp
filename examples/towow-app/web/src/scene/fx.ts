// 海面涟漪（接入、点亮、构型成员、离开）与新港上方的拓扑蓝图。
import {
  AdditiveBlending, BufferAttribute, BufferGeometry, Group, InstancedBufferAttribute, InstancedBufferGeometry,
  LineSegments, Mesh, ShaderMaterial, Scene, Vector3,
} from 'three';
import type { Shape } from '../protocol';
import { C, T } from '../style';
import { GLSL_COMMON, shared, col } from './common';

export class Rings {
  mesh: Mesh; private cap: number; private head = 0;
  private c: Float32Array; private p: Float32Array; private lo = Infinity; private hi = -1;
  constructor(scene: Scene, cap = 512) {
    this.cap = cap;
    const g = new InstancedBufferGeometry();
    g.setAttribute('position', new BufferAttribute(new Float32Array([-1, 0, -1, 1, 0, -1, 1, 0, 1, -1, 0, 1]), 3));
    g.setIndex([0, 2, 1, 0, 3, 2]);
    this.c = new Float32Array(cap * 3); this.p = new Float32Array(cap * 4);
    for (let i = 0; i < cap; i++) this.p[i * 4] = -1000;
    g.setAttribute('iC', new InstancedBufferAttribute(this.c, 3));
    g.setAttribute('iP', new InstancedBufferAttribute(this.p, 4)); // t0, dur, maxR, kind
    g.instanceCount = cap;
    const m = new ShaderMaterial({
      uniforms: { ...shared, cHi: col(C.sodiumHi), cBlue: col(C.blueprint), cAsh: col(C.ash) },
      vertexShader: GLSL_COMMON + /* glsl */ `
        attribute vec3 iC; attribute vec4 iP; varying vec2 vXY; varying float vS; varying float vKind; varying float vFog;
        void main() {
          float s = (uTime - iP.x) / iP.y;
          vS = s; vKind = iP.w; vXY = position.xz;
          if (s < 0.0 || s > 1.0) { gl_Position = vec4(2.0, 2.0, 2.0, 1.0); return; }
          float R = iP.z * (iP.w == 2.0 ? 1.0 : easeOut(s));
          vec4 mv = modelViewMatrix * vec4(iC + vec3(position.x * R, 0.6, position.z * R), 1.0);
          vFog = fogK(-mv.z);
          gl_Position = projectionMatrix * mv;
        }`,
      fragmentShader: /* glsl */ `
        uniform vec3 cHi, cBlue, cAsh; varying vec2 vXY; varying float vS; varying float vKind; varying float vFog;
        void main() {
          if (vS < 0.0 || vS > 1.0) discard;
          float r = length(vXY);
          float w = vKind == 2.0 ? 0.025 : 0.05;
          float ring = smoothstep(w, 0.0, abs(r - 0.96));
          float fill = vKind == 1.0 ? exp(-r * r * 5.0) * 0.6 * (1.0 - vS) : 0.0;
          vec3 c = vKind == 0.0 ? cHi : vKind == 3.0 ? cAsh : cBlue;
          float fade = vKind == 2.0 ? sin(3.14159 * vS) : (1.0 - vS) * (1.0 - vS);
          gl_FragColor = vec4(c * (ring + fill) * fade * vFog, 1.0);
        }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    this.mesh = new Mesh(g, m); this.mesh.frustumCulled = false; this.mesh.renderOrder = 3;
    scene.add(this.mesh);
  }
  spawn(at: Vector3, t0: number, dur: number, maxR: number, kind: number) {
    const i = this.head; this.head = (this.head + 1) % this.cap;
    this.c.set([at.x, at.y, at.z], i * 3); this.p.set([t0, dur, maxR, kind], i * 4);
    this.lo = Math.min(this.lo, i); this.hi = Math.max(this.hi, i);
  }
  upload() {
    if (this.hi < 0) return;
    const g = this.mesh.geometry;
    for (const [k, s] of [['iC', 3], ['iP', 4]] as const) {
      const a = g.getAttribute(k) as InstancedBufferAttribute;
      a.clearUpdateRanges(); a.addUpdateRange(this.lo * s, (this.hi - this.lo + 1) * s); a.needsUpdate = true;
    }
    this.lo = Infinity; this.hi = -1;
  }
}

/** 蓝图：构型拓扑的冷白线稿，悬在新港上方，连着两道投影光。 */
export class Blueprints {
  group = new Group();
  private items: { obj: LineSegments; mat: ShaderMaterial; t0: number; dissolved: number }[] = [];
  constructor(scene: Scene, private max = 60) { scene.add(this.group); }

  add(id: string, at: Vector3, shape: Shape, n: number, t0: number, scale: number) {
    const pts = topo(shape, Math.max(2, n));
    const R = 16 * scale * (0.8 + 0.12 * n);
    const H = 26 * scale;
    const verts: number[] = []; const along: number[] = [];
    const P = (x: number, z: number) => [at.x + x * R, at.y + H, at.z + z * R];
    let acc = 0;
    for (const [a, b] of pts.lines) {
      const A = P(pts.v[a][0], pts.v[a][1]); const B = P(pts.v[b][0], pts.v[b][1]);
      verts.push(...A, ...B); along.push(acc, acc + 1); acc += 1;
    }
    // 外圈
    const segs = 48;
    for (let i = 0; i < segs; i++) {
      const a0 = (i / segs) * Math.PI * 2, a1 = ((i + 1) / segs) * Math.PI * 2;
      verts.push(...P(Math.cos(a0) * 1.25, Math.sin(a0) * 1.25), ...P(Math.cos(a1) * 1.25, Math.sin(a1) * 1.25));
      along.push(acc * (i / segs), acc * ((i + 1) / segs));
    }
    // 投影光：港到蓝图
    for (const off of [-0.18, 0.18]) {
      verts.push(at.x + off * 2, at.y, at.z, at.x + off * R, at.y + H, at.z); along.push(-1, -1);
    }
    const g = new BufferGeometry();
    g.setAttribute('position', new BufferAttribute(new Float32Array(verts), 3));
    g.setAttribute('aAlong', new BufferAttribute(new Float32Array(along), 1));
    const mat = new ShaderMaterial({
      uniforms: { ...shared, cBlue: col(C.blueprint), uT0: { value: t0 }, uTotal: { value: Math.max(1, acc) }, uDis: { value: -1 }, uDraw: { value: T.blueprintDraw } },
      vertexShader: GLSL_COMMON + /* glsl */ `
        attribute float aAlong; varying float vAlong; varying float vFog;
        void main() { vAlong = aAlong; vec4 mv = modelViewMatrix * vec4(position, 1.0); vFog = fogK(-mv.z); gl_Position = projectionMatrix * mv; }`,
      fragmentShader: GLSL_COMMON + /* glsl */ `
        uniform vec3 cBlue; uniform float uT0; uniform float uTotal; uniform float uDis; uniform float uDraw;
        varying float vAlong; varying float vFog;
        void main() {
          float age = uTime - uT0;
          float prog = age / uDraw * uTotal;
          float a;
          if (vAlong < 0.0) a = 0.18 * smoothstep(0.0, 0.6, age);
          else { if (vAlong > prog) discard; a = 0.95 + 1.5 * exp(-(prog - vAlong) * 2.0); }
          // 定案后几秒降到常驻亮度
          // 构型多时常驻的蓝图会连成一片冷白，压到 12%：冷白只属于开垦那一刻
          a *= mix(1.0, 0.12, smoothstep(9.0, 14.0, age));
          if (uDis > 0.0) a *= 1.0 - clamp((uTime - uDis) / 1.5, 0.0, 1.0);
          gl_FragColor = vec4(cBlue * a * vFog, 1.0);
        }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    const obj = new LineSegments(g, mat); obj.frustumCulled = false; obj.name = id; obj.renderOrder = 4;
    this.group.add(obj);
    this.items.push({ obj, mat, t0, dissolved: -1 });
    while (this.items.length > this.max) { const it = this.items.shift()!; this.group.remove(it.obj); it.obj.geometry.dispose(); it.mat.dispose(); }
  }

  dissolve(id: string, now: number) {
    const it = this.items.find((x) => x.obj.name === id);
    if (it) { it.mat.uniforms.uDis.value = now; it.dissolved = now; }
  }
}

function topo(shape: Shape, n: number): { v: [number, number][]; lines: [number, number][] } {
  const v: [number, number][] = []; const lines: [number, number][] = [];
  const circle = (k: number, r = 1, ph = -Math.PI / 2) => { for (let i = 0; i < k; i++) v.push([Math.cos(ph + (i / k) * Math.PI * 2) * r, Math.sin(ph + (i / k) * Math.PI * 2) * r]); };
  switch (shape) {
    case 'pair': v.push([-0.8, 0], [0.8, 0]); lines.push([0, 1]); break;
    case 'relay': v.push([-0.9, 0], [0, 0], [0.9, 0.0]); lines.push([0, 1], [1, 2]); break;
    case 'chain': for (let i = 0; i < n; i++) v.push([-0.9 + (1.8 * i) / (n - 1), (i % 2 ? 0.25 : -0.25)]); for (let i = 0; i < n - 1; i++) lines.push([i, i + 1]); break;
    case 'ring': circle(n, 0.85); for (let i = 0; i < n; i++) lines.push([i, (i + 1) % n]); break;
    case 'team': circle(n, 0.8); v.push([0, 0]); for (let i = 0; i < n; i++) lines.push([i, n]); break;
    case 'star': v.push([0, 0]); circle(n - 1, 0.9); for (let i = 1; i < n; i++) lines.push([0, i]); break;
    case 'm2m': { const h = Math.ceil(n / 2); for (let i = 0; i < h; i++) v.push([-0.6, -0.7 + (1.4 * i) / Math.max(1, h - 1)]); for (let i = 0; i < n - h; i++) v.push([0.6, -0.7 + (1.4 * i) / Math.max(1, n - h - 1)]); for (let i = 0; i < h; i++) for (let j = h; j < n; j++) lines.push([i, j]); break; }
    case 'meta': circle(3, 0.35, 0); v.push([0.9, 0]); lines.push([0, 1], [1, 2], [2, 0], [0, 3]); circle(12, 0.5); for (let i = 0; i < 12; i += 2) lines.push([4 + i, 4 + i + 1]); break;
  }
  return { v, lines };
}
