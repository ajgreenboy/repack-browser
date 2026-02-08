use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use std::collections::HashSet;
use tokio::sync::RwLock;

use super::{GameScraper, LinkType, ScrapedGame, ScrapeProgress};
use super::utils::{self, WpPost};

pub struct SteamRipScraper {
    client: Client,
}

impl SteamRipScraper {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .timeout(Duration::from_secs(60))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Extract DDL links from HTML content
    /// Only returns links from supported Real-Debrid hosters
    fn extract_ddl_links(&self, html: &str, supported_hosts: &HashSet<String>) -> Option<String> {
        // Common DDL providers to check (in priority order)
        let providers = [
            "1fichier.com",
            "buzzheavier.com",
            "vikingfile.com",
            "filecrypt.cc",
            "gofile.io",
            "rapidgator.net",
            "uploaded.net",
            "mega.nz",
            "mediafire.com",
            "pixeldrain.com",
        ];

        // Try to find links in order of preference
        for provider in &providers {
            if let Some(link) = self.extract_link_for_provider(html, provider) {
                // Check if this hoster is supported by Real-Debrid
                if crate::realdebrid::RealDebridClient::is_supported_hoster(&link, supported_hosts) {
                    return Some(link);
                }
            }
        }

        None
    }

    fn extract_link_for_provider(&self, html: &str, provider: &str) -> Option<String> {
        // Match href="https://provider.com/..." or href="http://provider.com/..."
        let pattern = format!(r#"href="(https?://[^"]*{}[^"]*)"#, regex::escape(provider));
        let re = Regex::new(&pattern).ok()?;

        re.captures(html)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }
}

#[async_trait]
impl GameScraper for SteamRipScraper {
    async fn scrape_all_games(
        &self,
        progress: Arc<RwLock<ScrapeProgress>>
    ) -> Result<Vec<ScrapedGame>, Box<dyn std::error::Error>> {
        let base_url = "https://steamrip.com/wp-json/wp/v2/posts";
        let per_page = 100; // Max allowed by WP REST API

        // Get supported hosters from Real-Debrid (using anonymous client for validation)
        println!("Fetching supported hosters from Real-Debrid...");
        let rd_client = crate::realdebrid::RealDebridClient::new(String::new()); // Empty key for host list
        let supported_hosts = rd_client.get_supported_hosts().await
            .unwrap_or_else(|e| {
                eprintln!("Warning: Could not fetch Real-Debrid hosts: {}. Using default list.", e);
                // Fallback to known supported hosts
                let mut defaults = HashSet::new();
                defaults.insert("1fichier.com".to_string());
                defaults.insert("rapidgator.net".to_string());
                defaults.insert("uploaded.net".to_string());
                defaults.insert("mega.nz".to_string());
                defaults.insert("mediafire.com".to_string());
                defaults
            });
        println!("Validated {} supported hosters", supported_hosts.len());

        // Phase 1: Discover total pages
        {
            let mut p = progress.write().await;
            p.phase = "fetching_pages".to_string();
            p.message = "Connecting to SteamRIP API...".to_string();
            p.progress = 0.0;
        }

        let first_url = format!("{}?per_page={}&page=1&_embed=wp:featuredmedia&_fields=id,date,link,title,content,_embedded", base_url, per_page);
        let first_response = self.client.get(&first_url).send().await?;

        let total_pages: i64 = first_response
            .headers()
            .get("X-WP-TotalPages")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        let total_posts: i64 = first_response
            .headers()
            .get("X-WP-Total")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        println!("SteamRIP API reports {} total posts across {} pages", total_posts, total_pages);

        let first_posts: Vec<WpPost> = first_response.json().await?;

        {
            let mut p = progress.write().await;
            p.phase = "scraping_games".to_string();
            p.pages_found = total_pages;
            p.games_total = total_posts;
            p.games_scraped = first_posts.len() as i64;
            p.progress = 2.0;
            p.message = format!("Fetching SteamRIP posts (page 1/{})...", total_pages);
        }

        // Parse first page
        let mut all_games: Vec<ScrapedGame> = Vec::new();
        let mut posts_without_link: i64 = 0;
        for post in &first_posts {
            if let Some(game) = self.parse_wp_post(post, &supported_hosts) {
                all_games.push(game);
            } else {
                posts_without_link += 1;
            }
        }
        utils::update_metadata_counts(&progress, &all_games, posts_without_link).await;

        // Phase 2: Fetch remaining pages
        let batch_size = 5;
        let mut current_page: i64 = 2;

        while current_page <= total_pages {
            let end_page = std::cmp::min(current_page + batch_size - 1, total_pages);
            let mut handles = Vec::new();

            for page_num in current_page..=end_page {
                let client = self.client.clone();
                let url = format!(
                    "{}?per_page={}&page={}&_embed=wp:featuredmedia&_fields=id,date,link,title,content,_embedded",
                    base_url, per_page, page_num
                );

                handles.push(tokio::spawn(async move {
                    match client.get(&url).send().await {
                        Ok(resp) => {
                            if resp.status().is_success() {
                                match resp.json::<Vec<WpPost>>().await {
                                    Ok(posts) => Some((page_num, posts)),
                                    Err(e) => {
                                        eprintln!("  Failed to parse SteamRIP page {}: {}", page_num, e);
                                        None
                                    }
                                }
                            } else {
                                None
                            }
                        }
                        Err(e) => {
                            eprintln!("  Failed to fetch SteamRIP page {}: {}", page_num, e);
                            None
                        }
                    }
                }));
            }

            for handle in handles {
                if let Ok(Some((_page_num, posts))) = handle.await {
                    for post in &posts {
                        if let Some(game) = self.parse_wp_post(post, &supported_hosts) {
                            all_games.push(game);
                        } else {
                            posts_without_link += 1;
                        }
                    }
                }
            }

            utils::update_metadata_counts(&progress, &all_games, posts_without_link).await;
            {
                let mut p = progress.write().await;
                let pct = 2.0 + (end_page as f64 / total_pages as f64) * 88.0;
                p.games_scraped = all_games.len() as i64;
                p.progress = pct;
                p.message = format!(
                    "SteamRIP page {}/{} — {} games | 🖼 {} images",
                    end_page, total_pages, all_games.len(), p.with_thumbnail
                );
            }

            if (end_page % 10) == 0 || end_page == total_pages {
                let p = progress.read().await;
                println!(
                    "  SteamRIP page {}/{} — {} games | {} thumbnails | {} skipped (no DDL)",
                    end_page, total_pages, all_games.len(), p.with_thumbnail, posts_without_link
                );
            }

            current_page = end_page + 1;
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        {
            let mut p = progress.write().await;
            p.phase = "done".to_string();
            p.games_scraped = all_games.len() as i64;
            p.progress = 95.0;
            p.message = format!("Saving {} SteamRIP games — 🖼 {} images", all_games.len(), p.with_thumbnail);
        }

        println!("SteamRIP scrape complete: {} valid games with DDL", all_games.len());
        Ok(all_games)
    }

    fn source_name(&self) -> &'static str {
        "steamrip"
    }

