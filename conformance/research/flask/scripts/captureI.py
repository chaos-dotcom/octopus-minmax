
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from wire import build, cap
PORT = 5050
cap(PORT, "I01_put-config-405", build("PUT", "/config", body=b"", headers=["Content-Length: 0"]), "PUT /config -> 405 Allow list")
cap(PORT, "I02_delete-logs-405", build("DELETE", "/logs"), "DELETE /logs -> 405 Allow list")
