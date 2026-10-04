// 事件协议 v0，与 ../design/events.md 一一对应。字段可增不可删。
// 类型照真后端（host/views.py）实际发出的样子写：gone 的边没有读数、fixture 下方案没有标题，都可能是 null。

export type Exit = 'act' | 'ignore' | 'unsure';
export type Shape = 'pair' | 'relay' | 'chain' | 'ring' | 'team' | 'star' | 'm2m' | 'meta';
export type ConfigStage = 'candidate' | 'judged' | 'growing' | 'plan' | 'dissolved';
/** host/views.py SPOTLIGHT_WHY 的固定枚举 */
export type Why = 'join_first_opp' | 'config_formed' | 'meta_formed' | 'plan_ready' | 'disclose_granted';

export interface NodeJoin {
  t: number; type: 'node_join';
  id: string; kind: 'agent' | 'config'; label: string;
  host_agent?: string | null; lang?: string | null; city?: string | null; tier?: number;
  vec3?: [number, number, number];
  // 构型节点：id 就是构型 id
  config?: string; members?: string[]; shape?: Shape | null; real?: boolean;
}
export interface NodeLeave { t: number; type: 'node_leave'; id: string }
/** tier 0/1/2 对应 t0/t1/t2；带 to 时是只向这一位对方解锁 */
export interface Disclose { t: number; type: 'disclose'; id: string; to?: string; tier: number; added_chars: number; reason: string }
export interface Probe { t: number; type: 'probe'; from: string; to: string[]; stage: 'recall' | 'operator' }
export interface Batch { t: number; type: 'batch'; id: string | number; n_states: number; n_questions: number; latency_ms: number; merged_from: number }
export interface Judge {
  t: number; type: 'judge'; a: string; b?: string | null; config?: string | null; q: string; p: number; exit: Exit; batch: string | number;
  /** select 的选项 / measure 的档位（value 题 0/1/2 = 小/中/大） */
  value?: unknown; cause?: unknown;
}
export interface UnsureRoute {
  t: number; type: 'unsure_route'; a: string; b?: string | null; config?: string | null;
  missing: string; ask_to: string | string[]; route: string; q?: string;
  /** band（读数落在拿不准的带里）/ tie / deadline / '' */
  cause?: string | null;
  /** 后端补上后优先用（见 NEEDS-BACKEND.md）：拿不准那道题的读数 */
  p?: number | null;
}
export interface DiscloseRequest {
  t: number; type: 'disclose_request'; id: string; to: string; from: string;
  category: string; purpose: string | null; status: 'sent' | 'granted' | 'denied';
}
export interface EdgeEv {
  t: number; type: 'edge'; a: string; b: string; dir: 'a>b' | 'b>a' | 'both' | null;
  form: string | null; conf: number | null; state: 'new' | 'up' | 'down' | 'gone';
  /** 后端补上后优先用（见 NEEDS-BACKEND.md）：决定性题的短名与题面、已看到的层、还缺什么 */
  q?: string | null; q_text?: string | null; tier_seen?: string[] | null; lacks?: string[] | null;
}
export interface ConfigEv {
  t: number; type: 'config'; id: string; shape: Shape | null; members: string[];
  roles: Record<string, string>; conf: number | null; stage: ConfigStage;
  plan_title?: string | null;
}
export interface ConfigGrow { t: number; type: 'config_grow'; id: string; add: string | null; tighter_p: number | null }
export interface PlanEv { t: number; type: 'plan'; config: string; title: string | null; summary: string | null; conf: number | null }
export interface Invalidate { t: number; type: 'invalidate'; cause: 'disclose' | 'join' | 'leave'; n_judgments: number; ids?: string[]; id?: string }
export interface StatsFields {
  agents?: number; configs?: number; calls?: number; questions?: number;
  cost_usd?: number; qps?: number; cache_hit?: number; p50_join_s?: number;
}
export interface Stats extends StatsFields { t: number; type: 'stats' }
/** 回放文件首行可选的说明：数据来自哪里（真机 JEV / 离线伪读数 / 模拟） */
export interface Meta { t: number; type: 'meta'; source?: 'jev' | 'real' | 'fixture' | 'mock' | string; note?: string; [k: string]: unknown }
export interface Spotlight { t: number; type: 'spotlight'; id: string; why: Why | string; to?: string }
export type SnapEdge = Omit<EdgeEv, 't' | 'type' | 'state'> & { state?: EdgeEv['state'] };
export interface Snapshot {
  t: number; type: 'snapshot';
  nodes: Omit<NodeJoin, 't' | 'type'>[];
  edges: SnapEdge[];
  configs: Omit<ConfigEv, 't' | 'type'>[];
  stats: StatsFields;
}

