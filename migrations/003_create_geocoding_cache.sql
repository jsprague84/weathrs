CREATE TABLE IF NOT EXISTS geocoding_cache (
    city_query TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    lat REAL NOT NULL,
    lon REAL NOT NULL,
    country TEXT NOT NULL,
    state TEXT,
    cached_at INTEGER NOT NULL
);
