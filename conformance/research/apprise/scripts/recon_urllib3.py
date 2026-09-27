import signal
signal.alarm(120)
import urllib3, requests, inspect
print("urllib3", urllib3.__version__, "requests", requests.__version__)
import urllib3.connection as uc
print(inspect.getsource(uc.HTTPConnection._new_conn))
print(inspect.getsource(uc.HTTPConnection.connect))
try:
    import cryptography; print("cryptography", cryptography.__version__)
except Exception as e:
    print("no cryptography:", e)
import ssl; print("openssl", ssl.OPENSSL_VERSION)