    fn source_label(&self) -> &'static str {
        "SteamRIP"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ─── Post parsing ───

impl SteamRipScraper {
    fn parse_wp_post(&self, post: &WpPost, supported_hosts: &HashSet<String>) -> Option<ScrapedGame> {
        let title = utils::html_to_text(&post.title.rendered);
        if title.is_empty() {
            return None;
        }

        let content_html = &post.content.rendered;
        let content_text = utils::html_to_text(content_html);

        // Extract DDL link from HTML content (only from supported hosters)
        let ddl_link = self.extract_ddl_links(content_html, supported_hosts)?;

        // Extract file size - SteamRIP typically shows "Size: XX GB"
        let file_size = utils::extract_field(&content_text, r"(?i)(?:size|file size)\s*[:\s]\s*(.+?)(?:\n|$)")
            .unwrap_or_else(|| "N/A".to_string());

        // Extract genres if available
        let genres = utils::extract_field(&content_text, r"(?i)(?:genre|genres)\s*[:\s]\s*(.+?)(?:\n|$)")
            .map(|g| g.trim_end_matches(|c: char| c == '.' || c == ',').to_string());

        // Get thumbnail URL from featured media (strict_types=false for SteamRIP)
        let thumbnail_url = post.embedded.as_ref()
            .and_then(|e| e.featured_media.as_ref())
            .and_then(|media| media.first())
            .and_then(|m| {
                m.media_details.as_ref()
                    .and_then(|d| d.sizes.as_ref())
                    .and_then(|s| {
                        s.medium.as_ref().and_then(|ms| ms.source_url.clone())
                            .or_else(|| s.medium_large.as_ref().and_then(|ms| ms.source_url.clone()))
                            .or_else(|| s.thumbnail.as_ref().and_then(|ms| ms.source_url.clone()))
                    })
                    .or_else(|| m.source_url.clone())
            })
            .or_else(|| utils::extract_first_image(content_html, false));

        // Extract screenshots (less strict for SteamRIP)
        let screenshots = utils::extract_all_images(content_html, false);
        let screenshots = if screenshots.is_empty() {
            None
        } else {
            Some(screenshots.join("|||"))
        };

        Some(ScrapedGame {
            title,
            source: "steamrip".to_string(),
            file_size,
            download_link: ddl_link,
            link_type: LinkType::DirectDL,
            genres,
            company: None,  // SteamRIP doesn't typically list company
            original_size: None,
            thumbnail_url,
            screenshots,
            source_url: post.link.clone(),
            post_date: post.date.clone(),
        })
    }
}
