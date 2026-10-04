// 布局：城市是群岛，agent 是岛上的港。有语义坐标（vec3）时直接用它。
import { Vector3 } from 'three';

const UP = new Vector3(0, 1, 0);

export interface Island { name: string; c: Vector3; r: number; idx: number }

export function hash(str: string): number {
  let h = 2166136261;
  for (let i = 0; i < str.length; i++) { h ^= str.charCodeAt(i); h = Math.imul(h, 16777619); }
  return (h >>> 0) / 4294967296;
}
function hash2(str: string, salt: number) { return hash(str + '#' + salt); }

export class Layout {
  readonly D: number; // 群岛盘半径
  readonly islandR: number;
  islands = new Map<string, Island>();
  private count = 0;
  onIsland?: (isl: Island) => void;
  onLand?: (p: Vector3) => void;

  constructor(nExpected: number) {
    const k = Math.pow(Math.max(nExpected, 200) / 500, 0.3);
    this.D = 620 * k;
    this.islandR = 62 * k;
    this.R0 = this.D;
  }

  /** vec3 的比例：x/z 半径的中位数落在 0.42 倍群岛盘半径。null 表示还没见过 vec3。 */
  vecScale: number | null = null;
  calibrate(vs: [number, number, number][]) {
    if (this.vecScale !== null || !vs.length) return;
    const rs = vs.map((v) => Math.hypot(v[0], v[2])).filter((r) => r > 0).sort((a, b) => a - b);
    if (!rs.length) return;
    // 人少时群岛收拢：几十个 agent 摊在给 500 人准备的盘子上，地面连不成岛，读起来像星图
    this.R0 = this.D * Math.min(1, Math.max(0.3, Math.sqrt(Math.max(vs.length, 40) / 500)));
    this.vecScale = (this.R0 * 0.42) / rs[rs.length >> 1];
  }
  /** 语义坐标下的有效半径：定比例时按人数给一个初值，之后随港的实际铺开范围增长，不超过 D。 */
  R0: number;
  private maxR = 0;
  get R() { return Math.min(this.D, Math.max(this.R0, this.maxR * 0.9)); }

  island(city: string): Island {
    let isl = this.islands.get(city);
    if (isl) return isl;
    const i = this.count++;
    // 黄金角螺旋排岛，越晚出现的城越靠外
    const ang = i * 2.39996 + hash(city) * 0.6;
    const rad = this.D * (0.12 + 0.86 * Math.sqrt((i + 0.5) / 24));
    const r = this.islandR * (0.75 + 0.6 * hash2(city, 1));
    isl = { name: city, c: new Vector3(Math.cos(ang) * rad, 0, Math.sin(ang) * rad), r, idx: i };
    this.islands.set(city, isl);
    this.onIsland?.(isl);
    return isl;
  }

  agentPos(id: string, city?: string | null, vec3?: [number, number, number]): Vector3 {
    if (vec3 && this.vecScale === null) this.calibrate([vec3]);
    if (vec3 && this.vecScale) {
      // 语义坐标：宿主给的是固定随机投影，量级任意。比例只定一次，之后不再改，已放好的港不动。
      // 取 x、z 两个分量铺在海面上；远处用 tanh 收拢，别让离群点飞出群岛盘。
      const x = vec3[0] * this.vecScale, z = vec3[2] * this.vecScale;
      const r = Math.hypot(x, z) || 1;
      const k = r > this.D * 0.7 ? (this.D * 0.7 + this.D * 0.3 * Math.tanh((r - this.D * 0.7) / (this.D * 0.3))) / r : 1;
      // 同一语义点上的人略微错开，免得叠成一个港
      const j = this.islandR * 0.25;
      const p = new Vector3(x * k + (hash2(id, 7) - 0.5) * j, 1.5, z * k + (hash2(id, 8) - 0.5) * j);
      this.maxR = Math.max(this.maxR, Math.hypot(p.x, p.z));
      this.onLand?.(p); return p;
    }
    const isl = this.island(city || '远海');
    const a = hash2(id, 2) * Math.PI * 2;
    const u = Math.sqrt(hash2(id, 3));
    const rr = isl.r * (0.08 + 0.95 * u);
    // 岛内再分几个港湾，避免均匀撒点
    const cove = Math.floor(hash2(id, 4) * 4);
    const ca = cove * 1.57 + hash(isl.name) * 6.28;
    const cx = Math.cos(ca) * isl.r * 0.35, cz = Math.sin(ca) * isl.r * 0.35;
    return new Vector3(isl.c.x + cx * 0.6 + Math.cos(a) * rr * 0.8, 1.5, isl.c.z + cz * 0.6 + Math.sin(a) * rr * 0.8);
  }

  /** 新港的位置：成员质心向外推到海面上，船队要驶过去。 */
  portPos(id: string, members: Vector3[]): Vector3 {
    const c = new Vector3();
    for (const m of members) c.add(m);
    c.multiplyScalar(1 / Math.max(1, members.length));
    // 离最近的岛心推开，落在海上
    let near: Island | undefined; let nd = Infinity;
    for (const isl of this.islands.values()) { const d = isl.c.distanceTo(c); if (d < nd) { nd = d; near = isl; } }
    const out = new Vector3();
    if (near && nd < near.r * 1.6) {
      out.subVectors(c, near.c); out.y = 0;
      if (out.lengthSq() < 1) out.set(Math.cos(hash(id) * 6.28), 0, Math.sin(hash(id) * 6.28));
      out.normalize().multiplyScalar(near.r * 1.6 - nd + 18 + hash2(id, 5) * 20);
    } else {
      // 语义坐标下没有岛：从成员质心朝外海方向推出一段，让船队有路可走
      out.set(c.x, 0, c.z); if (out.lengthSq() < 1) out.set(1, 0, 0);
      out.normalize().applyAxisAngle(UP, (hash(id) - 0.5) * 1.6).multiplyScalar(this.islandR * Math.sqrt(this.R0 / this.D) * (0.7 + 0.5 * hash2(id, 6)));
    }
    c.add(out); c.y = 2;
    return c;
  }
}
