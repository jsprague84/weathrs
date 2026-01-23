-- Create devices table for push notification registrations
CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,
    token TEXT UNIQUE NOT NULL,
    platform TEXT NOT NULL,
    device_name TEXT,
    app_version TEXT,
    cities TEXT NOT NULL DEFAULT '[]',
    units TEXT NOT NULL DEFAULT 'imperial',
    enabled INTEGER NOT NULL DEFAULT 1,
    registered_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Create index on token for fast lookups
CREATE INDEX IF NOT EXISTS idx_devices_token ON devices(token);

-- Create index on enabled for filtering
CREATE INDEX IF NOT EXISTS idx_devices_enabled ON devices(enabled);

-- Create scheduler_jobs table for scheduled forecast jobs
CREATE TABLE IF NOT EXISTS scheduler_jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    city TEXT NOT NULL,
    units TEXT NOT NULL DEFAULT 'metric',
    cron TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    include_daily INTEGER NOT NULL DEFAULT 1,
    include_hourly INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    notify_config TEXT NOT NULL DEFAULT '{}'
);

-- Create index on enabled for filtering
CREATE INDEX IF NOT EXISTS idx_jobs_enabled ON scheduler_jobs(enabled);
