"""Fake worker for supervisor deadline, check-state and disposal controls; no engine calls."""
import json
import os
from pathlib import Path
import time


def publish(name, value):
    path = Path(name)
    pending = path.with_suffix('.pending')
    pending.write_text(json.dumps(value))
    pending.replace(path)


mode = os.environ['SDK_CHECK_MODE']
delay = float(os.environ['SDK_CHECK_DELAY'])
witness = dict(attempt='unit', game=int(os.environ['GAME_PID']), worker=os.getpid(),
               thread=7, returned=['common/traditions'], generation=0, state='held')
publish('session-paused.json', witness)
active = None
while True:
    request_path = Path('script-check.json')
    if request_path.exists() and active is None:
        active = json.loads(request_path.read_text())
        request_path.unlink()
        assert active['scope'] == 1 << 40
        if mode == 'worker-loss':
            os._exit(1)
        witness['state'] = {'checking': active['check']}
        started = time.monotonic()
    if active and time.monotonic() - started >= delay and mode == 'normal':
        observation = dict(check=active['check'], read_returned=True, children=1,
                           diagnostics=[], foreign=[], unjoined=[], hooks_active=True, bound_reached=False)
        publish('script-check-reply.json', dict(attempt='unit', check=active['check'], result={'Ok': observation}))
        witness['state'] = 'held'
        active = None
    if active and mode == 'register-mismatch':
        witness['state'] = {'failed': 'script check changed a saved register: pc'}
    check_path = Path('pause-check.json')
    if check_path.exists():
        request = json.loads(check_path.read_text())
        if request['generation'] > witness['generation']:
            witness['generation'] = request['generation']
            publish('session-paused.json', witness)
    time.sleep(.005)
