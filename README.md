# Octopus Minmax Bot 🐙🤖

## Rust rewrite (branch `rust`)

This branch replaces the Python implementation with a Rust implementation that is
byte-compatible with it.  `src/` (Python) is kept in the tree as the reference the
conformance suite compares against; the shipped artefact is the Rust binary, and the
Dockerfile builds it.

### Build and run

```bash
cargo build --release --manifest-path rust/Cargo.toml
cd /some/working/directory          # logs/octobot.log is written relative to the CWD
/path/to/octo-minmax
```

Environment variables, log file (`logs/octobot.log`, rotated at 10 MiB with 5
backups), notifications, the web dashboard on port 5050 and the GraphQL/REST traffic
to Octopus are all identical to the Python version - including the byte-level details
(header order, `json.dumps` spacing, Python `repr()` in log lines, Werkzeug error
pages, the Flask flash cookie).

The one intentional difference is the `Server:` response header, which identifies the
HTTP implementation.  Set `OCTO_SERVER_HEADER` to reproduce the reference value.

### Conformance suite

`conformance/` runs both implementations through the same scenarios against mock
Octopus Energy, Home Assistant and Apprise endpoints, records every byte each one
produces (requests on the wire, notifications, logs, stdout/stderr, web responses)
and diffs the two runs:

```bash
python3 conformance/run_all.py --impl py --out /tmp/art/py
python3 conformance/run_all.py --impl rs --out /tmp/art/rs
python3 conformance/compare.py --reference /tmp/art/py --candidate /tmp/art/rs
```

`conformance/measure.py` reports deployment size and resident memory for both.

Current result: **306/306 artifacts byte-identical over 18 scenarios** (see
`conformance/RESULTS.md` for the normalization list and the accepted differences).

| | Python | Rust |
|---|---|---|
| container image | 269 MB | 141 MB |
| runtime on disk (source + deps / binary) | 20.7 MiB | 2.74 MiB |
| resident memory, idle dashboard | 49.5 MiB | 6.5 MiB |

## Description
This bot will use your electricity usage and compare your current Smart tariff costs for the day with another smart tariff and initiate a switch if it's cheaper. See below for supported tariffs.

Due to how Octopus Energy's Smart tariffs work, switching manually makes the *new* tariff take effect from the start of the day. For example, if you switch at 11 PM, the whole day's costs will be recalculated based on your new tariff, allowing you to potentially save money by tariff-hopping.

I created this because I've been a long-time Agile customer who got tired of the price spikes. I now use this to enjoy the benefits of Agile (cheap days) without the risks (expensive days).

