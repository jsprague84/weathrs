-- Backfill location_key from existing lat/lon (rounded to 2 decimal places)
-- Use printf to match Rust's format!("{:.2}") which always pads trailing zeros
UPDATE weather_history
SET location_key = printf('%.2f', ROUND(lat, 2)) || ',' || printf('%.2f', ROUND(lon, 2))
WHERE location_key IS NULL;

-- Deduplicate: keep the row with the lowest id for each (location_key, timestamp, units) group
DELETE FROM weather_history
WHERE id NOT IN (
    SELECT MIN(id)
    FROM weather_history
    GROUP BY location_key, timestamp, units
);

-- Drop old indexes
DROP INDEX IF EXISTS idx_history_city_ts;
DROP INDEX IF EXISTS idx_history_city_units;

-- Add new indexes for location_key-based queries
CREATE UNIQUE INDEX IF NOT EXISTS idx_history_location_ts_units
    ON weather_history(location_key, timestamp, units);

CREATE INDEX IF NOT EXISTS idx_history_location_ts
    ON weather_history(location_key, timestamp);

-- Keep a city-based index for display lookups
CREATE INDEX IF NOT EXISTS idx_history_city
    ON weather_history(city);
