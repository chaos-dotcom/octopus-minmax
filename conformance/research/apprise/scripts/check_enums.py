import signal
signal.alarm(120)
import apprise
print("NotifyType.INFO =", repr(apprise.NotifyType.INFO))
print("NotifyFormat.TEXT =", repr(apprise.NotifyFormat.TEXT))
print("NotifyType() args:", apprise.NotifyType)
