use dashmap::DashMap;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A thread-safe cache with TTL (time-to-live) support
pub struct TtlCache<K, V> {
    data: DashMap<K, CacheEntry<V>>,
    ttl: Duration,
}

struct CacheEntry<V> {
    value: V,
    expires_at: Instant,
}

impl<K, V> TtlCache<K, V>
where
    K: std::hash::Hash + Eq + Clone,
    V: Clone,
{
    /// Create a new cache with the specified TTL
    pub fn new(ttl: Duration) -> Self {
        Self {
            data: DashMap::new(),
            ttl,
        }
    }

    /// Get a value from the cache if it exists and hasn't expired
    pub fn get(&self, key: &K) -> Option<V> {
        let entry = self.data.get(key)?;
        if entry.expires_at > Instant::now() {
            Some(entry.value.clone())
        } else {
            drop(entry);
            self.data.remove(key);
            None
        }
    }

    /// Insert a value into the cache
    pub fn insert(&self, key: K, value: V) {
        let entry = CacheEntry {
            value,
            expires_at: Instant::now() + self.ttl,
        };
        self.data.insert(key, entry);
    }

    /// Remove expired entries from the cache
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.data.retain(|_, entry| entry.expires_at > now);
    }

    /// Get the number of entries in the cache (including expired ones)
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the cache is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Geocoding cache for storing location lookups
pub type GeoCache = Arc<GeoCacheWithDb>;

/// Cached version of GeoLocation (needs Clone)
#[derive(Debug, Clone)]
pub struct CachedGeoLocation {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
    pub state: Option<String>,
}

/// Default SQLite geocoding cache TTL: 7 days
const GEO_CACHE_DB_TTL_SECS: i64 = 7 * 24 * 60 * 60;

/// Geocoding cache backed by in-memory DashMap + SQLite persistence
pub struct GeoCacheWithDb {
    memory: TtlCache<String, CachedGeoLocation>,
    pool: SqlitePool,
    db_ttl_secs: i64,
}

impl GeoCacheWithDb {
    /// Create a new geocoding cache with in-memory TTL of 24 hours
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            memory: TtlCache::new(Duration::from_secs(24 * 60 * 60)),
            pool,
            db_ttl_secs: GEO_CACHE_DB_TTL_SECS,
        }
    }

    /// Get a cached geo location: checks memory first, then SQLite
    pub async fn get(&self, key: &str) -> Option<CachedGeoLocation> {
        // Check in-memory cache first
        if let Some(cached) = self.memory.get(&key.to_string()) {
            metrics::counter!(crate::metrics::CACHE_HITS, "layer" => "memory").increment(1);
            return Some(cached);
        }

        // Check SQLite
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let min_cached_at = now - self.db_ttl_secs;

        let row: Option<GeoCacheRow> = sqlx::query_as(
            "SELECT city_query, name, lat, lon, country, state, cached_at
             FROM geocoding_cache WHERE city_query = ? AND cached_at > ?",
        )
        .bind(key)
        .bind(min_cached_at)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to query geocoding cache from SQLite");
            None
        });

        if let Some(row) = row {
            let location = CachedGeoLocation {
                name: row.name,
                lat: row.lat,
                lon: row.lon,
                country: row.country,
                state: row.state,
            };
            // Promote to in-memory cache
            self.memory.insert(key.to_string(), location.clone());
            metrics::counter!(crate::metrics::CACHE_HITS, "layer" => "sqlite").increment(1);
            tracing::debug!(key = %key, "Geocoding cache hit (SQLite)");
            Some(location)
        } else {
            metrics::counter!(crate::metrics::CACHE_MISSES).increment(1);
            None
        }
    }

    /// Insert a geo location into both memory and SQLite caches
    pub async fn insert(&self, key: String, value: CachedGeoLocation) {
        // Insert into memory
        self.memory.insert(key.clone(), value.clone());

        // Insert into SQLite
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if let Err(e) = sqlx::query(
            "INSERT OR REPLACE INTO geocoding_cache (city_query, name, lat, lon, country, state, cached_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&key)
        .bind(&value.name)
        .bind(value.lat)
        .bind(value.lon)
        .bind(&value.country)
        .bind(&value.state)
        .bind(now)
        .execute(&self.pool)
        .await
        {
            tracing::warn!(error = %e, "Failed to persist geocoding cache to SQLite");
        }
    }

    /// Cleanup: remove expired entries from both memory and SQLite
    pub async fn cleanup(&self) {
        self.memory.cleanup();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let min_cached_at = now - self.db_ttl_secs;

        match sqlx::query("DELETE FROM geocoding_cache WHERE cached_at <= ?")
            .bind(min_cached_at)
            .execute(&self.pool)
            .await
        {
            Ok(result) => {
                let deleted = result.rows_affected();
                if deleted > 0 {
                    tracing::debug!(
                        deleted = deleted,
                        "Geocoding SQLite cache cleanup completed"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to cleanup geocoding SQLite cache");
            }
        }
    }

    /// Get in-memory cache size
    pub fn memory_len(&self) -> usize {
        self.memory.len()
    }
}

#[derive(sqlx::FromRow)]
struct GeoCacheRow {
    #[allow(dead_code)]
    city_query: String,
    name: String,
    lat: f64,
    lon: f64,
    country: String,
    state: Option<String>,
    #[allow(dead_code)]
    cached_at: i64,
}

/// Normalize a location string for cache key
/// Converts to lowercase and trims whitespace
pub fn normalize_cache_key(location: &str) -> String {
    location.trim().to_lowercase()
}

/// Create a geocoding cache backed by SQLite
pub fn create_geo_cache(pool: SqlitePool) -> GeoCache {
    Arc::new(GeoCacheWithDb::new(pool))
}

/// Start a background task that cleans up expired cache entries hourly
pub fn start_cache_cleanup_task(cache: GeoCache) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60)); // 1 hour
        loop {
            interval.tick().await;
            let before = cache.memory_len();
            cache.cleanup().await;
            let after = cache.memory_len();
            if before != after {
                tracing::debug!(
                    removed = before - after,
                    remaining = after,
                    "Geocoding cache cleanup completed"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_get() {
        let cache: TtlCache<String, String> = TtlCache::new(Duration::from_secs(60));
        cache.insert("key".to_string(), "value".to_string());
        assert_eq!(cache.get(&"key".to_string()), Some("value".to_string()));
    }

    #[test]
    fn test_cache_miss() {
        let cache: TtlCache<String, String> = TtlCache::new(Duration::from_secs(60));
        assert_eq!(cache.get(&"missing".to_string()), None);
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let cache: TtlCache<String, String> = TtlCache::new(Duration::from_millis(1));
        cache.insert("key".to_string(), "value".to_string());
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(cache.get(&"key".to_string()), None);
    }

    #[test]
    fn test_cache_cleanup() {
        let cache: TtlCache<String, String> = TtlCache::new(Duration::from_millis(1));
        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());
        std::thread::sleep(Duration::from_millis(10));
        cache.cleanup();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_normalize_cache_key() {
        assert_eq!(normalize_cache_key("  Chicago  "), "chicago");
        assert_eq!(normalize_cache_key("NEW YORK"), "new york");
        assert_eq!(normalize_cache_key("London,GB"), "london,gb");
    }
}
