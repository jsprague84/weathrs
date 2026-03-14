# Ralph PRD: API Stats & History Tracking Dashboard

Add a comprehensive stats/tracking section to the mobile app settings screen, backed by new backend endpoints. Shows API call budget usage, database history coverage per city, backfill progress, and device/scheduler stats. This is an admin/developer feature — not for the future public app version.

## Rules

- Use Context7 MCP to fetch latest documentation for any crate or library before implementing (axum, serde, sqlx, react-native, tanstack-query, expo).
- Run `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings && cargo test` after every Rust change. Fix errors before moving on.
- Run `npx tsc --noEmit` in weathrs-mobile after every TypeScript change. Fix errors before moving on.
- Do NOT create new files unless absolutely necessary. Prefer editing existing files.
- Do NOT add comments, docstrings, or type annotations to code you didn't change.
- Keep changes minimal and focused. No refactoring unless required.
- After all tasks are done, run full verification (see FINAL VERIFICATION section).

## Context

**Backend** (`/home/jsprague/dev/weathrs`):
- `src/api_budget.rs` — `ApiCallBudget` with `used_today()`, `remaining()`, `daily_limit`
- `src/db/history_repo.rs` — `SqliteHistoryRepository` with `get_range()`, `get_missing_days()`, etc.
- `src/db/device_repo.rs` — `SqliteDeviceRepository` with `count()`, `get_all()`
- `src/db/job_repo.rs` — `SqliteJobRepository` with `count()`, `get_all()`
- `src/metrics.rs` — Prometheus counters: `OWM_API_CALLS`, `CACHE_HITS`, `CACHE_MISSES`, `BACKFILL_DAYS_FETCHED`
- `src/cache.rs` — `GeoCacheWithDb` with `memory_len()`
- `src/routes.rs` — Router with `api_v1_routes()`, existing `/scheduler/status` and `/devices/count` endpoints
- `src/main.rs` — `AppState` struct with all services + `api_budget` as `Arc<ApiCallBudget>`

**Mobile** (`/home/jsprague/dev/weathrs-mobile`):
- `app/settings.tsx` — Settings screen with Card components
- `src/services/api.ts` — `WeathrsApi` class with `request()` method
- `src/components/ui/` — Reusable UI components (Card, Button, Loading, etc.)
- `src/hooks/useWeather.ts` — React Query hooks pattern

**OWM API budget**: 1,000 free calls/day on One Call 3.0. The `ApiCallBudget` currently only tracks history/backfill calls, NOT forecast or weather calls. The budget should represent the global daily OWM usage.

---

## Tasks

### TASK 1: Add `api_budget` to AppState

**What:** Make the shared `ApiCallBudget` accessible from API handlers by adding it to `AppState`.

**How:**
- In `src/main.rs`, add `pub api_budget: Arc<api_budget::ApiCallBudget>` to the `AppState` struct
- Pass `Arc::clone(&api_budget)` when constructing `AppState`
- Add `use crate::api_budget::ApiCallBudget;` import if needed

**Verify:** `cargo check` passes.

---

### TASK 2: Add history stats query to HistoryRepository

**What:** Add a method to get per-city coverage stats from the `weather_history` table.

**How:**
- Use Context7 to check latest sqlx patterns for aggregate queries.
- In `src/db/history_repo.rs`, add to the `HistoryRepository` trait:
  ```rust
  async fn get_stats(&self) -> Result<HistoryStats, DbError>;
  ```
- Add `HistoryStats` struct:
  ```rust
  pub struct HistoryStats {
      pub total_records: i64,
      pub cities: Vec<CityHistoryStats>,
  }
  pub struct CityHistoryStats {
      pub city: String,
      pub record_count: i64,
      pub earliest_timestamp: i64,
      pub latest_timestamp: i64,
      pub missing_days: i64,  // gaps in coverage
  }
  ```
- Implement with SQL:
  ```sql
  SELECT city, COUNT(*) as record_count,
         MIN(timestamp) as earliest_timestamp,
         MAX(timestamp) as latest_timestamp
  FROM weather_history
  GROUP BY city
  ORDER BY city
  ```
- For `missing_days`: calculate `(latest - earliest) / 86400 - distinct_days_count` per city
- Add `total_records` query: `SELECT COUNT(*) FROM weather_history`

