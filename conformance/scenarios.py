"""Scenario definitions: environment + fixture knobs for one conformance run.

`min_notifications` is the number of Apprise deliveries the run must produce before
the harness moves on to the web-UI captures; `timeout` bounds the whole wait.
"""
from mocks import OCTOPUS_PORT, HA_PORT, SINK_PORT

SINK = "json://127.0.0.1:{}/notify".format(SINK_PORT)
HA_ENV = dict(HA_URL="http://127.0.0.1:{}/api".format(HA_PORT), HA_TOKEN="ha-token-123",
              HA_IMPORT_ENTITY="sensor.test_import_total")

WEB_REQUESTS = [
    ("01_root_auth", b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nConnection: close\r\n\r\n"),
    ("02_root_noauth", b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"),
    ("03_root_badauth", b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic QkFEOmJhZA==\r\nConnection: close\r\n\r\n"),
    ("04_root_ingress", b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Ingress-Path: /api/hassio_ingress/xyz\r\nConnection: close\r\n\r\n"),
    ("05_config_get", b"GET /config HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nConnection: close\r\n\r\n"),
    ("06_logs_get", b"GET /logs HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nConnection: close\r\n\r\n"),
    ("07_missing", b"GET /nope HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nConnection: close\r\n\r\n"),
    ("08_method", b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    ("09_http10", b"GET / HTTP/1.0\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n\r\n"),
    ("10_config_post_bad_time", b"POST /config HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n"
                                b"Content-Type: application/x-www-form-urlencoded\r\nContent-Length: 22\r\nConnection: close\r\n\r\n"
                                b"execution_time=25%3A00"),
    ("11_config_post_bad_threshold", b"POST /config HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Basic YWRtaW46YWRtaW4=\r\n"
                                     b"Content-Type: application/x-www-form-urlencoded\r\nContent-Length: 19\r\nConnection: close\r\n\r\n"
                                     b"switch_threshold=-3"),
]


def octopus(**kw):
    env = dict(CONSUMPTION_SOURCE="octopus", ONE_OFF="true", DRY_RUN="false",
               BATCH_NOTIFICATIONS="false", TARIFFS="cosy,agile,go", NOTIFICATION_URLS=SINK)
    env.update(kw)
    return env


def ha(**kw):
    env = dict(HA_ENV)
    env.update(dict(CONSUMPTION_SOURCE="homeassistant", ONE_OFF="true", DRY_RUN="false",
                    BATCH_NOTIFICATIONS="false", TARIFFS="cosy,agile,go", NOTIFICATION_URLS=SINK))
    env.update(kw)
    return env


SCENARIOS = {
    # The full happy path: compare, switch, accept the agreement, verify it.
    "octopus-switch": dict(
        fixture=dict(telemetry_costs=True),
        env=octopus(),
        min_notifications=7, timeout=260, settle_after=None),

    # The first verification fails, so the retry path (and its two messages) runs.
    "octopus-switch-verify-retry": dict(
        fixture=dict(telemetry_costs=True),
        env=octopus(),
        gql_stale_at=[7], min_notifications=9, timeout=320, settle_after=5),

    # Savings below the threshold: no switch, the "below your threshold" message.
    "octopus-noswitch": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0]),
        env=octopus(SWITCH_THRESHOLD="10"),
        min_notifications=5, timeout=90, settle_after=3),

    # Switch-worthy result but DRY_RUN=true.
    "octopus-dryrun": dict(
        fixture=dict(telemetry_costs=True),
        env=octopus(DRY_RUN="true"),
        min_notifications=6, timeout=90, settle_after=3),

    # The current tariff is the cheapest: the "already on the cheapest tariff" message.
    "octopus-already-cheapest": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0],
                     rates_override={"AGILE-24-10-01": [(200.0, 0, 1), (150.0, 1, 2), (300.0, 2, 24)],
                                     "GO-VAR-22-10-14": [(210.0, 0, 1), (160.0, 1, 2), (310.0, 2, 24)]}),
        env=octopus(),
        min_notifications=5, timeout=90, settle_after=3),

    # Batched notifications: one message, built at the end of the run.
    "octopus-batch": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0]),
        env=octopus(BATCH_NOTIFICATIONS="true", SWITCH_THRESHOLD="10"),
        min_notifications=1, timeout=90, settle_after=6),

    # No notification URLs: the messages go to the log instead.
    "octopus-nonotify": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0]),
        env=octopus(NOTIFICATION_URLS="", SWITCH_THRESHOLD="10"),
        min_notifications=0, timeout=60, settle_after=8),

    # An unknown tariff ID in TARIFFS: a warning notification, and set ordering.
    "octopus-bad-tariff-id": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0]),
        env=octopus(TARIFFS="cosy,agile,nonsense,go", SWITCH_THRESHOLD="10"),
        min_notifications=6, timeout=90, settle_after=3),

    # The current tariff is the grandfathered, non-switchable Cosy 12M Fixed.
    "octopus-current-cosyfix": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0],
                     current_tariff_codes=["E-1R-COSY-FIX-12M-26-06-25-H"],
                     current_product="COSY-FIX-12M-26-06-25", standing_charge=60.0),
        env=octopus(TARIFFS="cosy-fix,cosy,agile,go,flexible", SWITCH_THRESHOLD="20"),
        min_notifications=5, timeout=90, settle_after=3),

    # No IMPORT meter point in the account.
    "octopus-no-import": dict(
        fixture=dict(import_meter=False),
        env=octopus(),
        min_notifications=1, timeout=60, settle_after=6),

    # No smart device on the IMPORT meter.
    "octopus-no-device": dict(
        fixture=dict(telemetry_costs=True, no_device=True),
        env=octopus(),
        min_notifications=1, timeout=60, settle_after=6),

    # A tariff code the bot does not know.
    "octopus-unknown-tariff": dict(
        fixture=dict(telemetry_costs=True, current_tariff_codes=["E-1R-WHATEVER-1-H"]),
        env=octopus(),
        min_notifications=1, timeout=60, settle_after=6),

    # 401 on the second query: the token is refreshed and the query retried.
    "octopus-401-then-ok": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0]),
        env=octopus(SWITCH_THRESHOLD="10"),
        gql_401_at=[2], min_notifications=5, timeout=90, settle_after=3),

    # KT-CT-1124 on the second query: the JWT is refreshed and the query retried.
    "octopus-jwt-expired": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0]),
        env=octopus(SWITCH_THRESHOLD="10"),
        gql_kt1124_at=[2], min_notifications=5, timeout=90, settle_after=3),

    # A tariff whose rates do not cover every consumption period.
    "octopus-missing-rate": dict(
        fixture=dict(telemetry_costs=True, telemetry_costs_override=[4.0, 3.0, 2.0],
                     rates_override={"GO-VAR-22-10-14": [(10.0, 2, 24)]}),
        env=octopus(SWITCH_THRESHOLD="1000"),
        min_notifications=5, timeout=90, settle_after=3),

    # Home Assistant consumption source: usage costed with the current tariff's rates.
    "ha-noswitch": dict(
        fixture=dict(),
        env=ha(SWITCH_THRESHOLD="100"),
        min_notifications=5, timeout=90, settle_after=3),

    # Home Assistant returns no history for the entity.
    "ha-no-history": dict(
        fixture=dict(), ha_empty=True,
        env=ha(),
        min_notifications=1, timeout=60, settle_after=6),

    # No Home Assistant token at all.
    "ha-no-token": dict(
        fixture=dict(),
        env=ha(HA_TOKEN=""),
        min_notifications=1, timeout=60, settle_after=6),
}
