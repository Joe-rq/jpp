// 与 DESIGN.md 一一对应的常量。改这里之前先改 DESIGN.md。
import { Color } from 'three';

export const HEX = {
  sea: '#05080C', fog: '#070C12', land: '#1B2229', sodium: '#FFB35C', sodiumHi: '#FFE2B0',
  flight: '#C9D3DC', vermilion: '#FF5A36', blueprint: '#D6E6FF', ash: '#46505A', text: '#EDE6DA',
} as const;

export const C = Object.fromEntries(Object.entries(HEX).map(([k, v]) => [k, new Color(v)])) as Record<keyof typeof HEX, Color>;

export const T = {
  joinFall: 2.4, ripple: 0.9, flightMin: 0.9, flightMax: 1.6, arcK: 0.22,
  flashAct: 0.45, flashIgnore: 0.3, flashUnsure: 0.6, signal: 1.2, reply: 1.0,
  edgeFade: 0.6, goneTrace: 90, fleet: 3.2, ignite: 0.7, blueprintDraw: 1.4,
  captionIn: 0.5, captionHold: 3.2, captionOut: 0.9,
};

export const CAM = {
  cruiseElev: 52, cruiseAzSpeed: 1.5, pushDist: [160, 240] as const, orbitR: 180, orbitAzSpeed: 6,
  omegaDirector: 1.4, omegaReturn: 1.0, minShot: 5, maxShot: 11, idleReturn: 6,
};

// 闪光类型（写进港灯的 aFlash.y）
export const FLASH = { act: 1, ignore: 2, unsure: 3, invalidate: 4, member: 5, disclose: 6, ignite: 7 } as const;
// 航班类型
export const FLY = { probe: 0, act: 1, signal: 2, granted: 3, denied: 4, fleet: 5, join: 6 } as const;
// 涟漪类型
export const RING = { join: 0, ignite: 1, member: 2, leave: 3 } as const;
