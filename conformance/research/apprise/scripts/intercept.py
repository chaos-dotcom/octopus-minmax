"""Redirect selected remote hostnames to the local capture sink.

Only the TCP connect target is changed.  The urllib3 connection object keeps
its original host/port, so the Host header, the request target and the TLS SNI
are exactly what apprise would have produced in production.

Set APPRISE_CAPTURE_MAP to a JSON object {"hostname": port} to change the map.
"""
import json
import os

import urllib3.util.connection as u3c

DEFAULT_MAP = {
    "discord.com": 18810,
    "hooks.slack.com": 18811,
    "slack.com": 18811,
    "api.telegram.org": 18812,
    "api.pushover.net": 18813,
    "ntfy.sh": 18814,
}
MAP = dict(DEFAULT_MAP)
_env = os.environ.get("APPRISE_CAPTURE_MAP")
if _env:
    MAP.update({k: int(v) for k, v in json.loads(_env).items()})

_orig = u3c.create_connection
LOG = []


def patched(address, *args, **kwargs):
    host = address[0]
    if host in MAP:
        LOG.append(host)
        address = ("127.0.0.1", MAP[host])
    return _orig(address, *args, **kwargs)


u3c.create_connection = patched
