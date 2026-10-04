// 录一段回放：snapshot + 若干秒增量，写成 .jsonl。用法：npm run mock:record -- out.jsonl [--n 500] [--sec 120]
import { writeFileSync } from 'node:fs';
import { MockNetwork } from '../src/mock/generator';

const out = process.argv[2] || 'public/replay.jsonl';
const arg = (k: string, d: number) => { const i = process.argv.indexOf(k); return i >= 0 ? Number(process.argv[i + 1]) : d; };
const net = new MockNetwork({ n: arg('--n', 500) });
const lines = [JSON.stringify(net.snapshot())];
const sec = arg('--sec', 120);
for (let i = 0; i < sec * 20; i++) for (const e of net.step(0.05)) lines.push(JSON.stringify(e));
writeFileSync(out, lines.join('\n') + '\n');
console.log(`写出 ${out}：${lines.length} 条事件，${sec} 秒`);
