use regex::Regex;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::scraper::ScrapeProgress;

// ─── RAWG API response types ───

#[derive(Debug, Deserialize)]
struct RawgSearchResponse {
    results: Vec<RawgGame>,
}

#[derive(Debug, Deserialize)]
struct RawgGame {
    name: Option<String>,
    slug: Option<String>,
    background_image: Option<String>,
    released: Option<String>,
    metacritic: Option<i32>,
    genres: Option<Vec<RawgGenre>>,
    #[serde(default)]
    short_screenshots: Vec<RawgScreenshot>,
}

#[derive(Debug, Deserialize)]
struct RawgGenre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawgScreenshot {
    image: Option<String>,
}

/// Metadata fetched from RAWG
#[derive(Debug, Clone)]
pub struct GameMetadata {
    pub image_url: Option<String>,
    pub genres: Option<String>,
    pub released: Option<String>,
    pub metacritic: Option<i32>,
}

/// Enrich a list of games with metadata from RAWG API.
/// Updates the progress state during enrichment.
/// Returns a map of game index -> metadata.
pub async fn enrich_games(
    titles: &[String],
    api_key: &str,
    progress: Arc<RwLock<ScrapeProgress>>,
) -> Vec<Option<GameMetadata>> {
    let client = Client::builder()
        .user_agent("FitGirl-Browser/1.0")
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();

    let total = titles.len();
    let mut results: Vec<Option<GameMetadata>> = vec![None; total];
    let mut enriched_count: usize = 0;
    let mut image_count: usize = 0;
    let mut genre_count: usize = 0;

    {
        let mut p = progress.write().await;
        p.phase = "enriching".to_string();
        p.progress = 0.0;
        p.message = format!("Enriching metadata for {} games via RAWG...", total);
    }

    println!("Starting RAWG metadata enrichment for {} games...", total);

    // Process in batches to respect rate limits
    // RAWG free tier: ~5 requests/second
    for (i, title) in titles.iter().enumerate() {
        let clean_title = clean_game_title(title);
        if clean_title.is_empty() {
            results[i] = None;
            continue;
        }

        match search_rawg(&client, api_key, &clean_title).await {
            Some(meta) => {
                if meta.image_url.is_some() {
                    image_count += 1;
                }
                if meta.genres.is_some() {
                    genre_count += 1;
                }
                enriched_count += 1;
                results[i] = Some(meta);
            }
            None => {
                results[i] = None;
            }
        }

        // Update progress every 10 games
        if (i + 1) % 10 == 0 || i + 1 == total {
            let pct = ((i + 1) as f64 / total as f64) * 100.0;
            let mut p = progress.write().await;
            p.phase = "enriching".to_string();
            p.progress = pct;
            p.games_scraped = (i + 1) as i64;
            p.games_total = total as i64;
            p.with_thumbnail = image_count as i64;
            p.with_genres = genre_count as i64;
            p.message = format!(
                "RAWG enrichment {}/{} — 🖼 {} images | 🏷 {} genres",
                i + 1, total, image_count, genre_count
            );
        }

        // Print console progress every 50
        if (i + 1) % 50 == 0 || i + 1 == total {
            println!(
                "  RAWG {}/{} — {} matched, {} images, {} genres",
                i + 1, total, enriched_count, image_count, genre_count
            );
        }

        // Rate limit: ~5 requests per second
        if (i + 1) % 5 == 0 {
            tokio::time::sleep(Duration::from_millis(1100)).await;
        }
    }

    println!(
        "RAWG enrichment complete: {}/{} matched, {} images, {} genres",
        enriched_count, total, image_count, genre_count
    );

    results
}

/// Search RAWG for a game and return metadata
async fn search_rawg(client: &Client, api_key: &str, title: &str) -> Option<GameMetadata> {
    let url = format!(
        "https://api.rawg.io/api/games?key={}&search={}&page_size=1&search_precise=true",
        api_key,
        urlencoding::encode(title)
    );

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(_) => return None,
    };

    if !resp.status().is_success() {
        return None;
    }

    let data: RawgSearchResponse = match resp.json().await {
        Ok(d) => d,
        Err(_) => return None,
    };

    let game = data.results.into_iter().next()?;

    // Use background_image, or first screenshot as fallback
    let image_url = game.background_image
        .or_else(|| {
            game.short_screenshots
                .into_iter()
                .next()
                .and_then(|s| s.image)
        });

    let genres = game.genres
        .filter(|g| !g.is_empty())
        .map(|g| g.into_iter().map(|genre| genre.name).collect::<Vec<_>>().join(", "));

    Some(GameMetadata {
        image_url,
        genres,
        released: game.released,
        metacritic: game.metacritic,
    })
}

/// Clean a FitGirl repack title to extract the base game name for searching.
/// Examples:
///   "Cyberpunk 2077 (v2.13 + All DLCs + Bonus Content, MULTi18)" -> "Cyberpunk 2077"
///   "The Witcher 3: Wild Hunt – Complete Edition" -> "The Witcher 3: Wild Hunt"
///   "DOOM Eternal (v6.66 Rev 2.3 + All DLCs)" -> "DOOM Eternal"
fn clean_game_title(title: &str) -> String {
    let mut clean = title.to_string();

    // Remove anything in parentheses: (v1.2 + DLCs, ...) 
    let paren_re = Regex::new(r"\s*\(.*?\)").unwrap();
    clean = paren_re.replace_all(&clean, "").to_string();

    // Remove anything after " – " or " - " that looks like version/edition info
    // But keep subtitle-like content (e.g. "The Witcher 3: Wild Hunt – Complete Edition" -> keep)
    let dash_re = Regex::new(r"\s+[–—-]\s+(v\d|Build|Update|Repack|Edition|MULTi|DLC|Rev\s).*$").unwrap();
    clean = dash_re.replace(&clean, "").to_string();

    // Remove trailing " / " separated alternate names
    if let Some(pos) = clean.find(" / ") {
        clean = clean[..pos].to_string();
    }

    // Remove "- FitGirl Repack" or similar suffixes
    let fitgirl_re = Regex::new(r"(?i)\s*[-–]\s*fitgirl.*$").unwrap();
    clean = fitgirl_re.replace(&clean, "").to_string();

    // Remove "HD", "Remastered", etc. only if they appear at the very end after cleanup
    // (keep them if they're part of the game name)

    clean.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_game_title() {
        assert_eq!(
            clean_game_title("Cyberpunk 2077 (v2.13 + All DLCs + Bonus Content, MULTi18)"),
            "Cyberpunk 2077"
        );
        assert_eq!(
            clean_game_title("DOOM Eternal (v6.66 Rev 2.3 + All DLCs)"),
            "DOOM Eternal"
        );
        assert_eq!(
            clean_game_title("The Witcher 3: Wild Hunt"),
            "The Witcher 3: Wild Hunt"
        );
        assert_eq!(
            clean_game_title("Elden Ring – v1.12.1 + DLC"),
            "Elden Ring"
        );
    }
}
