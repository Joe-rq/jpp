const $ = id => document.getElementById(id);
const NS = 'http://www.w3.org/2000/svg';
let colors = {};
let result, population, positions, people, stage = 2, selected, timers = [], cases=[];
const element = (tag, text, className) => { const e = document.createElement(tag); if (text) e.textContent = text; if (className) e.className = className; return e; };
const svg = (tag, attrs) => { const e = document.createElementNS(NS, tag); for (const [k,v] of Object.entries(attrs)) e.setAttribute(k,v); return e; };
const read = async url => { const r = await fetch(url); if (!r.ok) throw new Error(`无法读取 ${url}`); return r.json(); };
const name = p => `${p.name || p.id} · ${p.role || '合成主体'}`;
function gradeText(grade) { return grade.level == null ? '判断未决，保留为待核对候选' : `暂定判断：${grade.label || grade.level}（仍需本人确认）`; }
function paragraph(parent, text, className) { parent.append(element('p', text, className)); }
function showPerson(person, role) {
  selected = person.id;
  $('detail-title').textContent = name(person);
  const box = $('detail'); box.replaceChildren();
  if (role) paragraph(box, `本轮检查的贡献：${role}`, 'badge');
  paragraph(box, person.context, 'member-context');
  renderGraph();
}
function showProposal(proposal) {
  selected = proposal.id;
  $('detail-title').textContent = `提议 ${proposal.order || result.proposals.indexOf(proposal)+1}：${proposal.members.length} 位成员怎样相遇`;
  const box = $('detail'); box.replaceChildren();
  paragraph(box, gradeText(proposal.grade), 'badge');
  for (const member of proposal.members) {
    box.append(element('h3', `${member.person.name || member.person.id} · ${member.contribution_role}`));
    paragraph(box, member.person.context, 'member-context');
  }
  box.append(element('h3', '开始前还需要确认'));
  paragraph(box, proposal.missing_conditions.join('；'));
  paragraph(box, proposal.next_question);
  renderGraph();
}
function renderGraph() {
  const graph = $('network'); graph.replaceChildren();
  const center = {x:380,y:285};
  const memberships = new Map();
  for(const member of result.input.seed?.members||[]) memberships.set(member.node.id,['prior']);
  for (const facet of result.input.facets) for (const row of result.candidates[facet.id]) {
    const current = memberships.get(row.person.id) || []; current.push(facet.id); memberships.set(row.person.id,current);
    if (stage >= 1) { const p = positions.get(row.person.id); graph.append(svg('line',{x1:center.x,y1:center.y,x2:p.x,y2:p.y,stroke:colors[facet.id] || '#70d6b5',opacity:.32,'stroke-width':1,class:'graph-edge'})); }
  }
  if (stage >= 2) for (const proposal of result.proposals) {
    const [a,...rest] = proposal.members.map(m=>positions.get(m.person.id)).filter(Boolean);
    for(const b of rest) graph.append(svg('line',{x1:a.x,y1:a.y,x2:b.x,y2:b.y,stroke:'#e6eff0',opacity:selected===proposal.id?1:.5,'stroke-width':selected===proposal.id?4:2,class:'graph-edge'}));
  }
  for (const person of population.people) {
    const p = positions.get(person.id), facets = memberships.get(person.id), lit = stage >= 1 && facets;
    const group = svg('g',{class:'person',role:'button',tabindex:'0','aria-label':name(person)});
    group.append(svg('circle',{cx:p.x,cy:p.y,r:selected===person.id?8:lit?6:2.2,fill:lit?(colors[facets[0]]||'#70d6b5'):'#29414d',class:lit?'node-lit':''}));
    if (lit) { const text = svg('text',{x:p.x+(p.x<380?-9:9),y:p.y-7,'text-anchor':p.x<380?'end':'start'}); text.textContent=person.name||person.id; group.append(text); }
    group.onclick=()=>showPerson(person,facets?.map(f=>result.input.facets.find(x=>x.id===f)?.label||'之前组合的成员').join('、'));
    group.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();group.onclick();}};
    graph.append(group);
  }
  graph.append(svg('circle',{cx:center.x,cy:center.y,r:44,fill:'#102c37',stroke:'#8ca8b0','stroke-width':1.5}));
  for (const [text,y] of [['当前输入',281],['意图 / 组合',300]]) { const label=svg('text',{x:380,y,'text-anchor':'middle'});label.textContent=text;graph.append(label); }
  $('stage').textContent = stage===0?'输入：当前意图及已有上下文。':stage===1?'第一轮：读取局部候选的资料，判断可以贡献什么。':'第二轮：组合成员与分工，继续判断合作提议。';
  $('show-people').classList.toggle('active',stage===1);$('show-teams').classList.toggle('active',stage===2);
}
function renderList() {
  const list=$('proposals');list.replaceChildren();$('list-title').textContent=stage===2?'接受下一轮判断的合作提议':'不同贡献的候选';
  if(stage===2) {
    for(const proposal of result.proposals) {const card=element('div',null,'proposal');card.append(element('h3',proposal.members.map(m=>m.person.name||m.person.id).join(' + ')));paragraph(card,proposal.members.map(m=>`${m.person.name||m.person.id}：${m.contribution_role}`).join('；'));paragraph(card,gradeText(proposal.grade),'badge');paragraph(card,`下一问：${proposal.next_question}`);const button=element('button','查看成员资料和待确认条件');button.onclick=()=>showProposal(proposal);card.append(button);list.append(card);}
    if(!result.proposals.length)paragraph(list,'本轮没有形成可复核的组合，保留单人候选和未决判断。');
  } else for(const facet of result.input.facets) {list.append(element('h3',facet.label));for(const row of result.candidates[facet.id]){const card=element('div',null,'proposal');const button=element('button',name(row.person));button.onclick=()=>showPerson(row.person,facet.label);card.append(button);paragraph(card,gradeText(row.grade),'badge');list.append(card);}}
}
function cancelReplay(){timers.forEach(clearTimeout);timers=[];}
function showStageDetail(){if(stage===2&&result.proposals.length)showProposal(result.proposals[0]);else if(stage===1){const f=result.input.facets[0],row=result.candidates[f.id][0];if(row)showPerson(row.person,f.label);}else{$('detail-title').textContent='从当前输入开始';$('detail').replaceChildren(element('p',result.input.intent.query));}}
function setStage(value){cancelReplay();stage=value;renderGraph();renderList();showStageDetail();}
async function loadCase(entry){
  cancelReplay();result=await read(entry.file);stage=2;selected=null;
  for(const proposal of result.proposals) for(const member of proposal.members){member.person=member.node;member.contribution_role=member.role;}
  colors={prior:'#c591ff',...Object.fromEntries(result.input.facets.map((f,i)=>[f.id,['#70d6b5','#e9ba74','#c591ff'][i%3]]))};
  $('signal-text').textContent=result.input.intent.query;$('case-description').textContent=entry.description;
  $('download-result').href=entry.file;
  const s=result.stats;
  $('run-summary').textContent=`在 ${population.people.length} 个合成主体中，完成 ${s.questions} 个问题判断，形成 ${result.proposals.length} 个接受复核的组合。本次执行 ${(s.elapsed_ms/1000).toFixed(2)} 秒，新增模型费用约 $${Number(s.new_estimated_cost_usd||0).toFixed(6)}。`;
  $('reuse-summary').textContent=result.reuse_check?`相同输入再次执行：新增后端请求 ${result.reuse_check.new_calls} 次。${result.ablation?`关掉组合步骤：${result.ablation.questions} 个单人判断，0 个组合提议。`:''}`:'';
  renderGraph();renderList();
  if(result.proposals.length)showProposal(result.proposals[0]);else {const f=result.input.facets[0];if(result.candidates[f.id].length)showPerson(result.candidates[f.id][0].person,f.label);}
}
try {
  [cases,population]=await Promise.all([read('./cases.json'),read('../population/data.json')]);
  people=new Map(population.people.map(p=>[p.id,p]));
  positions=new Map(population.people.map((p,i)=>{const r=235*Math.sqrt((i+.5)/population.people.length),a=i*2.39996323;return[p.id,{x:380+r*Math.cos(a),y:285+r*Math.sin(a)}];}));
  for(const [i,entry]of cases.entries()){const option=element('option',entry.title);option.value=String(i);$('case').append(option);}
  $('case').onchange=()=>loadCase(cases[Number($('case').value)]).catch(error=>{$('status').textContent=String(error);});
  $('show-people').onclick=()=>setStage(1);$('show-teams').onclick=()=>setStage(2);
  $('replay').onclick=()=>{cancelReplay();stage=0;renderGraph();renderList();showStageDetail();if(matchMedia('(prefers-reduced-motion: reduce)').matches){stage=2;renderGraph();renderList();showStageDetail();return;}timers.push(setTimeout(()=>{stage=1;renderGraph();renderList();showStageDetail();},1000),setTimeout(()=>{stage=2;renderGraph();renderList();showStageDetail();},2800));};
  await loadCase(cases[0]);
  $('status').textContent='已载入实际运行结果。人物为合成样本，提议不代表本人同意，也不代表已发生合作。';
}catch(error){$('status').textContent=String(error);}
