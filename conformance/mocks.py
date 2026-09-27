"""Scenario fixtures and mock endpoints for the conformance harness.

Every response is generated from today's date so that two runs on the same day see
identical bytes.  All requests are captured verbatim by conformance/rawhttp.py.
"""
import json
from datetime import date, datetime, timedelta

OCTOPUS_PORT = 18080
HA_PORT = 18123
SINK_PORT = 18200

PRODUCTS = [
    # (display_name, code, direction)
    ("Octopus Go", "GO-VAR-22-10-14", "IMPORT"),
    ("Octopus Go 12M Fixed", "GO-FIX-12M-26-06-25", "IMPORT"),
    ("Agile Octopus", "AGILE-24-10-01", "IMPORT"),
    ("Cosy Octopus", "COSY-22-06-08", "IMPORT"),
    ("Cosy Octopus 12M Fixed", "COSY-FIX-12M-26-06-25", "IMPORT"),
    ("Flexible Octopus", "VAR-22-11-01", "IMPORT"),
    # decoys: same display name but an export product, and a different name
    ("Agile Octopus", "AGILE-EXPORT-24-10-01", "EXPORT"),
    ("Octopus Go", "GO-EXP-22-10-14", "EXPORT"),
    ("Some Other Tariff", "OTHER-24-01-01", "IMPORT"),
]

# Unit rates per product code, as (value_inc_vat, start_hour, end_hour) covering today.
# Non-overlapping so that boundary matching is unambiguous.
RATES = {
    "COSY-22-06-08":          [(30.0, 0, 1), (20.0, 1, 2), (25.0, 2, 24)],
    "COSY-FIX-12M-26-06-25":  [(28.0, 0, 1), (18.0, 1, 2), (23.0, 2, 24)],
    "AGILE-24-10-01":         [(45.0, 0, 1), (35.0, 1, 2), (40.0, 2, 24)],
    "GO-VAR-22-10-14":        [(12.0, 0, 1), (10.0, 1, 2), (32.0, 2, 24)],
    "GO-FIX-12M-26-06-25":    [(11.0, 0, 1), (9.0, 1, 2), (31.0, 2, 24)],
    "VAR-22-11-01":           [(26.0, 0, 1), (24.0, 1, 2), (27.0, 2, 24)],
}

STANDING_CHARGES = {
    "COSY-22-06-08": 53.0,
    "COSY-FIX-12M-26-06-25": 60.0,
    "AGILE-24-10-01": 50.0,
    "GO-VAR-22-10-14": 48.0,
    "GO-FIX-12M-26-06-25": 47.0,
    "VAR-22-11-01": 55.0,
}

# Today's half-hourly telemetry (octopus consumption source).
TELEMETRY = [
    ("00:30:00", 250.0, 75.0),
    ("01:00:00", 300.0, 60.0),
    ("01:30:00", 200.0, 50.0),
]


