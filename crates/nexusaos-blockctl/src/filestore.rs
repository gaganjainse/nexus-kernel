use std::{collections::HashMap, sync::RwLock};

pub struct BlockFileStore {
    zones: RwLock<HashMap<String, ZoneData>>,
}

struct ZoneData {
    data: Vec<u8>,
    max_size: usize,
}

impl BlockFileStore {
    pub fn new() -> Self {
        Self { zones: RwLock::new(HashMap::new()) }
    }

    pub fn append(&self, block_id: &str, data: &[u8]) {
        let mut zones = self.zones.write().unwrap_or_else(|e| e.into_inner());
        let zone = zones
            .entry(block_id.to_string())
            .or_insert_with(|| ZoneData { data: Vec::new(), max_size: 1_048_576 });

        zone.data.extend_from_slice(data);
        if zone.data.len() > zone.max_size {
            let overflow = zone.data.len() - zone.max_size;
            zone.data.drain(..overflow);
        }
    }

    pub fn read_all(&self, block_id: &str) -> Option<Vec<u8>> {
        let zones = self.zones.read().unwrap_or_else(|e| e.into_inner());
        zones.get(block_id).map(|zone| zone.data.clone())
    }

    pub fn read_tail(&self, block_id: &str, max_bytes: usize) -> Option<Vec<u8>> {
        let zones = self.zones.read().unwrap_or_else(|e| e.into_inner());
        zones.get(block_id).map(|zone| {
            let start = if zone.data.len() > max_bytes { zone.data.len() - max_bytes } else { 0 };
            zone.data[start..].to_vec()
        })
    }

    pub fn truncate(&self, block_id: &str) {
        let mut zones = self.zones.write().unwrap_or_else(|e| e.into_inner());
        if let Some(zone) = zones.get_mut(block_id) {
            zone.data.clear();
        }
    }

    pub fn delete_zone(&self, block_id: &str) {
        let mut zones = self.zones.write().unwrap_or_else(|e| e.into_inner());
        zones.remove(block_id);
    }

    pub fn zone_size(&self, block_id: &str) -> usize {
        let zones = self.zones.read().unwrap_or_else(|e| e.into_inner());
        zones.get(block_id).map(|zone| zone.data.len()).unwrap_or(0)
    }

    pub fn set_max_size(&self, block_id: &str, max_size: usize) {
        let mut zones = self.zones.write().unwrap_or_else(|e| e.into_inner());
        let zone = zones
            .entry(block_id.to_string())
            .or_insert_with(|| ZoneData { data: Vec::new(), max_size });
        zone.max_size = max_size;
        if zone.data.len() > max_size {
            let overflow = zone.data.len() - max_size;
            zone.data.drain(..overflow);
        }
    }
}

