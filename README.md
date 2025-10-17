# mcc-gaql-rs

[![CI](https://github.com/mhuang74/mcc-gaql-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/mhuang74/mcc-gaql-rs/actions/workflows/rust.yml)

Command line tool to execute Google Ads GAQL queries across MCC child accounts. Inspired by [gaql-cli](https://github.com/getyourguide/gaql-cli).

## Example Usecases

* Query for Asset-based Ad Extensions traffic to see which accounts use them
* Look at adoption trend of Performance Max Campaigns across customer base

## Example commands

```
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql -q recent_campaign_changes -o all_recent_changes.csv
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql -n "campaign changes from last 14 days with current campaign status and bidding strategy" -o foo.csv
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql -n "recent changes for AD but don't include AD ID"
```

## Alternatives

* [gaql-cli](https://github.com/getyourguide/gaql-cli)
* [Google Ads API Report Fetcher (gaarf)](https://github.com/google/ads-api-report-fetcher)
