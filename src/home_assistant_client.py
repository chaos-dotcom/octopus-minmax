"""Reads today's electricity usage from Home Assistant.

Replaces the Octopus ``smartMeterTelemetry`` feed (which only works with an
Octopus Home Mini) with the grid-import energy sensor that is already in Home
Assistant - for example the GivTCP inverter's CT clamp. The sensor is a
cumulative kWh counter; this module turns it into half-hourly consumption in
the shape the comparison engine expects.
"""
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo
from urllib.parse import quote
import logging

import requests

import config

logger = logging.getLogger('octobot.home_assistant')


def _local_tz():
    try:
        return ZoneInfo(config.TIMEZONE)
    except Exception as exc:  # noqa: BLE001 - unknown TZ, fall back to UTC
        logger.warning(f"Could not load timezone '{config.TIMEZONE}' ({exc}); using UTC")
        return timezone.utc


class HomeAssistantClient:
    def __init__(self, base_url: str = None, token: str = None, import_entity: str = None):
        logger.debug(f"Initialising {__class__.__name__}")
        base = (base_url or config.HA_URL).rstrip('/')
        # Accept either a bare host (https://ha.example) or the API base
        # (http://supervisor/core/api).
        if not base.endswith('/api'):
            base = f"{base}/api"
        self.base_url = base
        self.token = token or config.HA_TOKEN
        self.import_entity = import_entity or config.HA_IMPORT_ENTITY
        self.headers = {
            'Authorization': f'Bearer {self.token}',
            # Some Home Assistant instances sit behind Cloudflare, which rejects
            # requests with no User-Agent.
            'User-Agent': 'octopus-minmax-bot/1.0',
            'Accept': 'application/json',
        }

    def get_today_half_hourly_consumption(self) -> list:
        """Return today's grid import as half-hourly slots.

        Each slot is ``{'readAt': <UTC ISO 'Z'>, 'consumptionDelta': <Wh>,
        'costDeltaWithTax': None}`` so it is a drop-in for the telemetry feed.
        """
        if not self.token:
            raise Exception(
                "No Home Assistant token available. Set HA_TOKEN, or run as a "
                "Home Assistant add-on with homeassistant_api enabled."
            )

        tz = _local_tz()
        now = datetime.now(tz)
        local_midnight = now.replace(hour=0, minute=0, second=0, microsecond=0)
        start_utc = local_midnight.astimezone(timezone.utc)
        end_utc = now.astimezone(timezone.utc)

        samples = self._fetch_history(self.import_entity, start_utc, end_utc)
        if not samples:
            raise Exception(
                f"Home Assistant returned no history for '{self.import_entity}'. "
                f"Check HA_IMPORT_ENTITY and that the entity has history enabled."
            )
        samples.sort(key=lambda sample: sample[0])

        # Walk half-hour slots aligned to the meter's :00/:30 boundaries.
        slot_start = start_utc.replace(second=0, microsecond=0)
        slot_start = slot_start.replace(minute=30 if slot_start.minute >= 30 else 0)

        previous_value = self._value_at(samples, slot_start)
        slots = []
        while slot_start < end_utc:
            slot_end = slot_start + timedelta(minutes=30)
            end_value = self._value_at(samples, slot_end)
            if previous_value is not None and end_value is not None:
                delta_kwh = end_value - previous_value
                if delta_kwh > 0:  # ignore counter resets / negative deltas
                    slots.append({
                        'readAt': slot_end.strftime('%Y-%m-%dT%H:%M:%SZ'),
                        'consumptionDelta': delta_kwh * 1000,  # kWh -> Wh
                        'costDeltaWithTax': None,
                    })
            previous_value = end_value
            slot_start = slot_end

        total_kwh = sum(slot['consumptionDelta'] for slot in slots) / 1000
        logger.info(
            f"Home Assistant '{self.import_entity}': {len(slots)} half-hour slots, "
            f"{total_kwh:.2f} kWh since {local_midnight:%Y-%m-%d %H:%M}"
        )
        return slots

    def _fetch_history(self, entity_id: str, start, end):
        """Return [(utc_datetime, float_value)] for a numeric entity."""
        url = f"{self.base_url}/history/period/{quote(start.isoformat(), safe='')}"
        params = {
            'filter_entity_id': entity_id,
            'end_time': end.isoformat(),
            'minimal_response': '',
        }
        logger.debug(f"Fetching HA history: {url} {params}")
        response = requests.get(url, headers=self.headers, params=params, timeout=60)
        if not response.ok:
            raise Exception(f"Home Assistant history request failed: {response.status_code} {response.text[:200]}")
        try:
            data = response.json()
        except ValueError:
            raise Exception(
                f"Home Assistant history request returned non-JSON (is HA_URL correct?): "
                f"{response.status_code} {response.text[:200]}"
            )
        if not data:
            return []

        points = []
        for entry in data[0]:
            state = entry.get('state')
            if state in (None, 'unknown', 'unavailable', ''):
                continue
            try:
                value = float(state)
            except (TypeError, ValueError):
                continue
            stamp = entry.get('last_updated') or entry.get('last_changed')
            if not stamp:
                continue
            points.append(
                (datetime.fromisoformat(stamp.replace('Z', '+00:00')).astimezone(timezone.utc), value)
            )
        return points

    @staticmethod
    def _value_at(samples, moment):
        """Counter value at (or before) ``moment``; None if we have nothing yet."""
        value = None
        for timestamp, sample_value in samples:
            if timestamp <= moment:
                value = sample_value
            else:
                break
        return value
