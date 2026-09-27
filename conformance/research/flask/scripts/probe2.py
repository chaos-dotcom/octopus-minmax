
import sys
sys.path.insert(0, "/Users/chaos/octopus-minmax/conformance/research/flask/scripts")
from wire import request
req = b"GET / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic\r\n\r\n"
print("SENDING:", repr(req))
r = request(5050, req)
print("GOT:", repr(r[:120]))
req2 = b"HEAD / HTTP/1.1\r\nHost: 127.0.0.1:5050\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n"
print("SENDING:", repr(req2))
print("GOT:", repr(request(5050, req2)[:120]))