**Verify:** `cargo test` passes (existing tests still work).

---

### TASK 3: Create `/api/v1/stats` endpoint (backend)

**What:** New JSON endpoint that aggregates all stats into a single response.

**How:**
- Use Context7 to check latest axum handler patterns for JSON responses.
- Create `src/stats.rs` module with:
  - `StatsResponse` struct (serde Serialize, camelCase):
    ```rust
    pub struct StatsResponse {
        pub api_budget: ApiBudgetStats,
        pub history: HistoryStatsResponse,
        pub devices: DeviceStats,
        pub scheduler: SchedulerStats,
        pub cache: CacheStats,
    }
    ```
  - Sub-structs for each section:
    - `ApiBudgetStats { daily_limit, used_today, remaining_today }`
    - `HistoryStatsResponse { total_records, cities: Vec<CityHistoryStats> }`
    - `DeviceStats { total, enabled, by_platform: HashMap<String, usize> }`
    - `SchedulerStats { total_jobs, enabled_jobs }`
    - `CacheStats { geocoding_memory_entries }`
  - Handler function: `pub async fn get_stats(State(state): State<AppState>) -> Json<StatsResponse>`
    - Calls `state.api_budget.used_today()`, `.remaining()`, `.daily_limit`
    - Calls `state.history_service` repo stats
    - Calls `state.devices_service.get_all()` and aggregates
    - Calls `state.scheduler_service.get_jobs()` and counts
- Add `mod stats;` to `src/main.rs`
- Add route in `src/routes.rs`: `.route("/stats", get(stats::get_stats))` in `api_v1_routes`

**Verify:** `cargo clippy` and `cargo test` pass. `curl https://weathrs.js-node.cc/api/v1/stats` returns JSON.

---

### TASK 4: Track ALL OWM API calls against the shared budget

**What:** Currently only `HistoryService::fetch_timemachine` calls `api_budget.record_call()`. Forecast, weather, and air quality calls are untracked. Track them all.

**How:**
- Add `api_budget: Arc<ApiCallBudget>` field to `ForecastService`, `WeatherService`, and `AirQualityService`
- Update constructors in `src/main.rs` to pass `Arc::clone(&api_budget)`
- In `ForecastService`:
  - `get_forecast()`, `get_daily_forecast()`, `get_hourly_forecast()` — call `self.api_budget.record_call()` before each OWM HTTP request
  - `geocode_city()`, `geocode_zip()` — do NOT track (geocoding API is free/separate)
- In `WeatherService`:
  - `get_weather()` / `get_weather_by_city()` — call `self.api_budget.record_call()` before OWM request
- In `AirQualityService`:
  - `get_air_quality()` — call `self.api_budget.record_call()` before OWM request
- Leave `daily_budget` config default at 1000 (matches OWM free tier)

**Verify:** `cargo clippy` passes. Hit the weather endpoint a few times, then check `/api/v1/stats` — `used_today` should increment.

---

### TASK 5: Add stats types and API method to mobile app

**What:** Add TypeScript types and API method for the new stats endpoint.

**How:**
- Use Context7 to check latest TanStack Query patterns for polling queries.
- In `src/types/weather.ts`, add:
  ```typescript
  export interface StatsResponse {
    apiBudget: { dailyLimit: number; usedToday: number; remainingToday: number };
    history: {
      totalRecords: number;
      cities: Array<{
        city: string;
        recordCount: number;
        earliestTimestamp: number;
        latestTimestamp: number;
        missingDays: number;
      }>;
    };
    devices: { total: number; enabled: number; byPlatform: Record<string, number> };
    scheduler: { totalJobs: number; enabledJobs: number };
    cache: { geocodingMemoryEntries: number };
  }
  ```
- In `src/services/api.ts`, add method:
  ```typescript
  async getStats(): Promise<StatsResponse> {
    return this.request('/stats', { timeout: TIMEOUT.FAST });
  }
  ```
- In `src/hooks/useWeather.ts`, add hook:
  ```typescript
  export function useStats() {
    return useQuery({
      queryKey: ['stats'],
      queryFn: () => api.getStats(),
      staleTime: 30 * 1000, // 30 seconds
      refetchInterval: 60 * 1000, // Auto-refresh every minute
    });
  }
  ```