impl Default for BlockFileStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_read_all() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"hello");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"hello");
        Ok(())
    }

    #[test]
    fn test_truncate_exceeding_max_size() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.set_max_size("blk1", 5);
        store.append("blk1", b"hello world");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"world");
        Ok(())
    }

    #[test]
    fn test_read_tail() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"1234567890");
        assert_eq!(store.read_tail("blk1", 3).ok_or("unexpected None")?, b"890");
        Ok(())
    }

    #[test]
    fn test_truncate_and_delete() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"123");
        store.truncate("blk1");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"");
        store.delete_zone("blk1");
        assert!(store.read_all("blk1").is_none());
        Ok(())
    }

    #[test]
    fn test_multiple_zones() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"aaa");
        store.append("blk2", b"bbb");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"aaa");
        assert_eq!(store.read_all("blk2").ok_or("unexpected None")?, b"bbb");
        Ok(())
    }

    #[test]
    fn test_default_constructor() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::default();
        assert!(store.read_all("any").is_none());
        assert_eq!(store.zone_size("any"), 0);
        Ok(())
    }

    #[test]
    fn test_append_empty_data() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"");
        assert_eq!(store.zone_size("blk1"), 0);
        Ok(())
    }

    #[test]
    fn test_append_multiple_times() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"hello ");
        store.append("blk1", b"world");
        store.append("blk1", b"!");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"hello world!");
        assert_eq!(store.zone_size("blk1"), 12);
        Ok(())
    }

    #[test]
    fn test_append_exceeds_default_max_size() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        let large_data = vec![b'x'; 2 * 1024 * 1024];
        store.append("blk1", &large_data);
        let size = store.zone_size("blk1");
        assert!(size <= 1024 * 1024);
        assert_eq!(size, 1024 * 1024);
        Ok(())
    }

    #[test]
    fn test_read_all_nonexistent_zone() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        assert!(store.read_all("nonexistent").is_none());
        Ok(())
    }

    #[test]
    fn test_read_tail_nonexistent_zone() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        assert!(store.read_tail("nonexistent", 10).is_none());
        Ok(())
    }

    #[test]
    fn test_read_tail_larger_than_data() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"abc");
        assert_eq!(store.read_tail("blk1", 100).ok_or("unexpected None")?, b"abc");
        Ok(())
    }

    #[test]
    fn test_read_tail_zero_bytes() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"abc");
        assert_eq!(store.read_tail("blk1", 0).ok_or("unexpected None")?, b"");
        Ok(())
    }

    #[test]
    fn test_truncate_nonexistent_zone() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.truncate("nonexistent");
        assert!(store.read_all("nonexistent").is_none());
        Ok(())
    }

    #[test]
    fn test_delete_zone_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.delete_zone("nonexistent");
        assert!(store.read_all("nonexistent").is_none());
        Ok(())
    }

    #[test]
    fn test_zone_size_nonexistent() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        assert_eq!(store.zone_size("nonexistent"), 0);
        Ok(())
    }

    #[test]
    fn test_zone_size_after_operations() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"hello");
        assert_eq!(store.zone_size("blk1"), 5);
        store.truncate("blk1");
        assert_eq!(store.zone_size("blk1"), 0);
        Ok(())
    }

    #[test]
    fn test_set_max_size_creates_zone() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.set_max_size("new_zone", 100);
        assert_eq!(store.zone_size("new_zone"), 0);
        store.append("new_zone", b"data");
        assert_eq!(store.read_all("new_zone").ok_or("unexpected None")?, b"data");
        Ok(())
    }

    #[test]
    fn test_set_max_size_shrinks_existing_data() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"hello world");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"hello world");
        store.set_max_size("blk1", 5);
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"world");
        assert_eq!(store.zone_size("blk1"), 5);
        Ok(())
    }

    #[test]
    fn test_set_max_size_zero() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"hello");
        store.set_max_size("blk1", 0);
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"");
        assert_eq!(store.zone_size("blk1"), 0);
        Ok(())
    }

    #[test]
    fn test_set_max_size_larger_than_current() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.set_max_size("blk1", 100);
        store.append("blk1", b"hello");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"hello");
        Ok(())
    }

    #[test]
    fn test_append_after_truncate() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"original");
        store.truncate("blk1");
        store.append("blk1", b"new");
        assert_eq!(store.read_all("blk1").ok_or("unexpected None")?, b"new");
        Ok(())
    }

    #[test]
    fn test_read_tail_after_multiple_appends() -> Result<(), Box<dyn std::error::Error>> {
        let store = BlockFileStore::new();
        store.append("blk1", b"abc");
        store.append("blk1", b"def");
        store.append("blk1", b"ghi");
        assert_eq!(store.read_tail("blk1", 4).ok_or("unexpected None")?, b"fghi");
        Ok(())
    }

    #[test]
    fn test_concurrent_access() -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::Arc;
        let store = Arc::new(BlockFileStore::new());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let store = store.clone();
                std::thread::spawn(move || {
                    store.append("blk1", &[i as u8]);
                })
            })
            .collect();

        for handle in handles {
            handle.join().map_err(|e| format!("thread error: {:?}", e))?;
        }

        assert_eq!(store.zone_size("blk1"), 10);
        Ok(())
    }
}
