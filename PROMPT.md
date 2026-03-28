# Weathrs Enhancement PRD

## Instructions

You are working on the Weathrs app across two repositories:
- **Backend:** `/home/jsprague/dev/weathrs` (Rust/Axum, tower-http, sqlx/SQLite, reqwest, serde)
- **Mobile:** `/home/jsprague/dev/weathrs-mobile` (React Native/Expo SDK 54, TanStack React Query, Zustand)

**MANDATORY: Use Context7 MCP to fetch up-to-date documentation for every crate or library you work with before writing implementation code. Do not rely on memorized patterns — always verify against current docs.**

Work through the tasks below in the specified order. After completing each task:
1. Backend: run `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`
2. Commit the change with a clear, conventional commit message
3. Move to the next task

When ALL tasks are complete, output: `<promise>ALL TASKS COMPLETE</promise>`

---

## Task 1: Response Compression (Backend)

**Problem:** The backend serves uncompressed JSON responses. History and trends endpoints return large payloads (90+ days of hourly weather data). The mobile client already sends `Accept-Encoding: gzip` headers.

**Context:** `tower-http` is already a dependency with features `["cors", "trace", "timeout"]`. Adding `compression-gzip` enables `CompressionLayer`.

**Requirements:**

1. Add `compression-gzip` to the `tower-http` features in `Cargo.toml`:
   ```toml
   tower-http = { version = "0.6", features = ["cors", "trace", "timeout", "compression-gzip"] }
   ```

2. Add `CompressionLayer` to the middleware stack in `main.rs`:
   - Apply after CORS but before routes
   - Use `tower_http::compression::CompressionLayer` with default gzip settings
   - The layer automatically respects `Accept-Encoding` headers and skips compression for clients that don't request it

3. Verify compression works:
   - All existing tests must pass
   - `cargo clippy` must pass

**Acceptance Criteria:**
- [ ] `compression-gzip` feature enabled in Cargo.toml
- [ ] `CompressionLayer` added to middleware stack
- [ ] Responses are gzip-compressed when client sends `Accept-Encoding: gzip`
- [ ] All tests pass, clippy clean

---

## Task 2: Prometheus Metrics (Backend)

**Problem:** No operational metrics for monitoring request counts, latencies, error rates, or OWM API usage.

**Requirements:**

1. Add dependencies to `Cargo.toml`:
   - `metrics = "0.24"` — standard Rust metrics facade
   - `metrics-exporter-prometheus = "0.16"` — Prometheus scrape endpoint
   - Check Context7 for latest compatible versions

2. Create `src/metrics.rs`:
   - Initialize `PrometheusBuilder` and install as global recorder
   - Return a `PrometheusHandle` for the scrape endpoint
   - Define metric name constants:
     ```rust
     pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
     pub const HTTP_REQUEST_DURATION: &str = "http_request_duration_seconds";
     pub const OWM_API_CALLS: &str = "weathrs_owm_api_calls_total";
     pub const CACHE_HITS: &str = "weathrs_cache_hits_total";
     pub const CACHE_MISSES: &str = "weathrs_cache_misses_total";
     pub const BACKFILL_DAYS_FETCHED: &str = "weathrs_backfill_days_fetched_total";
     ```

3. Create a metrics middleware:
   - Write an Axum middleware function (or use `axum::middleware::from_fn`)
   - For each request, record:
     - `metrics::counter!(HTTP_REQUESTS_TOTAL, "method" => method, "path" => path, "status" => status)`
     - `metrics::histogram!(HTTP_REQUEST_DURATION, duration, "method" => method, "path" => path)`
   - Normalize path labels to avoid cardinality explosion (e.g., `/api/v1/weather/Chicago` → `/api/v1/weather/:city`)

4. Add `/metrics` endpoint on the root router (NOT under `/api/v1`):
   - Returns `PrometheusHandle::render()` as `text/plain`
   - No rate limiting on this endpoint
   - No authentication required

5. Instrument key service methods:
   - `HistoryService::fetch_timemachine` — increment `OWM_API_CALLS` with label `endpoint = "timemachine"`
   - `ForecastService` geocoding and one-call methods — increment `OWM_API_CALLS` with appropriate endpoint labels
   - `GeoCacheWithDb::get` — increment `CACHE_HITS` or `CACHE_MISSES`
   - `backfill_data` — increment `BACKFILL_DAYS_FETCHED` by count of days fetched

