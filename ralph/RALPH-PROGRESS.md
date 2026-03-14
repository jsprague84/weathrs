# Ralph Progress: Stats Dashboard + Backfill Improvements

| Task | Status | Notes |
|------|--------|-------|
| TASK 1: Add api_budget to AppState | **done** | Added to AppState struct + construction |
| TASK 2: History stats query | **done** | HistoryStats + CityHistoryStats structs, get_stats() impl |
| TASK 3: /api/v1/stats endpoint | **done** | src/stats.rs + route wired |
| TASK 4: Track ALL OWM calls | **done** | Weather, Forecast, AirQuality all tracked |
| TASK 5: Mobile types + API method | **done** | StatsResponse type, getStats(), useStats() hook |
| TASK 6: Stats Dashboard UI | **done** | Progress bar, city history, devices, scheduler |
| TASK 7: Backfill progress tracking | **done** | BackfillConfigStats + per-city % completion |
| TASK 8: Response caching (forecast 15m, weather 5m) | **done** | TtlCache on Forecast + Weather services |
| TASK 9: Run backfill 3x/day | **done** | Cron: 2AM, 2PM, 10PM UTC |
| TASK 10: Canonical metric units for backfill | **done** | Store metric, convert on read |
| TASK 11: Reduce default budget to 800 | **done** | 800 backfill + 200 reserved for user |
| TASK 12: Update mobile stats with new improvements | **done** | Cache TTL info added |
| FINAL VERIFICATION | pending | |
