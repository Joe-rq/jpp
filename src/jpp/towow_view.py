"""Self-contained visualization of an executed trace, not a simulated live UI."""
import json
from pathlib import Path


def write_view(report, path):
    data = json.dumps(report, ensure_ascii=False).replace("<", "\\u003c")
    Path(path).write_text(TEMPLATE.replace("__REPORT__", data), encoding="utf-8")


TEMPLATE = r'''<!doctype html>
<html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>J++ · 当一个模糊意图遇见十个人</title>
<style>
:root{font-family:system-ui,-apple-system,sans-serif;color:#e8eef4;background:#101820;color-scheme:dark}*{box-sizing:border-box}body{margin:0}main{max-width:1180px;margin:auto;padding:42px 28px}a{color:#83e5c5}.eyebrow{color:#83e5c5;letter-spacing:.16em;font-size:12px}h1{font-size:clamp(25px,4vw,40px);font-weight:620;letter-spacing:-.04em;margin:14px 0}p{line-height:1.8;color:#bac7d2}.signal{border-left:3px solid #83e5c5;padding:14px 22px;background:#17232d;font-size:19px;margin:26px 0}button{cursor:pointer;font:inherit;border:1px solid #384b5b;background:transparent;color:#b9cbd9;border-radius:7px;padding:11px 16px}button.active{background:#83e5c5;color:#10261f;border-color:#83e5c5}nav{display:flex;gap:8px;flex-wrap:wrap;margin:24px 0}.grid{display:grid;grid-template-columns:1.8fr 1fr;gap:20px}.panel{background:#17232d;border:1px solid #293c4a;border-radius:12px;padding:20px;min-width:0}svg{width:100%;height:auto}svg text{font-size:13px;fill:#dde8ef}svg .node{cursor:pointer}svg .node:hover circle{stroke:#fff;stroke-width:3}h2{font-size:18px;margin-top:0}.small{font-size:13px;color:#8fa4b5;line-height:1.8}.metrics{display:flex;gap:25px;flex-wrap:wrap;margin:20px 0}.metric strong{font-size:25px;font-weight:500;color:#83e5c5}.metric span{display:block;color:#9eb0bf;font-size:12px}.evidence{margin-top:14px;padding:12px;background:#101820;border-radius:6px}#details{white-space:pre-wrap;line-height:1.8;font-size:14px}footer{margin-top:26px}.legend{display:flex;gap:20px;font-size:12px;color:#a2b5c4}.dot{display:inline-block;width:8px;height:8px;border-radius:50%;background:#83e5c5;margin-right:5px} @media(max-width:780px){.grid{grid-template-columns:1fr}main{padding:25px 16px}}
</style><main>
<div class="eyebrow">J++ / FIRST APPLICATION QUESTION / TOWOW</div>
<h1>一个模糊意图，怎样长出合作可能？</h1>
<p>十个虚构主体。每次判断只结合当前材料与一个接收方的本地上下文。点击阶段，查看实际程序留下的计算轨迹。</p>
<div id="mode" class="small"></div><div class="signal" id="signal"></div>
<nav id="steps"></nav>
<div class="grid"><section class="panel"><h2 id="stage"></h2><svg id="network" viewBox="0 0 650 475" role="img" aria-label="十人候选关系网络"></svg><div class="legend"><span><i class="dot"></i>当前候选</span><span>虚线：转介</span><span>点击节点：查看原始上下文</span></div></section>
<section class="panel"><h2 id="detail-title">计算留下了什么？</h2><div id="details"></div></section></div>
<div class="metrics" id="metrics"></div><p class="small" id="benchmark"></p>
<footer class="small">这是一次记录的逐阶段查看，不会连接模型或联系参与者。所有提名使用尚未校准的 provisional 判断；关系线表示进一步交流的候选，不表示成交。程序先完成一层判断再进入下一层，尚未实现任意中间结果的持续订阅。<br><a href="https://github.com/Towow-ai/jpp">GitHub · 源码、构想与运行方法</a></footer>
</main><script>
const report=__REPORT__;
const people=report.scenario.people, names=Object.fromEntries(people.map(p=>[p.id,p.name]));
const positions={lin:[85,238],mei:[255,70],lan:[255,230],zhou:[405,230],qiao:[540,60],tang:[555,145],an:[555,320],he:[530,415],yu:[270,410],bo:[120,405]};
const stages=[...report.cold.stages,report.updated.stages.at(-1)];
const labels=['1 · 意图遇见上下文','2 · 转介与三方构型','3 · 构型继续发现','4 · 局部变化与复用'];
const $=id=>document.getElementById(id), ns='http://www.w3.org/2000/svg';
let selected=0;
function svg(tag,attrs,text){const e=document.createElementNS(ns,tag);for(const [k,v] of Object.entries(attrs))e.setAttribute(k,v);if(text)e.textContent=text;return e}
$('signal').textContent='“'+report.scenario.signal+'”';
$('mode').textContent=report.mode+' · '+report.model+' · '+report.created_utc;
labels.forEach((label,i)=>{const b=document.createElement('button');b.textContent=label;b.onclick=()=>show(i);$('steps').append(b)});
function edge(a,b,dash=false,unknown=false){const [x1,y1]=positions[a], [x2,y2]=positions[b];$('network').append(svg('line',{x1,y1,x2,y2,stroke:unknown?'#8698a8':'#83e5c5','stroke-opacity':.65,'stroke-width':2,...(dash||unknown?{'stroke-dasharray':'5 5'}:{})}))}
function show(i){selected=i;const s=stages[i];[...$('steps').children].forEach((b,j)=>b.classList.toggle('active',i===j));$('stage').textContent=i===3?'只有司机的可用性变了':s.name;$('network').replaceChildren();const active=new Set(['lin']);
for(const d of s.direct){active.add(d.person);if(!d.via.length)edge('lin',d.person)}
for(const r of s.relays){active.add(r.via);active.add(r.target);edge('lin',r.via);edge(r.via,r.target,true)}
for(const e of s.extensions){const c=s.candidates.find(c=>c.id===e.candidate);if(e.relation===true)active.add(e.person);edge(c.members.at(-1),e.person,false,e.relation===null)}
for(const p of people){const [x,y]=positions[p.id];const g=svg('g',{class:'node',tabindex:0,role:'button','aria-label':p.name});g.append(svg('circle',{cx:x,cy:y,r:23,fill:active.has(p.id)?'#244f47':'#20303c',stroke:active.has(p.id)?'#83e5c5':'#4c6070'}));g.append(svg('text',{x,y:y+5,'text-anchor':'middle'},p.name.split(' · ')[0]));g.append(svg('text',{x,y:y+43,'text-anchor':'middle'},p.name.split(' · ')[1]));g.onclick=()=>detail(p,i);g.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();detail(p,i)}};$('network').append(g)}
$('detail-title').textContent='计算留下了什么？';
const lines=['直接支持候选：'+(s.direct.map(d=>names[d.person]).join('、')||'尚无'), '转介：'+(s.relays.map(r=>names[r.via]+' → '+names[r.target]).join('；')||'尚无')];
for(const c of s.candidates)lines.push('中间构型：'+c.members.map(m=>names[m].split(' · ')[0]).join(' + ')+'\n司机可用性：'+String(c.availability));
if(s.extensions.length)lines.push('构型的后续候选：\n'+s.extensions.map(e=>names[e.person]+(e.relation===null?' · 相关性未确定':e.discuss_now===true?' · 可讨论小规模尝试':' · 相关，但可用条件待补')).join('\n'));
if(i===3)lines.push('发送者与其他人的文本没有变化。程序重走同一方法，已有判断由 J++ 账本复用；变化后的司机材料和它形成的构型重新计算。');
$('details').textContent=lines.join('\n\n');}
function detail(p,i){$('detail-title').textContent=p.name;const run=i===3?report.updated:report.cold;const context=i===3&&p.id===report.scenario.update.person?report.scenario.update.context:p.context;const stageKinds=i===0?['direct','relay']:i===1?['direct','relay','ready']:['direct','relay','ready','extend','activate'];const obs=run.observations.filter(o=>o.person===p.id&&stageKinds.includes(o.kind));$('details').textContent=context+'\n\n'+obs.map(o=>o.kind+': '+(o.value===null?'未知':o.value?'候选成立':'未提名')+' · provisional='+o.provisional+'\n'+o.question).join('\n\n')}
for(const [label,key] of [['首次运行','cold'],['相同输入复用','warm'],['司机更新后','updated']]){const r=report[key],box=document.createElement('div');box.className='metric';const number=document.createElement('strong');number.textContent=(r.elapsed_ms/1000).toFixed(3)+' s';const caption=document.createElement('span');caption.textContent=label+' · '+r.stats.calls+' 次后端请求 · '+r.stats.ledger_hits+' 次账本命中';box.append(number,caption);$('metrics').append(box)}
if(report.benchmark){const b=report.benchmark;$('benchmark').textContent='同一首层请求集合，真实顺序执行 '+(b.sequential.elapsed_ms/1000).toFixed(3)+' s，并发执行 '+(b.parallel.elapsed_ms/1000).toFixed(3)+' s。本次观测 '+b.speedup+'×；单次测量，服务端缓存与网络未控制。'}
show(0);
</script></html>'''
