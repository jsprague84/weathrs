use axum::{extract::Request, middleware::Next, response::Response};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::time::Instant;

// Metric name constants
pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
pub const HTTP_REQUEST_DURATION: &str = "http_request_duration_seconds";
pub const OWM_API_CALLS: &str = "weathrs_owm_api_calls_total";
pub const CACHE_HITS: &str = "weathrs_cache_hits_total";
pub const CACHE_MISSES: &str = "weathrs_cache_misses_total";
pub const BACKFILL_DAYS_FETCHED: &str = "weathrs_backfill_days_fetched_total";

/// Initialize the Prometheus metrics recorder and return a handle for the scrape endpoint.
pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder")
}

/// Normalize a request path to avoid high-cardinality labels.
/// Replaces dynamic path segments with placeholders.
fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();

    // Routes with dynamic segments:
    // /api/v1/weather/{city}
    // /api/v1/forecast/{city}
    // /api/v1/forecast/daily/{city}
    // /api/v1/forecast/hourly/{city}
    // /api/v1/widget/{city}
    // /api/v1/air-quality/{city}
    // /api/v1/history/{city}
    // /api/v1/history/{city}/daily
    // /api/v1/history/{city}/trends
    // /api/v1/scheduler/jobs/{id}
    // /api/v1/scheduler/trigger/{city}

    if parts.len() >= 4 && parts[1] == "api" && parts[2] == "v1" {
        match parts[3] {
            "weather" if parts.len() == 5 => "/api/v1/weather/:city".to_string(),
            "forecast" if parts.len() == 5 && parts[4] != "daily" && parts[4] != "hourly" => {
                "/api/v1/forecast/:city".to_string()
            }
            "forecast" if parts.len() == 6 && parts[4] == "daily" => {
                "/api/v1/forecast/daily/:city".to_string()
            }
            "forecast" if parts.len() == 6 && parts[4] == "hourly" => {
                "/api/v1/forecast/hourly/:city".to_string()
            }
            "widget" if parts.len() == 5 => "/api/v1/widget/:city".to_string(),
            "air-quality" if parts.len() == 5 => "/api/v1/air-quality/:city".to_string(),
            "history" if parts.len() == 5 => "/api/v1/history/:city".to_string(),
            "history" if parts.len() == 6 && parts[5] == "daily" => {
                "/api/v1/history/:city/daily".to_string()
            }
            "history" if parts.len() == 6 && parts[5] == "trends" => {
                "/api/v1/history/:city/trends".to_string()
            }
            "scheduler" if parts.len() == 6 && parts[4] == "jobs" => {
                "/api/v1/scheduler/jobs/:id".to_string()
            }
            "scheduler" if parts.len() == 6 && parts[4] == "trigger" => {
                "/api/v1/scheduler/trigger/:city".to_string()
            }
            _ => path.to_string(),
        }
    } else {
        path.to_string()
    }
}

/// Axum middleware that records HTTP request metrics (count and duration).
pub async fn track_metrics(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = normalize_path(request.uri().path());

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();

    let status = response.status().as_u16().to_string();

    counter!(HTTP_REQUESTS_TOTAL, "method" => method.clone(), "path" => path.clone(), "status" => status).increment(1);
    histogram!(HTTP_REQUEST_DURATION, "method" => method, "path" => path).record(duration);

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_weather_city() {
        assert_eq!(
            normalize_path("/api/v1/weather/Chicago"),
            "/api/v1/weather/:city"
        );
    }

    #[test]
    fn test_normalize_path_forecast_city() {
        assert_eq!(
            normalize_path("/api/v1/forecast/London"),
            "/api/v1/forecast/:city"
        );
    }

    #[test]
    fn test_normalize_path_forecast_daily_city() {
        assert_eq!(
            normalize_path("/api/v1/forecast/daily/Chicago"),
            "/api/v1/forecast/daily/:city"
        );
    }

    #[test]
    fn test_normalize_path_history_city_trends() {
        assert_eq!(
            normalize_path("/api/v1/history/Chicago/trends"),
            "/api/v1/history/:city/trends"
        );
    }

    #[test]
    fn test_normalize_path_static_route() {
        assert_eq!(normalize_path("/health"), "/health");
        assert_eq!(normalize_path("/api/v1/forecast"), "/api/v1/forecast");
    }

    #[test]
    fn test_normalize_path_scheduler_jobs_id() {
        assert_eq!(
            normalize_path("/api/v1/scheduler/jobs/abc-123"),
            "/api/v1/scheduler/jobs/:id"
        );
    }

    #[test]
    fn test_normalize_path_air_quality() {
        assert_eq!(
            normalize_path("/api/v1/air-quality/Paris"),
            "/api/v1/air-quality/:city"
        );
    }
}
