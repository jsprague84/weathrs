CREATE TABLE IF NOT EXISTS tile_usage (
    date TEXT NOT NULL,
    owm_tiles INTEGER NOT NULL DEFAULT 0,
    google_maps_tiles INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(date)
);
