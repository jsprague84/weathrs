-- Create weather history table for caching historical weather data
CREATE TABLE IF NOT EXISTS weather_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    city TEXT NOT NULL,
    lat REAL NOT NULL,
    lon REAL NOT NULL,
    timestamp INTEGER NOT NULL,
    temperature REAL NOT NULL,
    feels_like REAL NOT NULL,
    humidity INTEGER NOT NULL,
    pressure INTEGER NOT NULL,
    wind_speed REAL NOT NULL,
    wind_direction INTEGER,
    clouds INTEGER,
    visibility INTEGER,
    description TEXT,
    icon TEXT,
    rain_1h REAL,
    snow_1h REAL,
    units TEXT NOT NULL DEFAULT 'metric',
    fetched_at INTEGER NOT NULL,
    UNIQUE(city, timestamp, units)
);

-- Index for querying history by city and time range
CREATE INDEX IF NOT EXISTS idx_history_city_ts ON weather_history(city, timestamp);

-- Index for querying history by city and units
CREATE INDEX IF NOT EXISTS idx_history_city_units ON weather_history(city, units);
