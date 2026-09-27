import os

#  The bot will declare its version in the welcome message.
# Updated by the release pipeline. Change manually if building from source
BOT_VERSION = "v.local"
# Add your stuff here
API_KEY = os.getenv("API_KEY", "")
# Your Octopus Energy account number. Starts with A-
ACC_NUMBER = os.getenv("ACC_NUMBER", "")
BASE_URL = os.getenv("BASE_URL", "https://api.octopus.energy/v1")
# Comma-separated list of Apprise notification URLs
NOTIFICATION_URLS = os.getenv("NOTIFICATION_URLS", "")
# Whether to send all the notifications as a batch or individually
BATCH_NOTIFICATIONS = os.getenv("BATCH_NOTIFICATIONS", "false") in ["true", "True", "1"]

EXECUTION_TIME = os.getenv("EXECUTION_TIME", "23:00")

# A threshold (in pence) over which the difference between the tariffs must be before the switch happens.
SWITCH_THRESHOLD = int(os.getenv("SWITCH_THRESHOLD", 2))

# List of tariff IDs to compare
TARIFFS = os.getenv("TARIFFS", "go,agile,flexible")

# Whether to just run immediately and exit
ONE_OFF_RUN = os.getenv("ONE_OFF", "false") in ["true", "True", "1"]
ONE_OFF_EXECUTED = False

# Whether to notify the user of a switch but not actually switch
DRY_RUN = os.getenv("DRY_RUN", "false") in ["true", "True", "1"]

# Web UI authentication
WEB_USERNAME = os.getenv("WEB_USERNAME", "admin")
WEB_PASSWORD = os.getenv("WEB_PASSWORD", "admin")
WEB_PORT = int(os.getenv("WEB_PORT", 5050))

# --- Consumption source -----------------------------------------------------
# Where the bot reads today's electricity usage from.
#   "homeassistant" - half-hourly grid import read from Home Assistant (no Home Mini needed)
#   "octopus"       - the Octopus smartMeterTelemetry feed (requires an Octopus Home Mini)
CONSUMPTION_SOURCE = os.getenv("CONSUMPTION_SOURCE", "homeassistant").strip().lower()

# Home Assistant API (used when CONSUMPTION_SOURCE == "homeassistant").
# Running as a Home Assistant add-on these default to the Supervisor proxy.
HA_URL = os.getenv("HA_URL", "http://supervisor/core/api").rstrip("/")
HA_TOKEN = os.getenv("HA_TOKEN") or os.getenv("SUPERVISOR_TOKEN", "")
# Cumulative grid-import energy sensor (kWh). Half-hourly deltas are derived from it.
HA_IMPORT_ENTITY = os.getenv("HA_IMPORT_ENTITY", "sensor.predbat_givtcp_0_import_total")
# Timezone used to work out "today" (should match the Home Assistant instance).
TIMEZONE = os.getenv("TZ", "Europe/London")