class Fixtures:
    def __init__(self, scenario):
        self.scenario = scenario
        self.today = date.today()
        self.gql_calls = 0
        self.products_calls = 0

    # -- helpers -----------------------------------------------------------
    def _day(self, offset=0):
        return (self.today + timedelta(days=offset)).isoformat()

    def rates_for(self, code, day_offset=0):
        out = []
        for value, start, end in RATES[code]:
            valid_from = "{}T{:02d}:00:00Z".format(self._day(day_offset), start)
            if end == 24:
                valid_to = "{}T00:00:00Z".format(self._day(day_offset + 1))
            else:
                valid_to = "{}T{:02d}:00:00Z".format(self._day(day_offset), end)
            out.append({"value_inc_vat": value, "valid_from": valid_from,
                        "valid_to": valid_to, "payment_method": None})
            # a rate the consumer must ignore
            out.append({"value_inc_vat": value + 5.0, "valid_from": valid_from,
                        "valid_to": valid_to, "payment_method": "NON_DIRECT_DEBIT"})
        return out

    def rates_url(self, code):
        return "http://127.0.0.1:{}/v1/products/{}/electricity-tariffs/E-1R-{}-H/standard-unit-rates/".format(
            OCTOPUS_PORT, code, code)

    # -- graphql -----------------------------------------------------------
    def graphql(self, query):
        self.gql_calls += 1
        if "obtainKrakenToken" in query:
            return {"data": {"obtainKrakenToken": {"token": "test-token-0123456789abcdef"}}}
        if "termsAndConditionsForProduct" in query:
            return {"data": {"termsAndConditionsForProduct": {"name": "Terms", "version": "2.3"}}}
        if "acceptTermsAndConditions" in query:
            return {"data": {"acceptTermsAndConditions": {"acceptedVersion": "2.3"}}}
        if "startOnboardingProcess" in query:
            return {"data": {"startOnboardingProcess": {
                "onboardingProcess": {"id": "OP-1234"},
                "productEnrolment": {"id": "PE-5678"}}}}
        if "smartMeterTelemetry" in query:
            return {"data": {"smartMeterTelemetry": self.telemetry()}}
        if "account(" in query:
            stale = self.gql_calls in (self.scenario.get("gql_stale_at") or [])
            return {"data": {"account": {"electricityAgreements": self.agreements(stale)}}}
        raise AssertionError("unexpected graphql query: " + query[:120])

    def telemetry(self):
        sc = self.scenario
        override = sc.get("telemetry_costs_override")
        out = []
        for index, (time_part, delta, cost) in enumerate(TELEMETRY):
            if override:
                cost = override[index]
            entry = {"readAt": "{}T{}".format(self._day(), time_part),
                     "consumptionDelta": delta}
            if sc.get("telemetry_costs", True):
                entry["costDeltaWithTax"] = cost
            else:
                entry["costDeltaWithTax"] = None
            out.append(entry)
        return out

    def agreements(self, stale=False):
        sc = self.scenario
        agreements = []
        if sc.get("import_meter", True):
            codes = sc.get("current_tariff_codes") or ["E-1R-COSY-22-06-08-H"]
            for code in codes:
                agreements.append({
                    "validFrom": "{}T00:00:00+00:00".format(self._day(-2 if stale else 0)),
                    "validTo": None,
                    "meterPoint": {
                        "mpan": "1900012345678",
                        "direction": "IMPORT",
                        "meters": [{"smartDevices": (
                            [{"deviceId": "DEVICE-0001"}]
                            if not sc.get("no_device") else [{"noDeviceId": True}])}],
                    },
                    "tariff": {"id": code, "productCode": sc.get("current_product", "COSY-22-06-08"),
                               "tariffCode": code,
                               "standingCharge": sc.get("standing_charge", 53.0)},
                })
        if sc.get("export_meter", True):
            agreements.append({
                "validFrom": "{}T00:00:00+00:00".format(self._day(-2 if stale else 0)),
                "validTo": None,
                "meterPoint": {"mpan": "1900087654321", "direction": "EXPORT",
                               "meters": [{"smartDevices": [{"deviceId": "DEVICE-EXPORT"}]}]},
                "tariff": {"id": "E-1R-EXPORT-1", "productCode": "EXPORT-1",
                           "tariffCode": "E-1R-EXPORT-1", "standingCharge": 0.0},
            })
        if sc.get("no_tariff_code"):
            import copy
            agreements[0] = copy.deepcopy(agreements[0])
            agreements[0]["tariff"]["tariffCode"] = None
        return agreements

    # -- rest --------------------------------------------------------------
    def products(self):
        results = []
        for display, code, direction in PRODUCTS:
            results.append({"code": code, "display_name": display, "direction": direction,
                            "is_variable": True, "brand": "OCTOPUS_ENERGY",
                            "available_from": "2020-01-01T00:00:00+00:00",
                            "available_to": None})
        return {"count": len(results), "next": None, "previous": None, "results": results}

    def product_detail(self, code):
        if code not in RATES:
            return {"code": code, "display_name": "Unknown", "direction": "IMPORT"}
        return {
            "code": code,
            "display_name": dict((c, d) for d, c, _ in PRODUCTS).get(code, code),
            "direction": "IMPORT",
            "is_variable": True,
            "single_register_electricity_tariffs": {
                "_H": {
                    "direct_debit_monthly": {
                        "code": "E-1R-{}-H".format(code),
                        "standing_charge_inc_vat": STANDING_CHARGES[code],
                        "links": [
                            {"href": self.rates_url(code), "rel": "standard_unit_rates",
                             "method": "GET"},
                            {"href": "http://127.0.0.1:{}/v1/products/{}/".format(OCTOPUS_PORT, code),
                             "rel": "self", "method": "GET"},
                        ],
                    },
                    "varying": {
                        "code": "E-2R-{}-H".format(code),
                        "standing_charge_inc_vat": STANDING_CHARGES[code] + 1.0,
                        "links": [],
                    },
                },
            },
        }

    def ha_history(self):
        """A cumulative kWh counter that rises during the first two hours of the day."""
        # local midnight in UTC: the harness pins TZ=Europe/London
        from zoneinfo import ZoneInfo
        tz = ZoneInfo("Europe/London")
        midnight_local = datetime.now(tz).replace(hour=0, minute=0, second=0, microsecond=0)
        base = 1000.0
        samples = []
        for sample_minute in range(0, 24 * 60, 15):
            moment_local = midnight_local + timedelta(minutes=sample_minute)
            if sample_minute <= 15:
                value = base
            elif sample_minute <= 45:
                value = base + 2.0
            elif sample_minute <= 75:
                value = base + 5.0
            elif sample_minute <= 105:
                value = base + 7.0
            else:
                value = base + 7.0
            samples.append({
                "entity_id": "sensor.test_import_total",
                "state": "{:.3f}".format(value),
                "last_updated": moment_local.astimezone(ZoneInfo("UTC")).strftime("%Y-%m-%dT%H:%M:%S+00:00"),
                "last_changed": moment_local.astimezone(ZoneInfo("UTC")).strftime("%Y-%m-%dT%H:%M:%S+00:00"),
            })
        return [samples]
