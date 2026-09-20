const $=id=>document.getElementById(id);
const sourceNames={academic:'科研主体',github:'开源主体',startup:'创业主体'};
const relationNames={coauthor_same_field:'共同论文记录',github_same_org:'同一开源组织',same_team_equivalent:'同一创业团队',same_team_complementary:'同团队互补角色',cross_source_same_field:'人工挑选的跨来源同领域关联',cross_source_complementary:'人工挑选的跨来源互补关联',cross_source_functional:'人工挑选的跨来源功能关联'};
const read=async path=>{const r=await fetch(path);if(!r.ok)throw new Error('无法载入 '+path);return r.json()};
let data,results,exploration,people,methods,selectedTarget,relations,positions;
const title=id=>`${sourceNames[people.get(id).source_type]||'主体'} ${id}`;
const pair=(a,b)=>[a,b].sort().join('|');
const NS='http://www.w3.org/2000/svg';
function svg(tag,attrs){const e=document.createElementNS(NS,tag);for(const[k,v]of Object.entries(attrs))e.setAttribute(k,v);return e}
function edgeLabel(edge){return `${edge.a} ↔ ${edge.b}${edge.type?` · ${relationNames[edge.type]||edge.type}`:''}`}
function originFor(source,target){const o=exploration?.origins||{};return o?.[source]?.[target]||o?.[pair(source,target)]||o?.[`${source}|${target}`]||null}
function explorationMethod(){return exploration?.comparison?.method||'explore_source4'}
function candidateFor(source,target){const c=exploration?.candidates||{},fromSource=c?.[source];if(Array.isArray(fromSource)){const index=fromSource.findIndex(x=>(typeof x==='string'?x:x.target||x.id)===target);return index<0?null:typeof fromSource[index]==='string'?{pool_rank:index+1,entered20:true}:fromSource[index]}return fromSource?.[target]||c?.[pair(source,target)]||c?.[`${source}|${target}`]||null}
function isExplore(source,target){return $('method')?.value===explorationMethod()&&originFor(source,target)==='different_source'}
function entranceText(source,target,rank){
  if($('method').value!==explorationMethod())return '';
  const origin=originFor(source,target),record=candidateFor(source,target);if(!origin&&!record)return '';
  const entrance={rrf16:'原排序保留入口',rrf_prefix:'原排序保留入口',different_source:'跨来源探索入口',rrf_backfill:'原排序回补入口'}[origin]||'录制的候选入口';
  const fields=record&&typeof record==='object'?[record.pool_rank&&`候选池第 ${record.pool_rank}`,record.candidate_pool_rank&&`候选池第 ${record.candidate_pool_rank}`,record.entered20!==undefined&&(record.entered20?'进入 20 人':'未进入 20 人'),record.rank&&`进入 20 人后排序第 ${record.rank}`,record.rank_after_entry&&`进入 20 人后排序第 ${record.rank_after_entry}`].filter(Boolean):[];
  return `入口：${entrance}。阶段：${fields.join('，')|| (rank?`进入 20 人后，当前排序第 ${rank}`:'当前未进入展示范围')}。`;
}
function detail(id){
  selectedTarget=id;const source=$('source').value,target=people.get(id),rel=relations.get(pair(source,id)),method=methods.get($('method').value),rank=(method.rankings[source]||[]).indexOf(id)+1||0;
  $('person-title').textContent=title(id);$('source-context').textContent=people.get(source).context;$('target-context').textContent=target.context;
  $('relation-status').textContent=source===id?'当前出发方':rel?'旧关系记录：'+(relationNames[rel.type]||rel.type)+'。':'旧关系名单中没有这条边；这不等于没有合作价值。';
  $('candidate-entrance').textContent=entranceText(source,id,rank);
  const grade=method.grades?.[source]?.[id];$('judgment').textContent=grade?(grade.level===null?'J++ 没有接受一个确定等级；可保留在相对排序中继续查看。':'J++ 暂定等级：'+results.levels[grade.level]+'。这不是已确认的合作关系。'):'这个方法提供检索排序；请结合双方资料判断具体关联。';
}
function renderGraph(){
  const source=$('source').value,method=methods.get($('method').value),k=Number($('count').value),ranked=(method.rankings[source]||[]).slice(0,k),selected=new Set(ranked),origin=positions.get(source),known=new Set();
  for(const rel of data.known_relations){if(rel.a===source)known.add(rel.b);if(rel.b===source)known.add(rel.a)}
  $('graph-title').textContent=title(source)+' 出发';$('local-metric').textContent=`候选 ${ranked.length} 人 · 找回已知关系 ${ranked.filter(id=>known.has(id)).length} / ${known.size}`;
  const graph=$('network');graph.replaceChildren();
  for(const id of new Set([...ranked,...known])){const p=positions.get(id),hit=selected.has(id),explore=hit&&isExplore(source,id),line=svg('line',{x1:origin.x,y1:origin.y,x2:p.x,y2:p.y,stroke:hit?(explore?'#c591ff':known.has(id)?'#70d6b5':'#e9ba74'):'#728490','stroke-width':hit?(explore?2.5:1.6):1,'class':'edge'+(hit?'':' missed')+(explore?' exploratory-edge':'')});graph.append(line)}
  for(const person of data.people){const id=person.id,p=positions.get(id),on=selected.has(id),isSource=id===source,explore=on&&isExplore(source,id),g=svg('g',{class:'person-node',role:'button',tabindex:'0','aria-label':title(id)+(on?'，候选':'')});g.append(svg('circle',{cx:p.x,cy:p.y,r:isSource?8:on?5.5:2.6,fill:isSource?'#eef9f6':on?(explore?'#c591ff':known.has(id)?'#70d6b5':'#e9ba74'):known.has(id)?'#728490':'#29414d',stroke:isSource?'#70d6b5':'none','stroke-width':3,'class':explore?'exploratory-node':''}));if(on||isSource){const t=svg('text',{x:p.x+8,y:p.y-6});t.textContent=id;g.append(t)}g.onclick=()=>detail(id);g.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();detail(id)}};graph.append(g)}
  $('candidate-list').replaceChildren(...ranked.map((id,index)=>{const b=document.createElement('button');b.textContent=`${String(index+1).padStart(2,'0')} · ${title(id)}`;const small=document.createElement('small');small.textContent=isExplore(source,id)?'跨来源探索入口 · 进入 20 人后排序':known.has(id)?'有旧关系标签':'未被旧名单标注';b.append(small);b.onclick=()=>detail(id);return b}));
  for(const row of $('metrics').children)row.classList.toggle('selected',row.dataset.key===$('method').value);detail(selectedTarget&&selected.has(selectedTarget)?selectedTarget:ranked[0]||source);
}
function addMethod(m){
  if(methods.has(m.id))return;methods.set(m.id,m);const o=document.createElement('option');o.value=m.id;o.textContent=m.label;$('method').append(o);const tr=document.createElement('tr');tr.dataset.key=m.id;const name=document.createElement('td'),b=document.createElement('button');b.textContent=m.label;b.onclick=()=>{$('method').value=m.id;renderGraph()};name.append(b);tr.append(name);
  for(const k of [1,5,10,20]){const metric=m.metrics?.[String(k)],td=document.createElement('td');td.textContent=metric?`${metric.undirected_hits}/${metric.undirected_total} · ${(metric.undirected_recall*100).toFixed(1)}%`:'—';tr.append(td)}$('metrics').append(tr);
}
function jumpTo(edge,id){const method=methods.get(id),find=n=>{const forward=method.rankings?.[edge.a]?.slice(0,n).includes(edge.b),reverse=method.rankings?.[edge.b]?.slice(0,n).includes(edge.a);return forward||reverse?{source:forward?edge.a:edge.b,target:forward?edge.b:edge.a,count:n}:null},match=find(10)||find(20);if(!match)return;$('source').value=match.source;selectedTarget=match.target;$('method').value=id;$('count').value=String(match.count);renderGraph()}
function fillExistingChanges(){const changes=results.changes_at_10_vs_rrf;$('changes-summary').textContent=`相对本地融合，J++ 相对排序新增找回 ${changes.added.length} 条，同时有 ${changes.lost.length} 条不再进入前十，净增 ${changes.added.length-changes.lost.length} 条。两组都可以逐对查看。`;for(const kind of ['added','lost']){for(const [i,edge]of changes[kind].entries()){const option=document.createElement('option');option.value=String(i);option.textContent=edgeLabel(edge);$(kind).append(option)}$(kind).onchange=()=>{$(kind).value!==''&&jumpTo(changes[kind][Number($(kind).value)],kind==='added'?'order20':'rrf')}}}
function fillExploration(){
  if(!exploration?.methods?.length)return;$('exploration-panel').hidden=false;exploration.methods.forEach(addMethod);$('exploration-finding').textContent=exploration.finding||'';$('exploration-tradeoff').textContent=exploration.cost_summary||'';
  const c=exploration.comparison||{},added=c.added||[],lost=c.lost||[],baseline=c.baseline_method||'order20',experiment=c.method||exploration.methods.find(m=>m.id!=='order20')?.id||exploration.methods[0].id;
  const pools=exploration.candidate_comparison||{},basePool=pools.baseline,explorePool=pools.explore_source4,base10=methods.get(baseline)?.metrics?.['10'],explore10=methods.get(experiment)?.metrics?.['10'];
  const poolText=(label,pool)=>pool?`${label}：候选池覆盖 ${pool.covered_edges}/${pool.total_edges} 条已知边；跨来源种子 ${pool.curated_cross_source_covered}/${pool.curated_cross_source_total} 条；定向候选位 ${pool.directed_candidate_slots}。`:'';
  const outputText=(label,metric)=>metric?`${label} 前 10 输出 ${metric.undirected_hits}/${metric.undirected_total} 条已知边。`:'';
  const typeMetrics=explore10?.by_type||explore10?.metrics_by_type||{},crossSeed=Object.entries(typeMetrics).filter(([type])=>type.startsWith('cross_source_')).map(([,metric])=>`${metric.hits??metric.undirected_hits}/${metric.total??metric.undirected_total}`).join('、');
  $('exploration-scope').textContent=[poolText('输入 20 位（原排序）',basePool),poolText('输入 20 位（含探索）',explorePool),outputText('输出',base10),outputText('探索输出',explore10),crossSeed&&`其中跨来源种子前十找回 ${crossSeed}。`].filter(Boolean).join(' ')+(explorePool?' 跨来源种子是待检验的探索假设，不等同于已知真实关系。':'');
  for(const [id,edges,method] of [['explore-added',added,experiment],['explore-lost',lost,baseline]]){for(const [i,edge]of edges.entries()){const option=document.createElement('option');option.value=String(i);option.textContent=edgeLabel(edge);$(id).append(option)}$(id).onchange=()=>{$(id).value!==''&&jumpTo(edges[Number($(id).value)],method)}}
  if(exploration.default_example){const e=exploration.default_example,m=methods.get(experiment),forward=m.rankings?.[e.a]?.slice(0,10).includes(e.b);$('source').value=forward?e.a:e.b;selectedTarget=forward?e.b:e.a;$('method').value=experiment;$('count').value='10';}
}
try{
  [data,results]=await Promise.all([read('./data.json'),read('./results.json')]);exploration=await read('./exploration.json').catch(()=>null);people=new Map(data.people.map(p=>[p.id,p]));methods=new Map();relations=new Map(data.known_relations.map(r=>[pair(r.a,r.b),r]));positions=new Map(data.people.map((p,i)=>{const radius=245*Math.sqrt((i+.5)/data.people.length),angle=i*2.39996323;return[p.id,{x:340+radius*Math.cos(angle),y:285+radius*Math.sin(angle)}]}));
  for(const p of data.people){const o=document.createElement('option');o.value=p.id;o.textContent=title(p.id);$('source').append(o)}results.methods.forEach(addMethod);$('method').value=results.default_method;$('source').value=results.default_source||data.people[0].id;fillExistingChanges();fillExploration();$('finding').textContent=results.finding;$('costs').textContent=results.cost_summary;$('status').textContent=`已载入实际计算记录${exploration?'及候选探索回放':''}。所有页面操作均为本地查看，无新模型调用。`;$('source').onchange=()=>{selectedTarget=null;renderGraph()};$('method').onchange=renderGraph;$('count').onchange=renderGraph;renderGraph();
}catch(error){$('status').textContent=String(error)}
