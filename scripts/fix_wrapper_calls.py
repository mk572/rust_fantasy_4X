#!/usr/bin/env python3
"""Move callers of removed `Game` wrapper methods to a handle.

Usage: scripts/fix_wrapper_calls.py HANDLE METHOD...
  e.g. scripts/fix_wrapper_calls.py units unit_is_undead unit_realm

First delete the wrapper methods from `impl Game` by hand. The script then builds the
workspace with its tests (`cargo build --workspace --tests`), reads the "no method named `m`
found" errors for the listed methods, and rewrites each reported call `.m(` to `.HANDLE().m(`
(e.g. `game.units().m(`), repeating until the build reports none (a later round finds the
callers that an earlier error hid). Only the listed method names are touched, and only where the
compiler says the method is missing, so calls on the handle itself are left alone. Run
`cargo fmt --all` afterwards (task 198).
"""
import argparse,json,subprocess,sys,re,os

parser = argparse.ArgumentParser(
    description=__doc__.split("\n\nUsage:")[0] + "\n\n" + __doc__.split("\n\n", 2)[2],
    formatter_class=argparse.RawDescriptionHelpFormatter,
)
parser.add_argument("handle", choices=["units", "economy"],
                    help="the handle the callers should ask: `game.HANDLE()`")
parser.add_argument("methods", nargs="+", metavar="METHOD",
                    help="names of the deleted wrapper methods whose callers to rewrite")
args = parser.parse_args()
handle = args.handle
methods = set(args.methods)
env=dict(os.environ); env['PATH']=os.path.expanduser('~/.cargo/bin:')+env['PATH']
for rnd in range(10):
    r=subprocess.run(['cargo','build','--workspace','--tests','--message-format=json'],capture_output=True,text=True,env=env)
    edits={}
    for line in r.stdout.splitlines():
        try: m=json.loads(line)
        except: continue
        if m.get('reason')!='compiler-message': continue
        msg=m['message']
        if msg.get('level')!='error': continue
        mm=re.match(r'no method named `(\w+)` found',msg['message'])
        if not mm or mm.group(1) not in methods: continue
        for sp in msg['spans']:
            if sp['is_primary']:
                edits.setdefault(sp['file_name'],set()).add((sp['line_start'],sp['column_start'],mm.group(1)))
    if not edits: print('clean after',rnd,'rounds'); break
    for f,es in edits.items():
        lines=open(f).read().split('\n')
        for (ln,col,name) in sorted(es,reverse=True):
            l=lines[ln-1]
            idx=col-1
            assert l[idx:idx+len(name)]==name,(f,ln,l)
            lines[ln-1]=l[:idx]+f'{handle}().'+l[idx:]
        open(f,'w').write('\n'.join(lines))
    print('round',rnd,sum(len(e) for e in edits.values()),'edits')
else:
    print('not converged')
