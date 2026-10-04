import { Color, Vector4 } from 'three';

// 所有自定义着色器共享的 uniform：时间、像素比例、雾。
export const shared = {
  uTime: { value: 0 },
  uPxScale: { value: 600 }, // 每单位世界长度在 1 单位视深处对应的像素数
  uFogNear: { value: 900 },
  uFogFar: { value: 3200 },
  uPulse: { value: new Vector4(0, 0, -1000, 300) }, // 全网脉冲：中心 x、z，起始时间，速度
};

export const GLSL_COMMON = /* glsl */ `
uniform float uTime;
uniform float uPxScale;
uniform float uFogNear;
uniform float uFogFar;
uniform vec4 uPulse;
// 全网唯一的一次破例：构型再组合时从新港向外扩散的一道光
float pulseAt(vec3 p) {
  float age = uTime - uPulse.z; if (age < 0.0 || age > 6.0) return 0.0;
  float r = age * uPulse.w; float d = length(p.xz - uPulse.xy);
  return exp(-abs(d - r) / (uPulse.w * 0.08)) * (1.0 - age / 6.0);
}
float fogK(float depth) { return 1.0 - smoothstep(uFogNear, uFogFar, depth); }
float easeOut(float x) { x = clamp(x, 0.0, 1.0); return 1.0 - pow(1.0 - x, 4.0); }
float easeInOut(float x) { x = clamp(x, 0.0, 1.0); return x * x * (3.0 - 2.0 * x); }
`;

export function col(c: Color) { return { value: c.clone() }; }

import type { BufferAttribute, InstancedBufferAttribute } from 'three';
/** 只上传改过的槽位；改得多时整段上传更划算。 */
export function uploadSlots(attr: BufferAttribute | InstancedBufferAttribute, slots: Set<number>, full: boolean) {
  attr.clearUpdateRanges();
  if (!full && slots.size <= 96) {
    const s = attr.itemSize;
    for (const i of slots) attr.addUpdateRange(i * s, s);
  }
  attr.needsUpdate = true;
}
