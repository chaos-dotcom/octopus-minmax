import signal
signal.alarm(120)
import json
from apprise.manager_plugins import PluginManager
pm = PluginManager()
d = pm._schema_map or {}
out = {k: (v.__module__ if v else None) for k, v in d.items()}
print(json.dumps(out, sort_keys=True))
