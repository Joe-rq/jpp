#!/usr/bin/env python3
"""`--guard` 回退对照（批 9 裁定 B187 §四·7；过程记录 `地基/过程记录/工程-默认相信判断器.md` 补·1）。

默认相信判断器之后，`--guard` 是旧语义的回退基线：带 `--guard` 跑金样清单里的每个用例，结果应与默认值翻转之前的
金样逐字段一致（报告与账本按 JSON 解析后比，`stderr` 按文本比）。本脚本照 `crates/jpp/tests/golden.rs` 的做法跑首跑与只凭账本重放（参数、运行目录、路径规范化都同），
每条命令都加 `--guard`，与基线提交上的金样文件比较。

允许的差别只有把关位本身带来的：
- 账本头 `compared.entry_hash`（把关位进哈希），以及由它带动的各条目 `prev` 链与行外壳的 `trace`（C-2：默认追踪编号由入口参数哈希推导，基线提交的金样里没有这个字段）；
- 报告顶层 `guard: true`；
- 从翻转前录的种子账本续跑时，账本头 `entry_hash` 不同带出的那一条 `W-header`。
其余字段不同即报差异。基线提交缺省为 `git merge-base HEAD main`（翻转之前的 main）。

用法：
    python3 scripts/guard_baseline.py [--jpp target/debug/jpp] [--base <提交>] [--json out.json]
只报告，不失败（退出码 0）；差异写在输出里，由门禁的人判读。
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent  # 地基/rust-jpp
REPO_PREFIX = '地基/rust-jpp/'


def git(*args) -> str:
    return subprocess.run(['git', *args], cwd=ROOT, capture_output=True, check=True).stdout.decode('utf-8')


def base_file(base: str, rel: str):
    try:
        return git('show', f'{base}:{REPO_PREFIX}{rel}')
    except subprocess.CalledProcessError:
        return None


def normalize(text: str, tmp: pathlib.Path) -> str:
    t = str(tmp.resolve())
    return text.replace(t, '<TMP>').replace(str(tmp), '<TMP>').replace(str(ROOT.resolve()), '<ROOT>')


def strip_report(v):
    if isinstance(v, dict):
        v = dict(v)
        v.pop('guard', None)
        # 从翻转前录的种子账本续跑（resume_ledger）时，账本头 entry_hash 由「无」变成含把关位的值，报一条 W-header
        tr = v.get('trace')
        if isinstance(tr, dict) and isinstance(tr.get('warnings'), list):
            tr = dict(tr)
            tr['warnings'] = [w for w in tr['warnings']
                              if not (w.startswith('W-header') and 'entry_hash 旧 （无）' in w)]
            v['trace'] = tr
    return v


def strip_ledger(text: str):
    out = []
    for i, line in enumerate(text.splitlines()):
        if not line.strip():
            continue
        j = json.loads(line)
        if i == 0 and 'header' in j:
            j['header']['compared'].pop('entry_hash', None)
        j.pop('prev', None)
        # C-2：账本行外壳的追踪字段（默认开，基线提交的金样里没有）；追踪编号由程序标识与入口参数哈希推导，
        # 而把关位进入口参数哈希，所以 --guard 下它必然不同，与把关无关的语义
        j.pop('trace', None)
        out.append(j)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--jpp', default=str(ROOT / 'target' / 'debug' / 'jpp'))
    ap.add_argument('--base', default=None)
    ap.add_argument('--json', default=None)
    ns = ap.parse_args()
    base = ns.base or git('merge-base', 'HEAD', 'main').strip()
    manifest = json.loads((ROOT / 'tests/golden/manifest.json').read_text(encoding='utf-8'))
    work = pathlib.Path(tempfile.mkdtemp(prefix='guard_baseline_'))
    results = []
    for c in manifest['cases']:
        name = c['name']
        tmp = work / name
        tmp.mkdir(parents=True)
        for fn, body in (c.get('files') or {}).items():
            (tmp / fn).write_text(body, encoding='utf-8')
        args = [ns.jpp, 'run', str(ROOT / c['source'])]
        if c.get('fixtures'):
            args += ['--fixtures', str(ROOT / c['fixtures'])]
        args += c.get('args', []) + ['--guard']
        diffs = []
        if c.get('expect') == 'error':
            r = subprocess.run(args, cwd=tmp, capture_output=True)
            got = normalize(r.stderr.decode('utf-8'), tmp)
            want = base_file(base, f'tests/golden/{name}/stderr.txt')
            if r.returncode == 0:
                diffs.append('预期失败却成功')
            elif got != want:
                diffs.append('stderr 与基线不同')
            results.append({'name': name, 'diffs': diffs})
            continue
        first = list(args)
        if c.get('calib'):
            first += ['--calib', str(ROOT / c['calib'])]
        if c.get('resume_from'):
            first += ['--resume', str(work / c['resume_from'] / 'ledger.json')]
        if c.get('resume_ledger'):
            first += ['--resume', str(ROOT / c['resume_ledger'])]
        first += ['--ledger-out', 'ledger.json', '--output', 'report.json']
        r = subprocess.run(first, cwd=tmp, capture_output=True)
        if r.returncode != 0:
            results.append({'name': name, 'diffs': ['首跑失败：' + r.stderr.decode('utf-8')[-300:]]})
            continue
        report = strip_report(json.loads(normalize((tmp / 'report.json').read_text(encoding='utf-8'), tmp)))
        want_r = base_file(base, f'tests/golden/{name}/report.json')
        if want_r is None or report != strip_report(json.loads(want_r)):
            diffs.append('report.json')
        ledger = strip_ledger(normalize((tmp / 'ledger.json').read_text(encoding='utf-8'), tmp))
        want_l = base_file(base, f'tests/golden/{name}/ledger.json')
        if want_l is None or ledger != strip_ledger(want_l):
            diffs.append('ledger.json')
        rtmp = tmp / 'replay'
        rtmp.mkdir()
        for fn, body in (c.get('files') or {}).items():
            (rtmp / fn).write_text(body, encoding='utf-8')
        rargs = list(args) + ['--replay', str(tmp / 'ledger.json'), '--output', 'replay-report.json']
        rr = subprocess.run(rargs, cwd=rtmp, capture_output=True)
        if c.get('replay'):
            want = base_file(base, f'tests/golden/{name}/replay-stderr.txt')
            if rr.returncode == 0 or normalize(rr.stderr.decode('utf-8'), tmp) != want:
                diffs.append('replay-stderr.txt')
        elif rr.returncode != 0:
            diffs.append('重放失败：' + rr.stderr.decode('utf-8')[-300:])
        else:
            rep = strip_report(json.loads(normalize((rtmp / 'replay-report.json').read_text(encoding='utf-8'), tmp)))
            want_rr = base_file(base, f'tests/golden/{name}/replay-report.json')
            if want_rr is None or rep != strip_report(json.loads(want_rr)):
                diffs.append('replay-report.json')
        results.append({'name': name, 'diffs': diffs})
    shutil.rmtree(work, ignore_errors=True)
    bad = [r for r in results if r['diffs']]
    print(f'--guard 回退对照：基线 {base[:8]}，用例 {len(results)}，与基线逐字段一致（JSON 解析后比，去掉把关位） {len(results) - len(bad)}，有差异 {len(bad)}')
    for r in bad:
        print(f'  {r["name"]}: {"; ".join(r["diffs"])}')
    if ns.json:
        pathlib.Path(ns.json).write_text(json.dumps({'base': base, 'results': results}, ensure_ascii=False, indent=1), encoding='utf-8')


if __name__ == '__main__':
    main()
