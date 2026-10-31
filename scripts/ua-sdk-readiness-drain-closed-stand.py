#!/usr/bin/env python3
"""Isolated instrumented SDK-observed Closed versus drain-completion probe."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('base',HERE/'ua-sdk-replay-engine-stand.py')
base=importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('worktree','client-source','client-dest','server-source','server-dest','barrier','target-dir','log'):
        parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();work=args.worktree.resolve()
    if not (work/'.git').is_file() or subprocess.check_output(['git','status','--porcelain'],cwd=work,text=True):
        raise ValueError('worktree must be a clean disposable linked worktree')
    expected={k:v for k,v in base.WORK_BLOBS.items() if k.endswith('live_tests.rs')}
    expected['crates/backend/src/bff/ua/session/connection.rs']='6d09a120178c226a2af96200c3776354bec8c24a'
    base.check_files(work,expected)
    info=json.loads((args.client_source/'.cargo_vcs_info.json').read_text())
    if info.get('git',{}).get('sha1')!=base.REVISION or info.get('path_in_vcs')!='async-opcua-client':
        raise ValueError('incorrect client VCS receipt')
    base.check_files(args.client_source,base.CLIENT_BLOBS)
    if args.client_dest.exists() or args.barrier.exists() or args.log.exists():
        raise ValueError('client, barrier and log must not exist')
    fixture_spec=importlib.util.spec_from_file_location('fixture',HERE/'ua-sdk-first-notification-stand.py')
    fixture=importlib.util.module_from_spec(fixture_spec);fixture_spec.loader.exec_module(fixture)
    fixture.prepare(args.server_source,args.server_dest)
    shutil.copytree(args.client_source,args.client_dest);args.barrier.mkdir()
    docs=HERE.parent/'docs/testing'
    for patch in ('ua-sdk-republish-v2-client.patch','ua-sdk-republish-fault-sticky.patch','ua-sdk-readiness-client.patch','ua-sdk-readiness-racefix-client.patch','ua-sdk-readiness-drain-client.patch','ua-sdk-readiness-drain-closed-client.patch'):
        base.apply(args.client_dest,docs/patch)
    base.apply(work,docs/'ua-sdk-readiness-drain-closed-probe.patch')
    env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()),CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2',OPCLOOK_TEST_DRAIN_BARRIER=str(args.barrier.resolve()))
    server='patch.crates-io.async-opcua-server.path='+json.dumps(str(args.server_dest.resolve()))
    client='patch.crates-io.async-opcua-client.path='+json.dumps(str(args.client_dest.resolve()))
    cmd=['cargo','test','--offline','--target','x86_64-unknown-linux-gnu','--config',server,'--config',client,'-p','opclook-backend','--lib','sdk_drain_simultaneous_complete_and_closed_prioritizes_loss','--','--nocapture']
    result=subprocess.run(cmd,cwd=work,env=env,capture_output=True,text=True,timeout=180)
    output=result.stdout+result.stderr;args.log.write_text(output);print(output,end='')
    if result.returncode or 'simultaneous_drain_closed=Cancelled no_complete=true' not in output:
        raise RuntimeError('instrumented probe failed; see log')

if __name__=='__main__':
    try: main()
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as e:
        print(f'drain-closed probe: {e}',file=sys.stderr);sys.exit(1)