**Verify:** `npx tsc --noEmit` passes.

---

### TASK 6: Build Stats Dashboard section in Settings screen

**What:** Add a "System Stats" section to the settings screen showing all tracked data in a clear, scannable layout.

**How:**
- Use Context7 to check latest React Native component patterns.
- In `app/settings.tsx`, add a new `<Card>` section titled "System Stats" after the existing settings sections.
- Import and use the `useStats()` hook.
- Layout the stats in grouped rows:

  **API Budget** (progress bar + numbers):
  - "API Calls Today: 234 / 1,000" with a colored progress bar (green < 50%, yellow < 80%, red >= 80%)
  - "Remaining: 766"

  **History Coverage** (per-city list):
  - For each city: city name, date range (e.g., "Jan 2024 — Mar 2026"), record count, missing days count
  - Use a colored dot: green if missing_days == 0, yellow if < 30, red if >= 30

  **Devices**:
  - "Registered: 2 (2 enabled)"
  - Platform breakdown: "Android: 2"

  **Scheduler**:
  - "Jobs: 1 (1 enabled)"

  **Cache**:
  - "Geocoding cache: 23 entries"

- Show a `<Loading />` state while stats are fetching
- Show last-updated timestamp at bottom of the section
- Use existing theme colors from `useTheme()` for all styling

**Verify:** `npx tsc --noEmit` passes. The section renders correctly in the app.

---

### TASK 7: Add backfill progress tracking

**What:** Enhance the stats response to include backfill-specific information: how many days are fully backfilled vs missing per city, and the configured max_years.

**How:**
- In the backend `StatsResponse`, add to `HistoryStatsResponse`:
  ```rust
  pub backfill_config: BackfillConfigStats,
  ```
  With:
  ```rust
  pub struct BackfillConfigStats {
      pub enabled: bool,
      pub max_years: u32,
      pub daily_budget: u32,
      pub cron: String,
  }
  ```
- Populate from `state.config.history_backfill`
- In the mobile stats section, add a "Backfill" subsection:
  - "Status: Enabled" or "Disabled"
  - "Target: 5 years of history"
  - "Schedule: Daily at 2:00 AM UTC"
  - Per-city: "Blue Grass: 95% complete (1,734 / 1,825 days)"
    - Calculate: `total_possible_days = max_years * 365`, `covered = total_possible_days - missing_days`, `percent = covered / total_possible_days * 100`

**Verify:** `cargo clippy` and `npx tsc --noEmit` pass.

---

### TASK 8: Add response caching to ForecastService and WeatherService

**What:** User-facing forecast and weather requests currently hit OWM live every time. Add in-memory TTL caching so the same city's data serves all requests within a window, saving API budget for backfill.

**How:**
- Use Context7 to check latest reqwest/axum caching patterns.
- In `src/forecast/service.rs`:
  - Add `forecast_cache: TtlCache<String, ForecastResponse>` field with 15-minute TTL
  - In `get_forecast()`, `get_daily_forecast()`, `get_hourly_forecast()`:
    - Build cache key: `format!("{}_{}_{}", normalized_city, units, endpoint_type)`
    - Check cache before OWM call; return cached if hit
    - Store response in cache after successful OWM fetch
  - Import `TtlCache` from `crate::cache`
- In `src/weather/service.rs`:
  - Add `weather_cache: TtlCache<String, serde_json::Value>` field with 5-minute TTL
  - Cache key: `format!("{}_{}", normalized_city, units)`
  - Same check-before-fetch, store-after-fetch pattern
- Update constructors in `src/main.rs` to initialize caches
- The `ForecastResponse` and weather response types need `Clone` derive (add if missing)

**Verify:** `cargo clippy` passes. Hit the same city forecast endpoint twice rapidly — second call should be near-instant (check server logs for absence of OWM API call).

---

### TASK 9: Run backfill multiple times per day

**What:** The backfill cron defaults to once at 2AM UTC. If user-facing calls don't exhaust the budget, remaining calls go unused. Run backfill 3x per day to use whatever budget remains.

