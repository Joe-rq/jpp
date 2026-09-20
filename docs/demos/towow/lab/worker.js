// Run the published Python implementation in a worker; only model answers replay.
let runtimePromise;
async function initialize() {
  postMessage({type:'status', text:'首次准备运行环境，需要下载 Python；之后运行会直接复用。'});
  const {loadPyodide} = await import('https://cdn.jsdelivr.net/pyodide/v314.0.7/full/pyodide.mjs');
  const python = await loadPyodide();
  await python.loadPackage('pyyaml');
  postMessage({type:'status', text:'正在载入仓库中的 J++ 程序与公开 JEV 记录…'});
  const [manifestResponse, bundleResponse] = await Promise.all([fetch('./manifest.json'), fetch('./jpp-source.zip')]);
  if (!manifestResponse.ok || !bundleResponse.ok) throw new Error('J++ source download failed');
  const manifest = await manifestResponse.json(), bundle = await bundleResponse.arrayBuffer();
  const hash = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bundle))].map(x=>x.toString(16).padStart(2,'0')).join('');
  if (hash !== manifest.sha256) throw new Error('Source bundle version mismatch. Refresh this page.');
  python.unpackArchive(bundle, 'zip', {extractDir:'/app'});
  python.runPython("import sys, json\nsys.path.insert(0, '/app')\nfrom jpp.towow_lab import LabSession, compare\n_lab = LabSession()");
  return {python, hash};
}
let queue = Promise.resolve();
self.onmessage = ({data}) => {
  queue = queue.then(async()=>{
    try {
      runtimePromise ||= initialize();
      const {python, hash} = await runtimePromise;
      let json;
      if(data.command === 'population') {
        if(!python.globals.has('_population_client')) python.runPython("from jpp.towow_population import run_population\nfrom jpp.towow import RecordedClient\nfrom importlib.resources import files\nfrom tempfile import TemporaryDirectory\n_population_recording = json.loads(files('jpp').joinpath('data/towow-population-recording.json').read_text())\n_population_client = RecordedClient(recording=_population_recording)\n_population_root = TemporaryDirectory()");
        json = python.runPython('json.dumps(run_population(_population_client, _population_root.name, browser=True), ensure_ascii=False)');
      }
      else if(data.command === 'compare') json = python.runPython('json.dumps(compare(), ensure_ascii=False)');
      else {
        if(data.fresh) python.runPython('_lab.close()\n_lab = LabSession()');
        python.globals.set('_options_json', JSON.stringify(data.options));
        json = python.runPython('json.dumps(_lab.run(**json.loads(_options_json)), ensure_ascii=False)');
      }
      postMessage({type:'result', id:data.id, result:JSON.parse(json), sourceHash:hash});
    } catch(error) {
      postMessage({type:'error', id:data.id, text:String(error)});
    }
  });
};