I personally have this running automatically every day at 11 PM inside a Raspberry Pi Docker container, but you can run it wherever you want.  It sends notifications and updates to a variety of services via [Apprise](https://github.com/caronc/apprise), but that's not required for it to work.

## Web Dashboard

After starting the bot you can access the web dashboard on `localhost:5050`

- Make changes to your config through the dashboard without needing to restart
- Access and read logs
- See graph of savings (coming soon)

## How to Use

### Requirements
- An Octopus Energy Account
  - In case you don't have one, we both get £50 for using my referral: https://share.octopus.energy/coral-lake-50
  - Get your API key [here](https://octopus.energy/dashboard/new/accounts/personal-details/api-access)
- A smart meter
- Be on a supported Octopus Smart Tariff (see tariffs below)
- Today's usage, from one of:
  - An energy sensor in **Home Assistant** (recommended, and the default). See [Consumption Source](#consumption-source) below - no Octopus hardware needed.
  - An Octopus Home Mini, if you prefer to read usage from Octopus directly. Request one from Octopus Energy for free [here](https://octopus.energy/blog/octopus-home-mini/).

### HomeAssistant Addon

[![Open your Home Assistant instance and show the add add-on repository dialog with a specific repository URL pre-filled.](https://my.home-assistant.io/badges/supervisor_add_addon_repository.svg)](https://my.home-assistant.io/redirect/supervisor_add_addon_repository/?repository_url=https%3A%2F%2Fgithub.com%2Feelmafia%2Foctopus-minmax)

OR

To install this third-party add-on:

1. Open Home Assistant > Settings > Add-ons > Add-on Store.
2. Click the menu (three dots in the top-right corner) and select Repositories.
3. Paste the GitHub repository link into the field at the bottom:
https://github.com/eelmafia/octopus-minmax
4. Refresh the page if needed. The add-on will appear under **Octopus MinMax Bot**.


### Running Manually
1. Install the Python requirements.
2. Configure the environment variables.
3. Run `main.py`. I recommend scheduling it to run it at 11 PM in order to leave yourself an hour as a safety margin in case Octopus takes a while to generate your new agreement.

### Running using Docker
Docker run command:
```
docker run -d \
  --name MinMaxOctopusBot \
  -p 5050:5050 \
  -v ./logs:/app/logs \
  -e ACC_NUMBER="<your_account_number>" \
  -e API_KEY="<your_api_key>" \
  -e EXECUTION_TIME="23:00" \
  -e SWITCH_THRESHOLD=2 \
  -e NOTIFICATION_URLS="<apprise_notification_urls>" \
  -e ONE_OFF=false \
  -e DRY_RUN=false \
  -e TARIFFS=cosy-fix,cosy,agile,flexible \
  -e TZ=Europe/London \
  -e BATCH_NOTIFICATIONS=false \
  -e WEB_USERNAME="<whatever_you_want>" \
  -e WEB_PASSWORD="<whatever_you_want>" \
  eelmafia/octopus-minmax-bot
```
or use the docker-compose.yaml **Don't forget to add your environment variables**

Note : Remove the --restart unless line if you set the ONE_OFF variable or it will continuously run.

#### Environment Variables
| Variable                    | Description                                                                                                                                                                                                             |
|-----------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `ACC_NUMBER`                | Your Octopus Energy account number.                                                                                                                                                                                     |
| `API_KEY`                   | API token for accessing your Octopus Energy account.                                                                                                                                                                    |
| `TARIFFS`                   | A list of tariffs to compare against. Default is `cosy-fix,cosy,agile,go,flexible`. **It must include the tariff you are currently on**, otherwise the bot cannot identify your current tariff. |
| `CONSUMPTION_SOURCE`        | Where to read today's usage from: `homeassistant` (default) or `octopus`. See [Consumption Source](#consumption-source). |
| `HA_URL`                    | Home Assistant API base URL. Defaults to the add-on Supervisor proxy (`http://supervisor/core/api`). For Docker use e.g. `https://your-home-assistant:8123/api`. |
| `HA_TOKEN`                  | Home Assistant long-lived access token. Not needed when running as an add-on (the Supervisor token is used automatically). |
| `HA_IMPORT_ENTITY`          | The cumulative grid-import energy sensor (kWh) to read usage from, e.g. your GivTCP/inverter import total. |
| `EXECUTION_TIME`            | (Optional) The time (HH:MM) when the script should execute. Default is `23:00` (11 PM).                                                                                                                                 |
| `SWITCH_THRESHOLD`          | A value (in pence) which the saving must be before the switch occurs. Default is `2` (2p). |
| `NOTIFICATION_URLS`         | (Optional) A comma-separated list of [Apprise](https://github.com/caronc/apprise) notification URLs for sending logs and updates.  See [Apprise documentation](https://github.com/caronc/apprise/wiki) for URL formats. |
| `ONE_OFF`                   | (Optional) A flag for you to simply trigger an immediate execution instead of starting scheduling.                                                                                                                      |
| `DRY_RUN`                   | (Optional) A flag to compare but not switch tariffs.                                                                                                                                                                    |
| `BATCH_NOTIFICATIONS`       | (Optional) A flag to send messages in one batch rather than individually.                                                                                                                                               |
| `WEB_USERNAME`              | (Optional) Defaults to `admin`. Auth for the web dashboard.
| `WEB_PASSWORD`              | (Optional) Defaults to `admin`. Auth for the web dashboard.
| `WEB_PORT`                  | (Optional) Defaults to `5050`.

*Reminder: Change the password to something else other than default. It's not meant to be secure, it's just there to stop others on your network from accessing the dashboard and your API key. If they have access to your compose/config files you're already cooked.*

#### Supported Tariffs

Below is a list of supported tariffs, their IDs (to use in environment variables), and whether they are switchable.

**None switchable tariffs are use for PRICE COMPARISON ONLY**

| Tariff Name      | Tariff ID | Switchable |
|------------------|-----------|------------|
| Flexible Octopus | flexible  | ❌          |
| Agile Octopus    | agile     | ✅          |
| Cosy Octopus     | cosy      | ✅          |
| Octopus Go       | go        | ✅          |


#### Consumption Source

The bot needs today's half-hourly electricity usage to work out what the day would have cost on each tariff.

By default (`CONSUMPTION_SOURCE=homeassistant`) it reads this from an energy sensor **in your Home Assistant instance** - the cumulative grid-import counter from your inverter/CT clamp (for example GivTCP). This is the same electricity your meter records (empirically within ~1% where a GivTCP CT sits on the grid supply), it is available on the day, and it does **not** require an Octopus Home Mini.

Point `HA_IMPORT_ENTITY` at your cumulative import counter; the bot turns it into half-hourly slots for costing. When running as an add-on, no token is needed - the Supervisor API is used. For Docker, set `HA_URL` and `HA_TOKEN`.

Set `CONSUMPTION_SOURCE=octopus` to use Octopus's `smartMeterTelemetry` feed instead, which requires an Octopus Home Mini.

*Note: the accuracy of time-of-use costing depends on how finely your import sensor logs - a counter with whole-kWh resolution will land usage in slightly the wrong half-hour. A CT-based counter (e.g. GivTCP) is recommended.*

#### Setting up Apprise Notifications

The `NOTIFICATION_URLS` environment variable allows you to configure notifications using the powerful [Apprise](https://github.com/caronc/apprise) library.  Apprise supports a wide variety of notification services, including Discord, Telegram, Slack, email, and many more.

To configure notifications:

1.  **Determine your desired notification services:**  Decide which services you want to receive notifications on (e.g., Discord, Telegram).

2.  **Find the Apprise URL format for each service:**  Consult the [Apprise documentation](https://github.com/caronc/apprise/wiki) to find the correct URL format for each service you've chosen.  For example:

    *   **Discord:** `discord://webhook_id/webhook_token`
    *   **Telegram:** `tgram://bottoken/ChatID`

3.  **Set the `NOTIFICATION_URLS` environment variable:** Create a comma-separated string containing the Apprise URLs for all your desired services.  For example:

    ```bash
    NOTIFICATION_URLS="discord://webhook_id/webhook_token,tgram://bottoken/ChatID,mailto://user:pass@example.com?to=recipient@example.com"
    ```

    Make sure to replace the example values with your actual credentials.

4.  **Restart the container (if using Docker) or run the script:**  The bot will now send notifications to all the configured services.
