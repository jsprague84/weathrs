use axum::extract::{Query, State};
use axum::Json;

use crate::geocode::models::{make_location_key, round_coord, GeocodeQuery, GeocodeResponse};
use crate::AppState;

pub async fn geocode(
    State(state): State<AppState>,
    Query(query): Query<GeocodeQuery>,
) -> Result<Json<GeocodeResponse>, (axum::http::StatusCode, String)> {
    let geo = state
        .forecast_service
        .geocode(&query.q)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                format!("Geocoding failed: {}", e),
            )
        })?;

    let lat = round_coord(geo.lat);
    let lon = round_coord(geo.lon);

    Ok(Json(GeocodeResponse {
        name: geo.name,
        lat,
        lon,
        country: geo.country,
        state: geo.state,
        location_key: make_location_key(lat, lon),
    }))
}
