# Weathrs API - Reliability, Cleanliness & Feature Improvements

## Instructions

You are working on the Weathrs Rust API at `/home/jsprague/dev/weathrs`. This is an Axum-based weather API using OpenWeatherMap, SQLite (sqlx), tokio, reqwest, and serde.

**MANDATORY: Use Context7 MCP to fetch up-to-date documentation for every crate you work with (axum, tower-http, sqlx, reqwest, serde, tokio, etc.) before writing implementation code. Do not rely on memorized patterns — always verify against current docs.**

Work through the tasks below sequentially. After completing each task:
1. Ensure the code compiles (`cargo check`)
2. Commit the change with a clear, conventional commit message
3. Move to the next task

When ALL tasks are complete, output: `<promise>ALL TASKS COMPLETE</promise>`

---

## Task 1: Remove Dual JSON+SQLite Storage

**Problem:** Devices and scheduler jobs are persisted to both `data/*.json` files AND SQLite, creating sync risk and data inconsistency bugs.

**Requirements:**
- Make SQLite the single source of truth for devices and scheduler jobs
- Remove all JSON file read/write logic from the device and scheduler modules
- Remove any JSON file initialization/migration code from startup
- Ensure all CRUD operations go through the SQLite repositories (`DeviceRepository`, `JobRepository`)
- If there is any one-time migration path from JSON to SQLite, preserve it as a clearly marked migration function but do not call it in normal startup flow
- Verify no remaining references to `data/devices.json` or `data/scheduler_jobs.json` in runtime code

**Context7:** Fetch sqlx and serde_json docs for current best practices on repository patterns.

---

## Task 2: Add Rate Limiting

**Problem:** No request rate limiting exists. A misbehaving client can exhaust OWM quota or overload the server.

**Requirements:**
- Add IP-based rate limiting using `tower-governor` or an equivalent tower-compatible crate
- Default limits: 60 requests/minute per IP for general endpoints, 10 requests/minute for scheduler mutation endpoints (POST/PUT/DELETE)
- Return standard `429 Too Many Requests` with `Retry-After` header
- Make rate limit values configurable via `config.toml` / environment variables
- Add rate limit config fields to the `Settings` struct in `config.rs`

**Context7:** Fetch tower-governor (or tower rate limiting middleware) docs for Axum integration.

---

## Task 3: Persist Geocoding Cache to SQLite

**Problem:** Geocoding cache is in-memory only (DashMap). Server restarts lose the cache, causing redundant OWM geocoding API calls on cold start.

**Requirements:**
- Create a `geocoding_cache` SQLite table: `(city_query TEXT PRIMARY KEY, name TEXT, lat REAL, lon REAL, country TEXT, state TEXT, cached_at INTEGER)`
- Add a new migration file for this table
- On cache miss in the in-memory DashMap, check SQLite before calling the OWM API
- On successful OWM geocoding response, write to both DashMap and SQLite
- Respect a configurable TTL for SQLite entries (default 7 days) — entries older than TTL are treated as misses and re-fetched
- Add a cleanup task that runs with the existing hourly cache cleanup to prune expired SQLite entries

**Context7:** Fetch sqlx docs for SQLite INSERT OR REPLACE and datetime handling.

---

## Task 4: Add Validation on History Date Ranges

**Problem:** The raw history endpoint (`/history/{city}`) has no practical limit on date range. Requesting years of hourly data is expensive and slow.

**Requirements:**
- Add a maximum date range validation to the history endpoint: 90 days max for hourly data, 365 days max for daily summaries
- Return `400 Bad Request` with a clear error message and `INVALID_DATE_RANGE` error code when exceeded
- Add validation that `start` < `end` and both are in the past
- Add these limits as constants or config values, not magic numbers
- Update the OpenAPI/utoipa annotations to document these constraints

**Context7:** Fetch axum docs for custom extractors and error responses.

---

## Task 5: Make Timeouts Configurable

**Problem:** HTTP connect timeout (5s) and request timeout (60s) are hardcoded in `main.rs` and the reqwest client setup.

**Requirements:**
- Add `request_timeout_secs` (default 60) and `connect_timeout_secs` (default 5) to `Settings` in `config.rs`
- Use these values when building the reqwest HTTP client and the tower `TimeoutLayer`
- Document in `config.example.toml`

**Context7:** Fetch reqwest and tower-http docs for timeout configuration.

---

## Task 6: Add CORS Middleware

**Problem:** No CORS headers. Web clients and Expo Web builds will fail.

**Requirements:**
- Add `tower-http`'s CORS layer to the router
- Default: allow all origins in development, configurable allowed origins list for production
- Add `cors_allowed_origins` config field (list of strings, empty = allow all)
- Allow headers: `Content-Type`, `X-API-Key`, `Authorization`
- Allow methods: `GET`, `POST`, `PUT`, `DELETE`, `OPTIONS`
- Place CORS layer so it applies to all routes

**Context7:** Fetch tower-http CORS middleware docs for Axum.

---

## Task 7: Deep Health Check

**Problem:** `/health` returns 200 without verifying SQLite connectivity or OWM API reachability.

