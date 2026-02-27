use lru::LruCache;
use murmur_protocol::CompletionResponse;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;

/// LRU cache for completion responses.
pub struct CompletionCache {
    inner: LruCache<u64, CacheEntry>,
}

struct CacheEntry {
    response: CompletionResponse,
    created_at: std::time::Instant,
}

const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300); // 5 minutes

impl CompletionCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(100).unwrap()),
            ),
        }
    }

    /// Build a cache key from input + context.
    pub fn cache_key(input: &str, cwd: &str, shell: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        cwd.hash(&mut hasher);
        shell.hash(&mut hasher);
        hasher.finish()
    }

    /// Get a cached response, if it exists and hasn't expired.
    pub fn get(&mut self, key: u64) -> Option<CompletionResponse> {
        if let Some(entry) = self.inner.get(&key) {
            if entry.created_at.elapsed() < CACHE_TTL {
                return Some(entry.response.clone());
            }
            // Expired — remove it
            self.inner.pop(&key);
        }
        None
    }

    /// Store a response in the cache.
    pub fn put(&mut self, key: u64, response: CompletionResponse) {
        self.inner.put(
            key,
            CacheEntry {
                response,
                created_at: std::time::Instant::now(),
            },
        );
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Clear all entries.
    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use murmur_protocol::{CompletionItem, CompletionKind};

    fn make_response() -> CompletionResponse {
        CompletionResponse {
            items: vec![CompletionItem {
                text: "git commit".to_string(),
                description: Some("Commit changes".to_string()),
                kind: CompletionKind::Command,
                score: 1.0,
            }],
            provider: "test".to_string(),
            latency_ms: 50,
            cached: false,
        }
    }

    #[test]
    fn cache_put_and_get() {
        let mut cache = CompletionCache::new(10);
        let key = CompletionCache::cache_key("git c", "/home", "zsh");
        let response = make_response();

        cache.put(key, response.clone());
        let cached = cache.get(key);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().items[0].text, "git commit");
    }

    #[test]
    fn cache_miss() {
        let mut cache = CompletionCache::new(10);
        let key = CompletionCache::cache_key("git c", "/home", "zsh");
        assert!(cache.get(key).is_none());
    }

    #[test]
    fn different_inputs_different_keys() {
        let key1 = CompletionCache::cache_key("git c", "/home", "zsh");
        let key2 = CompletionCache::cache_key("git s", "/home", "zsh");
        assert_ne!(key1, key2);
    }
}
