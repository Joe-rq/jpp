// 群岛的地面点阵与夜海。只在新城市出现时写一次缓冲区，平时不动。
import {
  AdditiveBlending, BufferAttribute, BufferGeometry, Mesh, PlaneGeometry, Points, ShaderMaterial, Scene,
} from 'three';
import type { Island } from '../layout';
import { hash } from '../layout';
import { C } from '../style';
import { GLSL_COMMON, shared, col } from './common';

const MAX_ISLANDS = 48;

export class Land {
  points: Points;
  private pos: Float32Array; private h: Float32Array;
  private used = 0; private readonly cap: number;
  private sea: Mesh;

  constructor(scene: Scene, private D: number) {
    const per = 6500;
    this.cap = MAX_ISLANDS * per;
    this.pos = new Float32Array(this.cap * 3);
    this.h = new Float32Array(this.cap);
    const g = new BufferGeometry();
    g.setAttribute('position', new BufferAttribute(this.pos, 3));
    g.setAttribute('aH', new BufferAttribute(this.h, 1));
    g.setDrawRange(0, 0);
    const m = new ShaderMaterial({
      uniforms: { ...shared, uLand: col(C.land) },
      vertexShader: GLSL_COMMON + /* glsl */ `
        attribute float aH; varying float vA; varying float vH; varying float vSize;
        void main() {
          vec4 mv = modelViewMatrix * vec4(position, 1.0);
          gl_Position = projectionMatrix * mv;
          // 每个地面点是一小块柔光地皮，加一粒 1 像素的颗粒；近看连成陆地，远看是颗粒
          float px = 5.5 * uPxScale / -mv.z;
          vSize = clamp(px, 2.0, 56.0);
          gl_PointSize = vSize;
          vA = fogK(-mv.z);
          vH = aH;
        }`,
      fragmentShader: /* glsl */ `
        uniform vec3 uLand; varying float vA; varying float vH; varying float vSize;
        void main() {
          vec2 d = gl_PointCoord - 0.5; float r = length(d) * 2.0; if (r > 1.0) discard;
          float haze = exp(-r * r * 3.0) * (1.0 - r) * 0.15;
          float grain = smoothstep(1.6 / vSize, 0.0, r) * 0.5;
          vec3 c = uLand * (2.4 + 2.6 * vH);
          gl_FragColor = vec4(c * (haze + grain) * vA, 1.0);
        }`,
      transparent: true, depthWrite: false, blending: AdditiveBlending,
    });
    this.points = new Points(g, m);
    this.points.frustumCulled = false;
    scene.add(this.points);

    // 夜海：一块很暗的平面，群岛下方略亮，向远处沉入黑色
    const sg = new PlaneGeometry(D * 8, D * 8, 1, 1);
    sg.rotateX(-Math.PI / 2);
    const sm = new ShaderMaterial({
      uniforms: { ...shared, uSea: col(C.sea), uFog: col(C.fog), uD: { value: D } },
      vertexShader: GLSL_COMMON + /* glsl */ `
        varying vec3 vW; varying float vDepth;
        void main() { vec4 w = modelMatrix * vec4(position, 1.0); vW = w.xyz; vec4 mv = viewMatrix * w; vDepth = -mv.z; gl_Position = projectionMatrix * mv; }`,
      fragmentShader: GLSL_COMMON + /* glsl */ `
        uniform vec3 uSea; uniform vec3 uFog; uniform float uD; varying vec3 vW; varying float vDepth;
        void main() {
          float r = length(vW.xz) / uD;
          float glow = exp(-r * r * 1.4) * 0.45;
          vec3 c = mix(uSea, uFog * 1.6, glow);
          // 极淡的长涌浪，只在近处可见
          float swell = sin(vW.x * 0.018) * sin(vW.z * 0.014); // 静止的涌浪纹理：没有事件就不动
          c += vec3(0.003, 0.0045, 0.006) * swell * fogK(vDepth * 2.0);
          gl_FragColor = vec4(c, 1.0);
        }`,
      depthWrite: false,
    });
    this.sea = new Mesh(sg, sm);
    this.sea.position.y = -0.5;
    this.sea.renderOrder = -10;
    scene.add(this.sea);
  }

  /** 语义坐标下：人聚在哪里，哪里就长出陆地。 */
  addAround(p: { x: number; y: number; z: number }, count: number, r: number) {
    for (let k = 0; k < count && this.used < this.cap; k++) {
      const a = Math.random() * Math.PI * 2; const rho = Math.pow(Math.random(), 0.7) * r;
      const i = this.used++;
      const h = 1 - rho / r;
      this.pos[i * 3] = p.x + Math.cos(a) * rho; this.pos[i * 3 + 1] = h * 2.5; this.pos[i * 3 + 2] = p.z + Math.sin(a) * rho;
      this.h[i] = h * 0.8;
    }
    this.dirty = true;
  }
  private dirty = false;
  flush() {
    if (!this.dirty) return; this.dirty = false;
    const g = this.points.geometry;
    (g.getAttribute('position') as BufferAttribute).needsUpdate = true;
    (g.getAttribute('aH') as BufferAttribute).needsUpdate = true;
    g.setDrawRange(0, this.used);
  }

  addIsland(isl: Island) {
    const want = Math.min(4000, Math.round(1500 * Math.pow(isl.r / 62, 1.25)));
    const seed = hash(isl.name);
    const ph = [seed * 6.28, seed * 17.1, seed * 31.7, seed * 5.3];
    let made = 0, tries = 0;
    while (made < want && tries < want * 4 && this.used < this.cap) {
      tries++;
      const a = Math.random() * Math.PI * 2;
      const rho = Math.sqrt(Math.random()) * isl.r * 1.55;
      const edge = isl.r * (1.12 + 0.22 * Math.sin(3 * a + ph[0]) + 0.12 * Math.sin(7 * a + ph[1]) + 0.07 * Math.sin(13 * a + ph[2]));
      if (rho > edge) continue;
      const h = Math.pow(1 - rho / edge, 1.4);
      const i = this.used++;
      this.pos[i * 3] = isl.c.x + Math.cos(a) * rho;
      this.pos[i * 3 + 1] = h * 3.5;
      this.pos[i * 3 + 2] = isl.c.z + Math.sin(a) * rho;
      this.h[i] = h;
      made++;
    }
    const g = this.points.geometry;
    (g.getAttribute('position') as BufferAttribute).needsUpdate = true;
    (g.getAttribute('aH') as BufferAttribute).needsUpdate = true;
    g.setDrawRange(0, this.used);
  }
}
