const $=id=>document.getElementById(id);
const sourceNames={academic:'科研主体',github:'开源主体',startup:'创业主体'};
const relationNames={coauthor_same_field:'共同论文记录',github_same_org:'同一开源组织',same_team_equivalent:'同一创业团队',same_team_complementary:'同团队互补角色',cross_source_same_field:'人工挑选的跨来源同领域关联',cross_source_complementary:'人工挑选的跨来源互补关联',cross_source_functional:'人工挑选的跨来源功能关联'};
const read=async path=>{const r=await fetch(path);if(!r.ok)throw new Error('无法载入 '+path);return r.json()};
let data,results,people,methods,selectedTarget;
const title=id=>`${sourceNames[people.get(id).source_type]||'主体'} ${id}`;
const pair=(a,b)=>[a,b].sort().join('|');
const NS='http://www.w3.org/2000/svg';
function svg(tag,attrs){const e=document.createElementNS(NS,tag);for(const[k,v]of Object.entries(attrs))e.setAttribute(k,v);return e}
let relations,positions;
function detail(id){
  selectedTarget=id;const source=$('source').value,target=people.get(id),rel=relations.get(pair(source,id)),method=methods.get($('method').value);
  $('person-title').textContent=title(id);$('source-context').textContent=people.get(source).context;$('target-context').textContent=target.context;
  $('relation-status').textContent=source===id?'当前出发方':rel?'旧关系记录：'+(relationNames[rel.type]||rel.type)+'。':'旧关系名单中没有这条边；这不等于没有合作价值。';
  const grade=method.grades?.[source]?.[id];
  $('judgment').textContent=grade?(grade.level===null?'J++ 没有接受一个确定等级；可保留在相对排序中继续查看。':'J++ 暂定等级：'+results.levels[grade.level]+'。这不是已确认的合作关系。'):'这个方法提供检索排序；请结合双方资料判断具体关联。';
}
function renderGraph(){
  const source=$('source').value,method=methods.get($('method').value),k=Number($('count').value),ranked=method.rankings[source].slice(0,k),selected=new Set(ranked),origin=positions.get(source),known=new Set();
  for(const rel of data.known_relations){if(rel.a===source)known.add(rel.b);if(rel.b===source)known.add(rel.a)}
  $('graph-title').textContent=title(source)+' 出发';$('local-metric').textContent=`候选 ${ranked.length} 人 · 找回已知关系 ${ranked.filter(id=>known.has(id)).length} / ${known.size}`;
  const graph=$('network');graph.replaceChildren();
  for(const id of new Set([...ranked,...known])){const p=positions.get(id),hit=selected.has(id),line=svg('line',{x1:origin.x,y1:origin.y,x2:p.x,y2:p.y,stroke:hit?(known.has(id)?'#70d6b5':'#e9ba74'):'#728490','stroke-width':hit?1.6:1,'class':'edge'+(hit?'':' missed')});graph.append(line)}
  for(const person of data.people){const id=person.id,p=positions.get(id),on=selected.has(id),isSource=id===source,g=svg('g',{class:'person-node',role:'button',tabindex:'0','aria-label':title(id)+(on?'，候选':'')});g.append(svg('circle',{cx:p.x,cy:p.y,r:isSource?8:on?5.5:2.6,fill:isSource?'#eef9f6':on?(known.has(id)?'#70d6b5':'#e9ba74'):known.has(id)?'#728490':'#29414d',stroke:isSource?'#70d6b5':'none','stroke-width':3}));if(on||isSource){const t=svg('text',{x:p.x+8,y:p.y-6});t.textContent=id;g.append(t)}g.onclick=()=>detail(id);g.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();detail(id)}};graph.append(g)}
  $('candidate-list').replaceChildren(...ranked.map((id,index)=>{const b=document.createElement('button');b.textContent=`${String(index+1).padStart(2,'0')} · ${title(id)}`;const small=document.createElement('small');small.textContent=known.has(id)?'有旧关系标签':'未被旧名单标注';b.append(small);b.onclick=()=>detail(id);return b}));
  for(const row of $('metrics').children)row.classList.toggle('selected',row.dataset.key===$('method').value);
  detail(selectedTarget&&selected.has(selectedTarget)?selectedTarget:ranked[0]||source);
}
try{
  [data,results]=await Promise.all([read('./data.json'),read('./results.json')]);people=new Map(data.people.map(p=>[p.id,p]));methods=new Map(results.methods.map(m=>[m.id,m]));relations=new Map(data.known_relations.map(r=>[pair(r.a,r.b),r]));
  positions=new Map(data.people.map((p,i)=>{const radius=245*Math.sqrt((i+.5)/data.people.length),angle=i*2.39996323;return[p.id,{x:340+radius*Math.cos(angle),y:285+radius*Math.sin(angle)}]}));
  for(const p of data.people){const o=document.createElement('option');o.value=p.id;o.textContent=title(p.id);$('source').append(o)}
  for(const m of results.methods){const o=document.createElement('option');o.value=m.id;o.textContent=m.label;$('method').append(o);const tr=document.createElement('tr');tr.dataset.key=m.id;const name=document.createElement('td'),b=document.createElement('button');b.textContent=m.label;b.onclick=()=>{$('method').value=m.id;renderGraph()};name.append(b);tr.append(name);for(const k of [1,5,10,20]){const metric=m.metrics[String(k)],td=document.createElement('td');td.textContent=`${metric.undirected_hits}/${metric.undirected_total} · ${(metric.undirected_recall*100).toFixed(1)}%`;tr.append(td)}$('metrics').append(tr)}
  $('method').value=results.default_method;$('source').value=results.default_source||data.people[0].id;
  const changes=results.changes_at_10_vs_rrf;
  $('changes-summary').textContent=`相对本地融合，J++ 相对排序新增找回 ${changes.added.length} 条，同时有 ${changes.lost.length} 条不再进入前十，净增 ${changes.added.length-changes.lost.length} 条。两组都可以逐对查看。`;
  for(const kind of ['added','lost']){for(const [i,edge]of changes[kind].entries()){const option=document.createElement('option');option.value=String(i);option.textContent=`${edge.a} ↔ ${edge.b} · ${relationNames[edge.type]||edge.type}`;$(kind).append(option)}$(kind).onchange=()=>{if($(kind).value==='')return;const edge=changes[kind][Number($(kind).value)],method=kind==='added'?'order20':'rrf',forward=methods.get(method).rankings[edge.a].slice(0,10).includes(edge.b);$('source').value=forward?edge.a:edge.b;selectedTarget=forward?edge.b:edge.a;$('method').value=method;$('count').value='10';renderGraph()}}
  $('finding').textContent=results.finding;$('costs').textContent=results.cost_summary;$('status').textContent='已载入实际计算记录。所有页面操作均为本地查看，无新模型调用。';
  $('source').onchange=()=>{selectedTarget=null;renderGraph()};$('method').onchange=renderGraph;$('count').onchange=renderGraph;renderGraph();
}catch(error){$('status').textContent=String(error)}
