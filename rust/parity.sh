#!/usr/bin/env bash
# Compare public CLI output. Live counters vary between sequential snapshots.
set -euo pipefail
REAPER_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export REAPER_ROOT
export PYTHONPATH="$REAPER_ROOT/src${PYTHONPATH:+:$PYTHONPATH}"
export REAPER_RUST_BIN="${REAPER_RUST_BIN:-$REAPER_ROOT/rust/target/release/headless-reaper}"
exec "${PYTHON:-python3}" - "$@" <<'PY'
import difflib, json, os, re, subprocess, sys
python = [sys.executable, '-m', 'headless_reaper']
rust = [os.environ['REAPER_RUST_BIN']]

def normal_json(value, key=''):
    if isinstance(value, dict):
        return {k: normal_json(v, k) for k,v in value.items()}
    if isinstance(value, list):
        return [normal_json(v, key) for v in value]
    if key in ('scanned', 'age_seconds', 'rss_bytes'):
        assert type(value) is int and value >= 0, (key, value)
        return '<nonnegative int>'
    if key == 'age':
        assert isinstance(value,str) and re.fullmatch(r'\d+[dhms](?: \d+[hms])?',value), value
        return '<duration>'
    if key == 'rss':
        assert isinstance(value,str) and re.fullmatch(r'\d+(?:\.\d+)? (?:B|KB|MB|GB|TB)',value), value
        return '<bytes>'
    return value

def normalize(text):
    if text.startswith('{'):
        return json.dumps(normal_json(json.loads(text)),sort_keys=True,indent=2)
    text = re.sub(r'\(\d+ processes scanned', '(<count> processes scanned',text)
    text = re.sub(r'\d+(?:\.\d+)? (?:B|KB|MB|GB|TB) RSS','<rss> RSS',text)
    lines=[]
    for line in text.splitlines():
        if re.match(r'^ROOT\s+AGE\s+RSS\s+PIDS\s+PROFILE$',line):
            line='ROOT AGE RSS PIDS PROFILE'
        elif re.match(r'^\s+why:',line):
            line=line.strip()
        else:
            row=re.match(r'^(\d+)\s+((?:\d+[dhms]\s*)+)\s+(\d+(?:\.\d+)? (?:B|KB|MB|GB|TB))\s+(\d+)\s+(.+)$',line)
            if row:
                line=f'{row[1]} <age> <rss> {row[4]} {row[5]}'
        lines.append(line)
    return '\n'.join(lines)

cases = [sys.argv[1:]] if len(sys.argv)>1 else [
    ['--version'], ['--help'], [], ['--json'], ['--json','--min-age','1d'],
    ['--path','/tmp/headless-reaper-parity-empty','--min-age','30m'],
    ['--json','--path','/tmp/headless-reaper-parity-empty','--path','/var/tmp/headless-reaper-parity-empty'],
    ['--json','--kill','--remove-profiles','--grace','0','--path','/tmp/headless-reaper-parity-empty'],
    ['--json','--min-age','bad'], ['--json','--remove-profiles'],
    ['--json','--unknown'], ['--json','--path'],
    ['--json','--min-age','-1'], ['--help=x'], ['--json','--j','--min-age=1m'],
    ['--json','--min-age','9'*400], ['--json','--min-age','\u0663s'], ['--json','--grace','9'*400],
    ['--json','--min-age','999999999999999999999'], ['--json','--grace','36501d'],
    ['--json','--min-age','36500d'],
    ['--clean-clones','--json','--min-age','1d'], ['--clean-clones','--min-age','1d'],
    ['--all-leftovers','--json','--min-age','36500d','--path','/tmp/headless-reaper-parity-empty'],
]
for flags in cases:
    # A sequential comparison cannot kill the same group twice. Keep mutation
    # comparisons to empty scopes; test a real orphan with the Rust CLI itself.
    if '--kill' in flags or '--all-leftovers' in flags:
        dry=[f for f in flags if f not in ('--kill','--remove-profiles','--json','--all-leftovers')]+['--json']
        if '--all-leftovers' in flags:
            dry.append('--clean-clones')
        observed=json.loads(subprocess.run(python+dry,text=True,capture_output=True).stdout)
        if observed.get('groups') or observed.get('clones',{}).get('candidates'):
            sys.exit('Refusing mutating parity with targets; use an empty --path prefix and an older --min-age cutoff.')
    a,b=[subprocess.run(cmd+flags,text=True,capture_output=True) for cmd in (python,rust)]
    left,right=[(r.returncode,normalize(r.stdout),normalize(r.stderr)) for r in (a,b)]
    if left != right:
        print('FAIL',flags)
        print('\n'.join(difflib.unified_diff(str(left).splitlines(),str(right).splitlines(),fromfile='Python',tofile='Rust')))
        sys.exit(1)
    print('PASS', ' '.join(flags) or '(default)')
print(f'parity: {len(cases)} case(s) passed')
PY