**How:**
- In `src/config.rs`, change `default_backfill_cron()` from `"0 0 2 * * *"` to `"0 0 2,14,22 * * *"` (2AM, 2PM, 10PM UTC)
- The existing `ApiCallBudget::remaining()` check in `run_backfill()` already gates calls — if budget is exhausted from earlier runs or user requests, the later runs will simply skip with "budget exhausted"
- No other code changes needed — the budget auto-resets at UTC day boundary

**Verify:** `cargo check` passes.

---

### TASK 10: Canonical units for backfill data

**What:** The backfill runner hardcodes `let units = "metric"` but the `UNIQUE(city, timestamp, units)` constraint means imperial queries won't find metric-cached data. Store in metric and convert on read.

**How:**
- In `src/history/service.rs`:
  - In `get_history()` and `get_daily_history()` and `get_trends()`: always query the DB with `units = "metric"` regardless of the user's requested units
  - After fetching records, convert temperature values if the user requested imperial or standard:
    - metric→imperial: `temp_f = temp_c * 9.0 / 5.0 + 32.0` (temperature, feels_like)
    - metric→imperial: `speed_mph = speed_ms * 2.237` (wind_speed)
    - metric→standard: `temp_k = temp_c + 273.15`
  - Add a helper function `fn convert_units(value: f64, field: &str, from: &str, to: &str) -> f64`
- In `src/backfill/runner.rs`, the hardcoded `let units = "metric"` is already correct — keep it
- The `backfill_data()` method in `HistoryService` also hardcodes the units parameter to OWM — ensure it always passes "metric"

**Verify:** `cargo test` passes. Query history with `units=imperial` — should return converted values from metric-stored data.

---

### TASK 11: Reduce default daily budget to reserve capacity for user calls

**What:** The default `daily_budget` is 1,000 — the entire OWM free tier. With all calls now tracked (TASK 4), reduce to 800 to ensure 200 calls are always available for user-facing requests.

**How:**
- In `src/config.rs`, change `default_daily_budget()` return value from `1000` to `800`
- Update `config.example.toml` comment to explain the reservation:
  ```toml
  # Maximum OWM API calls per day for backfill (default: 800)
  # Reserves ~200 calls/day for user-facing forecast/weather requests
  # daily_budget = 800
  ```

**Verify:** `cargo check` passes.

---

### TASK 12: Update mobile stats UI with new backfill improvements

**What:** Update the Stats Dashboard (TASK 6) to reflect the new caching and budget improvements.

**How:**
- In the API Budget section, add:
  - "Reserved for user requests: 200" (dailyLimit - backfillBudget, or hardcoded if not in response)
  - "Backfill budget: 800"
- In the Backfill section, add:
  - "Schedule: 3x daily (2AM, 2PM, 10PM UTC)" — read from backfill config cron
  - Show next scheduled run time (parse cron and compute)
- Add a note about forecast caching: "Forecast cache: 15 min TTL | Weather cache: 5 min TTL"

**Verify:** `npx tsc --noEmit` passes.

---

## FINAL VERIFICATION

After all tasks are complete, run these checks:

### Backend
```bash
cd /home/jsprague/dev/weathrs
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
# Verify endpoint
curl -s https://weathrs.js-node.cc/api/v1/stats | jq .
```

### Mobile
```bash
cd /home/jsprague/dev/weathrs-mobile
npx tsc --noEmit
```

### Integration
1. Open the app → Settings → scroll to "System Stats" section
2. Verify API budget shows current usage with backfill vs user breakdown
3. Verify history coverage lists each registered city with date ranges
4. Verify backfill progress shows completion percentage
5. Verify stats auto-refresh (numbers should update if you trigger a forecast)
6. Hit the same city forecast twice rapidly — verify second is cached (no OWM call in logs)
7. Check that `used_today` increments for forecast/weather calls (not just backfill)

### Final Audit
After all features are verified:
1. Use Context7 MCP to check that the axum handler patterns match latest best practices
2. Use Context7 MCP to check that TanStack Query usage follows latest v5 patterns
3. Use Context7 MCP to check React Native component patterns are current
4. If any patterns are outdated, create a follow-up task list to modernize
5. Commit all changes with descriptive messages
6. Push both repos
7. Trigger a signed APK build

<promise>STATS_DASHBOARD_COMPLETE</promise>
