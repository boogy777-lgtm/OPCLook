#!/usr/bin/env python3
"""Run isolated SDK readiness race regressions; never patch the registry."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('base', HERE / 'ua-sdk-readiness-stand.py')
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)


def replace_once(path, old, new):
    content = path.read_text()
    if content.count(old) != 1:
        raise ValueError('unexpected counterfactual source: ' + str(path))
    path.write_text(content.replace(old, new, 1))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('worktree', 'client-source', 'server-source', 'scratch', 'target-dir'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    work = args.worktree.resolve()
    if not (work / '.git').is_file() or subprocess.check_output(['git','status','--porcelain'],cwd=work,text=True):
        raise ValueError('worktree must be a clean disposable linked worktree')
    base.check_files(work, base.WORK_BLOBS)
    info = json.loads((args.client_source / '.cargo_vcs_info.json').read_text())
    if info.get('git', {}).get('sha1') != base.REVISION or info.get('path_in_vcs') != 'async-opcua-client':
        raise ValueError('incorrect client VCS receipt')
    base.check_files(args.client_source, base.CLIENT_BLOBS)
    scratch = args.scratch.resolve()
    scratch.mkdir(parents=True, exist_ok=False)
    client, server, counter, barrier_client = (scratch / n for n in ('client','server','counter','barrier-client'))
    fixture_spec = importlib.util.spec_from_file_location('fixture', HERE / 'ua-sdk-first-notification-stand.py')
    fixture = importlib.util.module_from_spec(fixture_spec)
    fixture_spec.loader.exec_module(fixture)
    fixture.prepare(args.server_source, server)
    shutil.copytree(args.client_source, client)
    docs = HERE.parent / 'docs/testing'
    for patch in ('ua-sdk-republish-v2-client.patch','ua-sdk-republish-fault-sticky.patch','ua-sdk-readiness-client.patch','ua-sdk-readiness-racefix-client.patch'):
        base.apply(client,docs / patch)
    base.apply(work,docs / 'ua-sdk-readiness-racefix-probe.patch')
    shutil.copytree(client,counter)
    replace_once(counter / 'src/session/mod.rs', '    before_lock();\n    let mut accepted = false;', '    let sampled = clock();\n    before_lock();\n    let mut accepted = false;')
    replace_once(counter / 'src/session/mod.rs', '&& clock() < state.deadline', '&& sampled < state.deadline')
    replace_once(counter / 'src/session/event_loop.rs', '    let guard = ReplayCancellationGuard { outcome, armed: true };\n    async move {\n        let mut guard = guard;', '    async move {\n        let mut guard = ReplayCancellationGuard { outcome, armed: true };')
    shutil.copytree(client,barrier_client)
    base.apply(barrier_client,docs / 'ua-sdk-readiness-racefix-barrier.patch')
    barrier = scratch / 'barrier'
    barrier.mkdir()
    env = os.environ.copy()
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()),CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='2')
    def run(which, package, name, expected, should_pass=True, extra=None):
        config = 'patch.crates-io.async-opcua-client.path=' + json.dumps(str(which))
        server_config = 'patch.crates-io.async-opcua-server.path=' + json.dumps(str(server))
        cmd=['cargo','test','--offline','--target','x86_64-unknown-linux-gnu','--config',server_config,'--config',config,'-p',package,'--lib',name,'--','--nocapture']
        result=subprocess.run(cmd,cwd=work,env={**env,**(extra or {})},capture_output=True,text=True,timeout=180)
        output=result.stdout+result.stderr
        (scratch / (name+'-'+('pass' if should_pass else 'counter')+'.log')).write_text(output)
        print(output,flush=True)
        if (result.returncode == 0) != should_pass or expected not in output:
            raise RuntimeError('unexpected result for '+name)
    run(counter,'async-opcua-client','deadline_is_checked_after_acquiring_ready_lock','expired readiness must not be confirmed',False)
    run(counter,'async-opcua-client','drop_unpolled_replay_reports_cancelled','FAILED',False)
    run(client,'async-opcua-client','recovery_','14 passed; 0 failed')
    run(client,'opclook-backend','sdk_ready_handoff_holds_replay_without_blocking_read','busy_read=progress replay=52.5 then_publish=53.5 unknown=true')
    run(client,'opclook-backend','sdk_ready_deadline_and_transport_loss_reject_late_ack','loss_cancel=typed stale_ack=rejected')
    run(barrier_client,'opclook-backend','sdk_ready_simultaneous_ack_and_closed_prioritizes_loss','simultaneous_ack_closed=Cancelled stale_ack=rejected',extra={'OPCLOOK_TEST_SELECT_BARRIER':str(barrier)})

if __name__ == '__main__':
    try:
        main()
    except (OSError,ValueError,RuntimeError,subprocess.SubprocessError) as error:
        print(f'readiness race stand: {error}',file=sys.stderr)
        sys.exit(1)
