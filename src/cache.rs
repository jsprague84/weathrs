use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Geocoding cache for storing location lookups
pub type GeoCache = Arc<TtlCache<String, CachedGeoLocation>>;

/// Cached version of GeoLocation (needs Clone)
#[derive(Debug, Clone)]
pub struct CachedGeoLocation {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
    pub state: Option<String>,
}

/// Create a geocoding cache with 24-hour TTL
pub fn create_geo_cache() -> GeoCache {
    Arc::new(TtlCache::new(Duration::from_secs(24 * 60 * 60)))
}

/// Normalize a location string for cache key
/// Converts to lowercase and trims whitespace
pub fn normalize_cache_key(location: &str) -> String {
    location.trim().to_lowercase()
}

/// Start a background task that cleans up expired cache entries hourly
pub fn start_cache_cleanup_task(cache: GeoCache) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60 * 60)); // 1 hour
        loop {
            interval.tick().await;
            let before = cache.len();
            cache.cleanup();
            let after = cache.len();
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
