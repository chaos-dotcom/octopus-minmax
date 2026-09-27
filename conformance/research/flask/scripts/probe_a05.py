
import sys, os, socket
sys.path.insert(0, "/Users/chaos/octopus-minmax/conformance/research/flask/scripts")
from wire import request
req = b"GET / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic\r\n\r\n"
for i in range(5):
    r = request(5050, req)
    print(i, repr(r[:60]))
