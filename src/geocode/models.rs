use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize)]
pub struct GeocodeQuery {
    pub q: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GeocodeResponse {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
    pub state: Option<String>,
    pub location_key: String,
}

/// Round a coordinate to 2 decimal places for canonical keying
pub fn round_coord(val: f64) -> f64 {
    (val * 100.0).round() / 100.0
}

/// Generate a location_key from coordinates
pub fn make_location_key(lat: f64, lon: f64) -> String {
    format!("{:.2},{:.2}", round_coord(lat), round_coord(lon))
}
