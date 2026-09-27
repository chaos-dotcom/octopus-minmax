import signal
signal.alarm(120)
import apprise, os, json
import apprise.plugins as P
mods = sorted(os.listdir(os.path.dirname(P.__file__)))
print("N", len(mods))
print(json.dumps(mods))