**Requirements:**
- Keep the existing lightweight `/health` endpoint for load balancer probes (just returns 200)
- Add a new `GET /health/deep` endpoint that checks:
  - SQLite: run a simple `SELECT 1` query
  - OWM API: verify the API key is valid by making a lightweight geocoding request (cache the result)
  - Report individual component status and overall status
- Response shape:
  ```json
  {
    "status": "healthy" | "degraded" | "unhealthy",
    "components": {
      "database": { "status": "healthy", "latency_ms": 2 },
      "openweathermap": { "status": "healthy", "latency_ms": 150 }
    },
    "version": "0.1.0"
  }
  ```
- Return 200 for healthy, 503 for degraded/unhealthy
- Add to OpenAPI docs

**Context7:** Fetch axum docs for JSON response serialization and shared state access.

---

## Task 8: Per-Endpoint Timeouts on Mobile API Client

**Problem:** Mobile app uses a flat 15s timeout for all requests. Heavy endpoints (history, trends) may need more time.

**Scope:** This task modifies the **mobile app** at `/home/jsprague/dev/weathrs-mobile`.

**Requirements:**
- Refactor `src/services/api.ts` to accept an optional timeout parameter per request
- Set default timeout to 15s, but allow overrides:
  - History/trends endpoints: 30s
  - Forecast endpoints: 20s
  - Weather/health endpoints: 10s
- Apply these timeouts in the respective hook calls in `src/hooks/useWeather.ts`

**Context7:** Fetch React Native / Expo fetch API docs for AbortController patterns.

---

## Task 9: Add Fetch Retry Logic on Mobile

**Problem:** No retry logic in the mobile API client. Mobile connections are flaky.

**Scope:** This task modifies the **mobile app** at `/home/jsprague/dev/weathrs-mobile`.

**Requirements:**
- Add a retry wrapper in `src/services/api.ts` for transient failures (network errors, 502, 503, 504)
- Max 2 retries with exponential backoff (1s, then 2s)
- Do NOT retry 4xx errors (client errors) except 408 and 429
- For 429, respect `Retry-After` header if present
- Keep the implementation simple — a helper function, not a library

---

## Task 10: Push Token Refresh Handling on Mobile

**Problem:** Push tokens can rotate. The app registers once but doesn't handle token refresh.

**Scope:** This task modifies the **mobile app** at `/home/jsprague/dev/weathrs-mobile`.

**Requirements:**
- Add a token change listener in `src/hooks/useNotifications.ts` using Expo's `addPushTokenListener`
- When token changes, automatically re-register with the API using the new token
- Update the notification store with the new token
- Log token changes for debugging

**Context7:** Fetch expo-notifications docs for push token subscription/listener API.

---

## Task 11: Widget Data Endpoint

**Problem:** No lightweight endpoint optimized for home screen widgets.

**Requirements:**
- Add `GET /api/v1/widget/{city}` endpoint
- Response: minimal payload for widget rendering:
  ```json
  {
    "city": "Chicago",
    "temperature": 22.5,
    "high": 25.0,
    "low": 18.0,
    "icon": "02d",
    "description": "partly cloudy",
    "units": "metric",
    "updated_at": 1700000000
  }
  ```
- Source data from the forecast endpoint (reuse `ForecastService`)
- Aggressive caching: add `Cache-Control: public, max-age=300` header
- Add to OpenAPI docs with utoipa
- Add route to `routes.rs`

**Context7:** Fetch axum docs for response headers and handler patterns.

---

## Task 12: Air Quality Endpoint

**Problem:** OWM offers an Air Pollution API that pairs naturally with weather data.

**Requirements:**
- Create a new `air_quality` module (follow the pattern of existing modules like `weather/` or `forecast/`)
- Add `GET /api/v1/air-quality/{city}` endpoint
- Use the OWM Air Pollution API (`/data/2.5/air_pollution`)
- Response shape:
  ```json
  {
    "city": "Chicago",
    "aqi": 2,
    "aqi_label": "Fair",
    "components": {
      "co": 230.31,
      "no": 0.0,
      "no2": 5.13,
      "o3": 68.53,
      "so2": 0.64,
      "pm2_5": 8.42,
      "pm10": 12.37,
      "nh3": 0.12
    },
    "updated_at": 1700000000
  }
  ```
- AQI labels: 1=Good, 2=Fair, 3=Moderate, 4=Poor, 5=Very Poor
- Reuse the geocoding/cache infrastructure for lat/lon resolution
- Add proper error types following existing `HttpError` patterns
- Add to OpenAPI docs and routes
- Use the same reqwest client from AppState

**Context7:** Fetch the OpenWeatherMap Air Pollution API docs. Fetch axum and utoipa docs for module/route patterns.

---

## General Guidelines

- Follow existing code patterns and conventions in the codebase
- Use `thiserror` for error types, `anyhow` only where appropriate
- All new endpoints must have utoipa/OpenAPI annotations
- Run `cargo check` after each task to verify compilation
- Keep changes focused — do not refactor unrelated code
- Use `tracing` for logging (info for operations, debug for details, warn/error for failures)