6. Add `mod metrics;` to `main.rs`, initialize metrics in startup, store handle in state or pass to router.

**Acceptance Criteria:**
- [ ] `/metrics` endpoint returns valid Prometheus text format
- [ ] HTTP request count and duration tracked per route
- [ ] OWM API call counter tracked with endpoint labels
- [ ] Cache hit/miss rates tracked
- [ ] All tests pass, clippy clean
- [ ] `/metrics` not rate-limited

---

## Task 3: Prominent "Feels Like" Display (Mobile)

**Problem:** `feels_like` is shown as one of four equal detail metrics in WeatherCard. When wind chill or heat index is significant, it should be more prominent.

**Context:** `WeatherCard.tsx` currently shows feels_like in a detail row alongside humidity, wind, and pressure. The `FullCurrentWeather` type includes both `temperature` and `feels_like` as numbers.

**Requirements:**

1. Update `src/components/WeatherCard.tsx`:
   - Calculate `diff = Math.abs(feelsLike - temperature)`
   - **When diff >= 5** (in user's current unit system):
     - Render a callout directly below the main temperature display
     - Text: `"Feels like {feelsLike}° · {descriptor}"` where descriptor is:
       - `"Wind chill"` when `feelsLike < temperature`
       - `"Heat index"` when `feelsLike > temperature`
     - Styling: rounded pill/badge with:
       - Blue-tinted background (`rgba(33, 150, 243, 0.15)`) + blue text for wind chill
       - Orange-tinted background (`rgba(255, 152, 0, 0.15)`) + orange text for heat index
     - Remove feels_like from the primary detail row to avoid duplication
   - **When diff < 5**: keep current behavior unchanged

**Acceptance Criteria:**
- [ ] Feels-like callout appears below main temperature when diff >= 5
- [ ] Blue styling for wind chill, orange for heat index
- [ ] Descriptor text ("Wind chill" / "Heat index") shown
- [ ] Feels_like removed from detail row when callout is visible (no duplication)
- [ ] Normal detail row display when diff < 5

---

## Task 4: Share Weather (Mobile)

**Problem:** Users cannot share current weather conditions with others.

**Context:** React Native includes a built-in `Share` API — no additional dependencies needed. The home screen has current weather data available via the forecast query.

**Requirements:**

1. Add a share button to `src/components/WeatherCard.tsx`:
   - Use `Ionicons` share icon (`share-outline`)
   - Position in the card header row, right-aligned
   - Small, unobtrusive (24px icon, themed color)

2. Implement share handler:
   - Use `import { Share } from 'react-native'`
   - Format the share message:
     ```
     Weather in {city}: {temperature}° and {description}
     Feels like {feelsLike}° | Humidity: {humidity}% | Wind: {windSpeed} {unit}

     via Weathrs
     ```
   - Use `Share.share({ message })` — no URL needed
   - Handle share cancellation gracefully (no error shown)

3. Pass the necessary data to WeatherCard:
   - WeatherCard already receives `weather`, `location`, and `units` props
   - The share handler should use data from these existing props

**Acceptance Criteria:**
- [ ] Share icon button visible on WeatherCard
- [ ] Tapping opens native share sheet
- [ ] Shared text includes city, temperature, description, feels like, humidity, wind
- [ ] Temperature unit matches user's setting
- [ ] No crash or error on share cancellation

---

## Task 5: Weather Alerts on Home Screen (Mobile)

**Problem:** The backend returns weather alerts in the forecast response, but the mobile app ignores them. Users have no visibility into active weather warnings.

**Context:** The forecast response includes an `alerts` array of `{ sender, event, start, end, description, tags }`. The array is empty when no alerts are active. `WeatherAlert` type already exists in mobile `types/weather.ts`.

**Requirements:**

1. Create `src/components/AlertBanner.tsx`:
   - Props: `alerts: WeatherAlert[]`
   - Severity color logic based on the `event` string:
     - **Red** (`#D32F2F` bg, white text): event contains "Warning", "Tornado", "Hurricane", "Tsunami", "Emergency"
     - **Orange** (`#F57C00` bg, white text): event contains "Watch", "Advisory"
     - **Yellow** (`#FFF176` bg, dark text): all other events
   - Display the first alert prominently:
     - Event name (bold), sender name (smaller)
     - Time range: "Until {end time}" or "{start} — {end}" formatted nicely
   - Tap to expand: show the full `description` text in a collapsible section
   - If `alerts.length > 1`, show a count badge: "{n} active alerts" and allow scrolling through them with a horizontal `FlatList` or pager dots
   - Dismiss button (X icon, top-right): hides the banner for the current session
     - Use `useState` — dismissal resets on app restart or new data fetch
   - Style: rounded card with 16px horizontal margin, matching app card styling

2. Export from `src/components/index.ts`

3. Integrate into `app/index.tsx`:
   - Import `AlertBanner`
   - Position between `CitySelector` and `WeatherCard`
   - Only render when `forecast?.alerts?.length > 0`
   - Pass `forecast.alerts` as prop

4. Verify the `WeatherAlert` type in `src/types/weather.ts` matches the backend response shape (`sender`, `event`, `start`, `end`, `description`, `tags`). Fix if needed.

**Acceptance Criteria:**
- [ ] AlertBanner component created and exported
- [ ] Alerts render on home screen between city selector and weather card
- [ ] Color-coded by severity (red/orange/yellow)
- [ ] Tap expands to show full description
- [ ] Multiple alerts supported with count indicator
- [ ] Dismiss hides for current session
- [ ] No banner when alerts array is empty
- [ ] Types match backend response

---

## Task 6: Offline Mode with Cached Data (Mobile)

**Problem:** When offline, the app shows error screens even though React Query has cached data available via AsyncStorage persistence.

**Context:** React Query is already configured with an AsyncStorage persister (24-hour maxAge). Queries have `staleTime` and `gcTime` configured per endpoint. The `isError` and `data` states can coexist — a query can have stale cached data AND be in error state simultaneously.

**Requirements:**

1. Create `src/components/ui/StaleDataBanner.tsx`:
   - Props: `dataUpdatedAt: number` (React Query's `dataUpdatedAt` timestamp in ms)
   - Shows relative time: "Updated 5 min ago", "Updated 2 hours ago", "Updated yesterday"
   - Styling: subtle banner with warning color tint
     - Muted/amber background when data is > 10 minutes stale
     - Informational/gray when < 10 minutes stale
   - Icon: `cloud-offline-outline` from Ionicons
   - Export from `src/components/ui/index.ts`

2. Update `app/index.tsx` (Home Screen):
   - Change error handling logic:
     - **Before:** if `error` → show `ErrorDisplay`
     - **After:** if `error && !forecast` → show `ErrorDisplay` (no cached data at all)
     - If `error && forecast` → show cached weather data + `StaleDataBanner`
   - Add `StaleDataBanner` above the WeatherCard when showing stale data
   - Use `dataUpdatedAt` from the React Query result to calculate staleness

3. Update `app/forecast.tsx` (Forecast Screen):
   - Same pattern: show cached forecast with `StaleDataBanner` when error + cached data
   - Only show `ErrorDisplay` when truly no data available

4. Update `app/history.tsx` (History Screen):
   - Same pattern for history/trends data

5. Ensure pull-to-refresh still works and attempts to reconnect when showing stale data

**Acceptance Criteria:**
- [ ] Home screen shows cached weather data when offline (instead of error screen)
- [ ] Forecast screen shows cached data when offline
- [ ] History screen shows cached data when offline
- [ ] StaleDataBanner shows with relative time ("Updated X ago")
- [ ] Pull-to-refresh still functional when showing cached data
- [ ] ErrorDisplay only shown when no cached data exists at all
- [ ] Banner disappears when fresh data is successfully fetched

---

## Implementation Order

1. **Task 1: Response Compression** — Backend, quick win
2. **Task 2: Prometheus Metrics** — Backend, new module
3. **Task 3: Prominent Feels Like** — Mobile, small UI change
4. **Task 4: Share Weather** — Mobile, self-contained
5. **Task 5: Weather Alerts** — Mobile, new component
6. **Task 6: Offline Mode** — Mobile, cross-screen, do last

## General Guidelines

- Follow existing code patterns — read existing files before creating new ones
- Use `thiserror` for error types on backend
- Use themed colors from `useTheme()` on mobile — do not hardcode light/dark mode colors
- Keep changes focused — do not refactor unrelated code
- Use `tracing` for backend logging (info for operations, debug for details)
- Test on dark mode as well as light mode for mobile UI changes
