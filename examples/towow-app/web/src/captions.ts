// 旁白字幕的英文版（首页 ?lang=en）。只翻模板句子；agent 标签、展示语、方案标题、披露类别这些来自数据的原文一律原样引用。
// 边的合作形式在 net.ts 里已被 formLabel 转成中文，这里按中文标签对回英文；不认识的（模拟源自带的中文）原样。
export const FORM_EN: Record<string, string> = {
  '直接互补': 'direct complement', '单向帮助': 'one-way help',
  '经 A 身边的人转介': 'referral through a contact of A', '经 B 身边的人转介': 'referral through a contact of B',
  '还需要第三方才成立': 'needs a third party', '不成立': 'no match', '关系': 'a relationship',
};
export const SHAPE_EN: Record<string, string> = {
  pair: 'complementary pair', relay: 'referral link', chain: 'chain', ring: 'mutual-aid ring', team: 'team',
  star: 'one-to-many group', m2m: 'many-to-many group', meta: 'larger group',
};
export const Q_EN: Record<string, string> = {
  open: 'whether they can help', catcher: 'the screening question', hold: 'whether these people can work together',
  timing: 'timing', value: 'value', dir: 'who helps whom', form: 'the form of cooperation',
  tighter: 'whether joining tightens it', disclose: 'whether to share',
};
export const formEn = (f: string) => FORM_EN[f] ?? f;
export const shapeEn = (s: string) => SHAPE_EN[s] ?? 'group';
export const plural = (n: number, w: string) => `${n.toLocaleString('en')} ${w}${n === 1 ? '' : 's'}`;
export const listEn = (xs: string[]) => xs.length < 2 ? xs.join('') : `${xs.slice(0, -1).join(', ')} and ${xs[xs.length - 1]}`;
export const cap1 = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
// 信息层（点开一座港后的面板）的英文词表
export const STATUS_EN: Record<string, string> = {
  sent: 'requested', granted: 'shared', denied: 'declined', escalate: 'escalated', drop: 'dropped', return: 'returned to caller',
  near_boundary: 'reading near the line', refine: 'refined, judged again',
};
export const TIER_EN = ['one public line', 'shared once there is a sign of fit', 'shared only after trust'];
export const VALUE_EN = ['small', 'medium', 'large'];
