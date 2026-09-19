#!/usr/bin/env python3
"""Production verification now exercises the async Game session contract (SDK-521)."""
from pathlib import Path
import subprocess
import sys

if __name__ == '__main__':
    raise SystemExit(subprocess.call([sys.executable, str(Path(__file__).with_name('check-game-sessions.py')), '--production', *sys.argv[1:]]))