export type NetEvent =
  | Snapshot | NodeJoin | NodeLeave | Disclose | Probe | Batch | Judge | UnsureRoute
  | DiscloseRequest | EdgeEv | ConfigEv | ConfigGrow | PlanEv | Invalidate | Stats | Spotlight | Meta;

export const SHAPE_NAME: Record<Shape, string> = {
  pair: '互补', relay: '转介', chain: '链', ring: '互助环', team: '团队',
  star: '一对多', m2m: '多对多', meta: '再组合',
};

/** 边的合作形式：宿主发的是代码（views.FORM_LABEL），模拟源发的是中文 */
export const FORM_LABEL: Record<string, string> = {
  direct: '直接互补', oneway: '单向帮助', relay_a: '经 A 身边的人转介',
  relay_b: '经 B 身边的人转介', third: '还需要第三方才成立', none: '不成立',
};
export const formLabel = (f: string | null | undefined) => (f ? FORM_LABEL[f] ?? f : '关系');

/** 披露层：t0/t1/t2 */
export const TIER_NAMES = ['公开的一句话', '有苗头后愿给的', '互信后才给的'];
export const MAX_TIER = 2;

/** 题的短名 → 题面（app/net.jpx 的原文；open/catcher 有两个方向或逐条确认题，这里写通用的说法） */
export const Q_TEXT: Record<string, string> = {
  open: '读对方的世界，对方（或对方身边的人）能对这一方现在的状态或需要做点实在的事吗？',
  catcher: '这一方事先写了一道确认题，问来信的人是不是自己接得住的那种人；就对方的情况回答。',
  hold: '这几个人按这个形状合作，能成吗？',
  dir: '如果他们合作，谁主要帮谁？',
  form: '最可能的合作形式是哪一种？',
  value: '如果合作成了，对双方的价值有多大？',
  timing: '双方现在的时间、阶段与条件对得上吗？',
  'neg-same': '他们其实在找同一种人或同一种资源吗？',
  'neg-solved': '对方的问题其实已经解决了吗？',
  'neg-values': '他们的价值观或底线有冲突吗？',
  tighter: '他加入会让这个合作更紧密吗？',
  disclose: '把这一类信息给这位对方，落在主人愿意给的范围内、并且会推进这桩合作吗？',
  weakest: '最弱的一环是谁？',
  without: '去掉其中一位之后，这个合作还成立吗？',
};
/** 题的短名 → 一个词，用在字幕和故事线里 */
export const Q_WORD: Record<string, string> = {
  open: '能不能帮上忙', catcher: '确认题', hold: '这几个人能不能成', timing: '时机', value: '价值',
  dir: '谁帮谁', form: '合作形式', tighter: '加入后更紧密吗', disclose: '给不给',
};
export const VALUE_LEVELS = ['小', '中', '大'];
/** 未决去向（unsure_route.route） */
export const ROUTE_WORD: Record<string, string> = {
  disclose_request: '去要信息', near_boundary: '读数贴着判断线', refine: '细化后再判', escalate: '升级判断', drop: '放下', return: '退回调用方',
};
