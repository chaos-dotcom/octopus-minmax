## v1.2.0 - v1.2.0 - the Rust implementation
## v1.2.0 - the Rust implementation

The bot is now a single Rust binary. It is **byte-identical** to the Python version it
replaces: the same GraphQL/REST traffic, the same log file, the same notifications, the
same dashboard bytes (including the Flask flash cookie and Werkzeug's error pages).

### Verified
* 306/306 conformance artifacts identical across 18 scenarios - requests on the wire,
  notifications, logs, stdout/stderr and every dashboard response. See
  `conformance/RESULTS.md` for the normalization ledger and the accepted differences.
* 25 unit tests (`cd rust && cargo test`), including Apprise 1.9.2 wire captures and
  captured Flask session cookies.

### Smaller and cheaper

| | Python (v1.1.0) | Rust (v1.2.0) |
|---|---|---|
| container image | 269 MB | 141 MB |
| runtime on disk | 20.7 MiB | 2.74 MiB |
| resident memory (idle dashboard) | 49.5 MiB | 6.5 MiB |
| CPU per dashboard request | 443 us | 47 us |
| start-up CPU | 0.096 s | 0.0037 s |

### Configuration
Environment variables, the web dashboard, `logs/octobot.log` (10 MiB rotation, 5
backups), notifications and tariff behaviour are unchanged. The one visible difference
is the `Server:` response header, which identifies the HTTP implementation; set
`OCTO_SERVER_HEADER=Werkzeug/3.1.9 Python/3.11.15` to reproduce the old value byte for
byte.

## v1.1.0 - v1.1.0
Home Assistant consumption source (no Octopus Home Mini required); recognise Cosy FIX; GHCR image publishing.

## v1.0.9 - v1.0.9
## What's Changed
* Update Addon Configuration to v1.0.8 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/163
* Incorrect port number in readme.md by @Morph-Ed in https://github.com/eelmafia/octopus-minmax/pull/165
* Add new tariff option by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/177

## New Contributors
* @Morph-Ed made their first contribution in https://github.com/eelmafia/octopus-minmax/pull/165

**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.8...v1.0.9

## v1.0.8 - v1.0.8
## What's Changed
* Update Addon Configuration to v1.0.7 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/159
* Update SWITCH_THRESHOLD type to int in config.yaml (fixes #160) by @DJBenson in https://github.com/eelmafia/octopus-minmax/pull/161
* Log notifications regardless regardless if apprise exists by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/162


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.7...v1.0.8

## v1.0.7 - v1.0.7
## What's Changed
* Update Addon Configuration to v1.0.6 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/157
* Fix indents by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/158


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.6...v1.0.7

## v1.0.6 - v1.0.6
## What's Changed
* Update Addon Configuration to v1.0.5 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/148
* Restore legacy:true to config.yaml (fixes #141) by @DJBenson in https://github.com/eelmafia/octopus-minmax/pull/154
* Update notification_service.py by @DJBenson in https://github.com/eelmafia/octopus-minmax/pull/156


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.5...v1.0.6

## v1.0.5 - v1.0.5
## What's Changed
* Update Addon Configuration to v1.0.4 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/146
* Ingress fixes by @DJBenson in https://github.com/eelmafia/octopus-minmax/pull/147


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.4...v1.0.5

## v1.0.4 - v1.0.4
## What's Changed
* Update Addon Configuration to v1.0.3 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/143
* Add missing authentication fields by @DJBenson in https://github.com/eelmafia/octopus-minmax/pull/145


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.3...v1.0.4

## v1.0.3 - v1.0.3
## What's Changed
* Update Addon Configuration to v1.0.1 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/138
* Update Addon Configuration to v1.0.2 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/139
* Fix web dashboard paths in HA by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/142


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.2...v1.0.3

## v1.0.2 - v1.0.2
**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.1...v1.0.2
## v1.0.1 - v1.0.1
## What's Changed
* Update Addon Configuration to v1.0.0 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/136
* Fix path in workflow script by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/137


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v1.0.0...v1.0.1

## v1.0.0 - v1.0.0
## What's Changed
* Refactor code and add web UI by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/128


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.8.5...v1.0.0

## v0.8.5 - v0.8.5
## What's Changed
* Improve cosy regex to avoid fixed tariff by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/119


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.8.4...v0.8.5

## v0.8.4 - v0.8.4
## What's Changed
* Update Addon Configuration to v0.8.3 by @github-actions[bot] in https://github.com/eelmafia/octopus-minmax/pull/112
* Fix switch threshold type by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/115


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.8.3...v0.8.4

## v0.8.3 - v0.8.3
## What's Changed
* Add config entry/environment variable for switch threshold, defaults to 2p by @DJBenson in https://github.com/eelmafia/octopus-minmax/pull/111

## New Contributors
* @DJBenson made their first contribution in https://github.com/eelmafia/octopus-minmax/pull/111

**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.8.2...v0.8.3

## v0.8.2 - v0.8.2
## What's Changed
* Update tariff.py by @adyoull in https://github.com/eelmafia/octopus-minmax/pull/99


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.8.1...v0.8.2

## v0.8.1 - v0.8.1
## What's Changed
* Replace gql with requests by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/88

## New Contributors

**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.8.0...v0.8.1

## v0.8.0 - v0.8.0
## What's Changed
* Update README.md by @adyoull in https://github.com/eelmafia/octopus-minmax/pull/75
* Add HA Addon Config by @joeShuff in https://github.com/eelmafia/octopus-minmax/pull/76
* Add option to batch notifications by @lilongwe in https://github.com/eelmafia/octopus-minmax/pull/77
* Fix timeout errors by @eelmafia in https://github.com/eelmafia/octopus-minmax/pull/84


**Full Changelog**: https://github.com/eelmafia/octopus-minmax/compare/v0.7.2...v0.8.0

## Initial Release

This is the initial release of Octopus MinMax Bot