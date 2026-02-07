use super::GameScraper;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry for managing multiple game scrapers
pub struct ScraperRegistry {
    scrapers: HashMap<String, Arc<dyn GameScraper>>,
}

impl ScraperRegistry {
    /// Create a new registry with default scrapers
    pub fn new() -> Self {
        Self {
            scrapers: HashMap::new(),
        }
    }

    /// Register a scraper
    pub fn register(&mut self, scraper: Arc<dyn GameScraper>) {
        self.scrapers.insert(
            scraper.source_name().to_string(),
            scraper
        );
    }

    /// Get a scraper by source name
    pub fn get(&self, source: &str) -> Option<Arc<dyn GameScraper>> {
        self.scrapers.get(source).cloned()
    }

    /// List all available sources as (source_name, source_label) pairs
    pub fn list_sources(&self) -> Vec<(&str, &str)> {
        self.scrapers.iter()
            .map(|(_, s)| (s.source_name(), s.source_label()))
            .collect()
    }

    /// Get all scrapers
    pub fn all(&self) -> Vec<Arc<dyn GameScraper>> {
        self.scrapers.values().cloned().collect()
    }
}

impl Default for ScraperRegistry {
    fn default() -> Self {
        Self::new()
    }
}
