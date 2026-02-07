use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, sqlite::SqlitePoolOptions};

// ─── Download-related row types ───

#[derive(Debug, Clone, FromRow)]
pub struct DownloadRow {
    pub id: i64,
    pub game_id: i64,
    pub status: String,
    pub progress: f64,
    pub download_speed: Option<String>,
    pub eta: Option<String>,
    pub file_path: Option<String>,
    pub installer_path: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub game_title: String,
    pub game_size: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct DownloadFileRow {
    pub id: i64,
    pub filename: String,
    pub file_size: Option<i64>,
    pub file_path: Option<String>,
    pub is_extracted: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Game {
    pub id: i64,
    pub title: String,
    pub source: String,  // "fitgirl" or "steamrip"
    pub file_size: String,
    pub magnet_link: String,  // Can be magnet link or DDL
    pub genres: Option<String>,
    pub company: Option<String>,
    pub original_size: Option<String>,
    pub thumbnail_url: Option<String>,
    pub screenshots: Option<String>,
    pub source_url: Option<String>,
    pub post_date: Option<String>,
    pub search_title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GameQuery {
    pub search: Option<String>,
    pub sort: Option<String>,
    pub genre: Option<String>,
    pub source: Option<String>,  // Filter by source
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// Initialize the database connection pool and run migrations.
pub async fn init_db(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Create tables if they don't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS games (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            file_size TEXT NOT NULL DEFAULT '',
            magnet_link TEXT NOT NULL,
            genres TEXT,
            company TEXT,
            original_size TEXT,
            thumbnail_url TEXT,
            screenshots TEXT,
            source_url TEXT,
            post_date TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Migrations for existing DBs - add new columns if they don't exist
    for col in &["genres", "company", "original_size", "thumbnail_url", "source_url", "post_date", "screenshots", "search_title"] {
        let _ = sqlx::query(&format!("ALTER TABLE games ADD COLUMN {} TEXT", col))
            .execute(&pool)
            .await;
    }

    // Add source column with default value 'fitgirl' for backward compatibility
    let _ = sqlx::query("ALTER TABLE games ADD COLUMN source TEXT DEFAULT 'fitgirl'")
        .execute(&pool)
        .await;

    // Set source='fitgirl' for existing games that have NULL source
    let _ = sqlx::query("UPDATE games SET source = 'fitgirl' WHERE source IS NULL")
        .execute(&pool)
        .await;

    // Add index for search performance
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_games_title ON games(title COLLATE NOCASE)"
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_games_search_title ON games(search_title COLLATE NOCASE)"
    )
    .execute(&pool)
    .await?;

    // Add index for source filtering
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_games_source ON games(source)"
    )
    .execute(&pool)
    .await?;

    // System checks table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS system_checks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            check_date TEXT NOT NULL,
            ram_available_gb REAL,
            temp_space_gb REAL,
            cpu_cores INTEGER,
            antivirus_active BOOLEAN,
            missing_dlls TEXT,
            missing_dependencies TEXT,
            overall_status TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Installation logs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS installation_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            game_id INTEGER,
            started_at TEXT NOT NULL,
            completed_at TEXT,
            status TEXT NOT NULL,
            error_code TEXT,
            error_message TEXT,
            ram_usage_peak REAL,
            install_duration_minutes INTEGER,
            FOREIGN KEY (game_id) REFERENCES games(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Community ratings table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS community_ratings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            game_id INTEGER NOT NULL,
            install_difficulty INTEGER,
            install_success BOOLEAN,
            issues_encountered TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY (game_id) REFERENCES games(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Game requirements table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS game_requirements (
            game_id INTEGER PRIMARY KEY,
            min_ram_gb INTEGER,
            rec_ram_gb INTEGER,
            min_cpu TEXT,
            rec_cpu TEXT,
            min_gpu TEXT,
            rec_gpu TEXT,
            disk_space_gb INTEGER,
            requires_directx TEXT,
            requires_dotnet TEXT,
            requires_vcredist TEXT,
            FOREIGN KEY (game_id) REFERENCES games(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Download management tables
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS downloads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            game_id INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'queued',
            progress REAL DEFAULT 0.0,
            download_speed TEXT,
            eta TEXT,
            file_path TEXT,
            installer_path TEXT,
            error_message TEXT,
            created_at TEXT NOT NULL,
            completed_at TEXT,
            FOREIGN KEY (game_id) REFERENCES games(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Migration: add installer_path column if it doesn't exist (for existing DBs)
    let _ = sqlx::query("ALTER TABLE downloads ADD COLUMN installer_path TEXT")
        .execute(&pool)
        .await;

    // Settings key-value table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS download_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            download_id INTEGER NOT NULL,
            filename TEXT NOT NULL,
            file_size INTEGER,
            file_path TEXT,
            is_extracted BOOLEAN DEFAULT 0,
            FOREIGN KEY (download_id) REFERENCES downloads(id)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

/// Query games with search, sort, and pagination.
pub async fn query_games(
    pool: &SqlitePool,
    query: GameQuery,
) -> Result<(Vec<Game>, i64), sqlx::Error> {
    let per_page = query.per_page.unwrap_or(50);
    let page = query.page.unwrap_or(1);
    let offset = (page - 1) * per_page;

    let search_pattern = query
        .search
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", s));

    let genre_pattern = query
        .genre
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", s));

    let order_clause = match query.sort.as_deref() {
        Some("title_asc") => "title COLLATE NOCASE ASC",
        Some("title_desc") => "title COLLATE NOCASE DESC",
        Some("size_asc") => "file_size ASC",
        Some("size_desc") => "file_size DESC",
        Some("date_asc") => "COALESCE(post_date, '') ASC, id ASC",
        Some("date_desc") => "COALESCE(post_date, '') DESC, id DESC",
        _ => "id DESC",
    };

    // Build WHERE clauses dynamically
    let mut conditions: Vec<String> = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref pattern) = search_pattern {
        conditions.push("(title LIKE ? OR search_title LIKE ?)".to_string());
        bind_values.push(pattern.clone());
        bind_values.push(pattern.clone());
    }

    if let Some(ref pattern) = genre_pattern {
        conditions.push("genres LIKE ?".to_string());
        bind_values.push(pattern.clone());
    }

    // Filter by source
    if let Some(ref source) = query.source {
        if source != "all" && !source.is_empty() {
            conditions.push("source = ?".to_string());
            bind_values.push(source.clone());
        }
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count total matching rows
    let count_sql = format!("SELECT COUNT(*) FROM games {}", where_clause);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    for val in &bind_values {
        count_query = count_query.bind(val);
    }
    let total: i64 = count_query.fetch_one(pool).await?;

    // Fetch page of results
    let select_sql = format!(
        "SELECT id, title, source, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date, search_title FROM games {} ORDER BY {} LIMIT ? OFFSET ?",
        where_clause, order_clause
    );
    let mut select_query = sqlx::query_as::<_, Game>(&select_sql);
    for val in &bind_values {
        select_query = select_query.bind(val);
    }
    let games = select_query
        .bind(per_page)
        .bind(offset)
        .fetch_all(pool)
        .await?;

    Ok((games, total))
}

/// Get all unique genres from the database, split by comma.
pub async fn get_all_genres(pool: &SqlitePool) -> Result<Vec<(String, i64)>, sqlx::Error> {
    // Get all genre strings
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT genres FROM games WHERE genres IS NOT NULL AND genres != ''"
    )
    .fetch_all(pool)
    .await?;

    // Split by comma, count occurrences
    let mut genre_counts: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (genres_str,) in rows {
        for genre in genres_str.split(',') {
            let trimmed = genre.trim().to_string();
            if !trimmed.is_empty() {
                *genre_counts.entry(trimmed).or_insert(0) += 1;
            }
        }
    }

    // Sort by count descending
    let mut genres: Vec<(String, i64)> = genre_counts.into_iter().collect();
    genres.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(genres)
}

/// Get a random game
pub async fn get_random_game(pool: &SqlitePool) -> Result<Game, sqlx::Error> {
    sqlx::query_as::<_, Game>(
        "SELECT id, title, source, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date, search_title FROM games ORDER BY RANDOM() LIMIT 1"
    )
    .fetch_one(pool)
    .await
}

/// Get a single game by ID.
pub async fn get_game_by_id(pool: &SqlitePool, id: i64) -> Result<Game, sqlx::Error> {
    sqlx::query_as::<_, Game>(
        "SELECT id, title, source, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date, search_title FROM games WHERE id = ?"
    )
    .bind(id)
    .fetch_one(pool)
    .await
}

/// Get existing metadata cache — returns map of lowercase title -> (thumbnail_url, genres)
/// Used to avoid re-querying RAWG for games we already have metadata for.
pub async fn get_metadata_cache(pool: &SqlitePool) -> Result<std::collections::HashMap<String, (Option<String>, Option<String>)>, sqlx::Error> {
    let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT title, thumbnail_url, genres FROM games WHERE thumbnail_url IS NOT NULL OR genres IS NOT NULL"
    )
    .fetch_all(pool)
    .await?;

    let mut cache = std::collections::HashMap::new();
    for (title, thumb, genres) in rows {
        let norm = title.to_lowercase()
            .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        cache.insert(norm, (thumb, genres));
    }
    Ok(cache)
}

/// A game record ready for insertion
pub struct GameInsert {
    pub title: String,
    pub source: String,  // "fitgirl" or "steamrip"
    pub file_size: String,
    pub magnet_link: String,  // Can be magnet link or DDL
    pub genres: Option<String>,
    pub company: Option<String>,
    pub original_size: Option<String>,
    pub thumbnail_url: Option<String>,
    pub screenshots: Option<String>,
    pub source_url: Option<String>,
    pub post_date: Option<String>,
    pub search_title: Option<String>,
}

/// Clean a game title for search indexing.
/// Strips version numbers, DLC lists, language tags, parenthetical info, etc.
/// so that searching "Cyberpunk 2077" matches "Cyberpunk 2077 (v2.13 + All DLCs + Bonus Content, MULTi18)"
pub fn clean_search_title(title: &str) -> String {
    let mut clean = title.to_string();

    // Remove anything in parentheses: (v1.2 + DLCs, ...)
    let paren_re = regex::Regex::new(r"\s*\(.*?\)").unwrap();
    clean = paren_re.replace_all(&clean, "").to_string();

    // Remove anything after " – " or " - " that looks like version/edition info
    let dash_re = regex::Regex::new(r"\s+[–—-]\s+(v\d|Build|Update|Repack|MULTi|DLC|Rev\s).*$").unwrap();
    clean = dash_re.replace(&clean, "").to_string();

    // Remove trailing " / " separated alternate names
    if let Some(pos) = clean.find(" / ") {
        clean = clean[..pos].to_string();
    }

    // Remove "- FitGirl Repack" or similar suffixes
    let fitgirl_re = regex::Regex::new(r"(?i)\s*[-–]\s*fitgirl.*$").unwrap();
    clean = fitgirl_re.replace(&clean, "").to_string();

    // Remove trailing edition suffixes that are noise for search
    let edition_noise = regex::Regex::new(r"(?i)\s+(Digital Deluxe|Ultimate|Complete|Game of the Year|GOTY|Gold|Premium|Definitive|Enhanced|Legendary|Special)\s*(Edition)?$").unwrap();
    clean = edition_noise.replace(&clean, "").to_string();

    clean.trim().to_string()
}

/// Atomically replace all games in a single transaction.
/// Deletes existing games and inserts new ones; rolls back on failure.
pub async fn replace_all_games(
    pool: &SqlitePool,
    games: Vec<GameInsert>,
) -> Result<usize, sqlx::Error> {
    let count = games.len();
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM games")
        .execute(&mut *tx)
        .await?;

    for g in &games {
        sqlx::query(
            "INSERT INTO games (title, source, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date, search_title) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
            .bind(&g.title)
            .bind(&g.source)
            .bind(&g.file_size)
            .bind(&g.magnet_link)
            .bind(&g.genres)
            .bind(&g.company)
            .bind(&g.original_size)
            .bind(&g.thumbnail_url)
            .bind(&g.screenshots)
            .bind(&g.source_url)
            .bind(&g.post_date)
            .bind(&g.search_title)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(count)
}

/// Clear all games from the database.
#[allow(dead_code)]
pub async fn clear_games(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM games").execute(pool).await?;
    Ok(())
}

/// Insert games without clearing first. Returns count inserted.
#[allow(dead_code)]
pub async fn insert_games(
    pool: &SqlitePool,
    games: Vec<GameInsert>,
) -> Result<usize, sqlx::Error> {
    let count = games.len();

    for g in &games {
        sqlx::query(
            "INSERT INTO games (title, source, file_size, magnet_link, genres, company, original_size, thumbnail_url, screenshots, source_url, post_date, search_title) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
            .bind(&g.title)
            .bind(&g.source)
            .bind(&g.file_size)
            .bind(&g.magnet_link)
            .bind(&g.genres)
            .bind(&g.company)
            .bind(&g.original_size)
            .bind(&g.thumbnail_url)
            .bind(&g.screenshots)
            .bind(&g.source_url)
            .bind(&g.post_date)
            .bind(&g.search_title)
            .execute(pool)
            .await?;
    }

    Ok(count)
}

// ─── Settings ───

/// Get a setting value by key. Returns None if not found.
pub async fn get_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM settings WHERE key = ?"
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(v,)| v))
}

/// Set a setting value (upsert).
pub async fn set_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value"
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a setting by key.
pub async fn delete_setting(pool: &SqlitePool, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get all settings as key-value pairs.
pub async fn get_all_settings(pool: &SqlitePool) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value FROM settings ORDER BY key"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ─── New Feature Tables ───

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct SystemCheck {
    pub id: i64,
    pub check_date: String,
    pub ram_available_gb: Option<f64>,
    pub temp_space_gb: Option<f64>,
    pub cpu_cores: Option<i64>,
    pub antivirus_active: Option<bool>,
    pub missing_dlls: Option<String>,
    pub missing_dependencies: Option<String>,
    pub overall_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct InstallationLog {
    pub id: i64,
    pub game_id: Option<i64>,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub ram_usage_peak: Option<f64>,
    pub install_duration_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct CommunityRating {
    pub id: i64,
    pub game_id: i64,
    pub install_difficulty: Option<i64>,
    pub install_success: Option<bool>,
    pub issues_encountered: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GameRequirement {
    pub game_id: i64,
    pub min_ram_gb: Option<i64>,
    pub rec_ram_gb: Option<i64>,
    pub min_cpu: Option<String>,
    pub rec_cpu: Option<String>,
    pub min_gpu: Option<String>,
    pub rec_gpu: Option<String>,
    pub disk_space_gb: Option<i64>,
    pub requires_directx: Option<String>,
    pub requires_dotnet: Option<String>,
    pub requires_vcredist: Option<String>,
}

// ─── Source Statistics ───

#[derive(Debug, Clone, Serialize)]
pub struct SourceStat {
    pub source: String,
    pub count: i64,
}

/// Get game count per source
pub async fn get_source_stats(pool: &SqlitePool) -> Result<Vec<SourceStat>, sqlx::Error> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT source, COUNT(*) as count FROM games GROUP BY source ORDER BY source"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|(source, count)| SourceStat { source, count }).collect())
}

// ─── System Checks ───

/// Insert a new system check
pub async fn insert_system_check(
    pool: &SqlitePool,
    ram_available_gb: Option<f64>,
    temp_space_gb: Option<f64>,
    cpu_cores: Option<i64>,
    antivirus_active: Option<bool>,
    missing_dlls: Option<String>,
    missing_dependencies: Option<String>,
    overall_status: Option<String>,
) -> Result<i64, sqlx::Error> {
    let check_date = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        "INSERT INTO system_checks (check_date, ram_available_gb, temp_space_gb, cpu_cores, antivirus_active, missing_dlls, missing_dependencies, overall_status) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&check_date)
    .bind(ram_available_gb)
    .bind(temp_space_gb)
    .bind(cpu_cores)
    .bind(antivirus_active)
    .bind(missing_dlls)
    .bind(missing_dependencies)
    .bind(overall_status)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Get the latest system check
pub async fn get_latest_system_check(pool: &SqlitePool) -> Result<Option<SystemCheck>, sqlx::Error> {
    sqlx::query_as::<_, SystemCheck>(
        "SELECT * FROM system_checks ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await
}

// ─── Installation Logs ───

/// Insert a new installation log
pub async fn insert_installation_log(
    pool: &SqlitePool,
    game_id: Option<i64>,
    status: &str,
) -> Result<i64, sqlx::Error> {
    let started_at = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        "INSERT INTO installation_logs (game_id, started_at, status) VALUES (?, ?, ?)"
    )
    .bind(game_id)
    .bind(&started_at)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Update an installation log
pub async fn update_installation_log(
    pool: &SqlitePool,
    log_id: i64,
    status: &str,
    error_code: Option<String>,
    error_message: Option<String>,
    ram_usage_peak: Option<f64>,
    install_duration_minutes: Option<i64>,
) -> Result<(), sqlx::Error> {
    let completed_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE installation_logs SET completed_at = ?, status = ?, error_code = ?, error_message = ?, ram_usage_peak = ?, install_duration_minutes = ? WHERE id = ?"
    )
    .bind(&completed_at)
    .bind(status)
    .bind(error_code)
    .bind(error_message)
    .bind(ram_usage_peak)
    .bind(install_duration_minutes)
    .bind(log_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get installation logs for a game
pub async fn get_installation_logs_for_game(pool: &SqlitePool, game_id: i64) -> Result<Vec<InstallationLog>, sqlx::Error> {
    sqlx::query_as::<_, InstallationLog>(
        "SELECT * FROM installation_logs WHERE game_id = ? ORDER BY started_at DESC"
    )
    .bind(game_id)
    .fetch_all(pool)
    .await
}

/// Get all installation logs
pub async fn get_all_installation_logs(pool: &SqlitePool) -> Result<Vec<InstallationLog>, sqlx::Error> {
    sqlx::query_as::<_, InstallationLog>(
        "SELECT * FROM installation_logs ORDER BY started_at DESC"
    )
    .fetch_all(pool)
    .await
}

// ─── Community Ratings ───

/// Insert a community rating
pub async fn insert_community_rating(
    pool: &SqlitePool,
    game_id: i64,
    install_difficulty: Option<i64>,
    install_success: Option<bool>,
    issues_encountered: Option<String>,
) -> Result<i64, sqlx::Error> {
    let created_at = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        "INSERT INTO community_ratings (game_id, install_difficulty, install_success, issues_encountered, created_at) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(game_id)
    .bind(install_difficulty)
    .bind(install_success)
    .bind(issues_encountered)
    .bind(&created_at)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

/// Get community ratings for a game
pub async fn get_community_ratings_for_game(pool: &SqlitePool, game_id: i64) -> Result<Vec<CommunityRating>, sqlx::Error> {
    sqlx::query_as::<_, CommunityRating>(
        "SELECT * FROM community_ratings WHERE game_id = ? ORDER BY created_at DESC"
    )
    .bind(game_id)
    .fetch_all(pool)
    .await
}

/// Get average rating stats for a game
#[derive(Debug, Clone, Serialize)]
pub struct GameRatingStats {
    pub total_ratings: i64,
    pub avg_difficulty: Option<f64>,
    pub success_rate: Option<f64>,
}

pub async fn get_game_rating_stats(pool: &SqlitePool, game_id: i64) -> Result<GameRatingStats, sqlx::Error> {
    let row: Option<(i64, Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT
            COUNT(*) as total,
            AVG(install_difficulty) as avg_diff,
            AVG(CASE WHEN install_success THEN 1.0 ELSE 0.0 END) as success_rate
         FROM community_ratings
         WHERE game_id = ?"
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await?;

    let (total, avg_diff, success_rate) = row.unwrap_or((0, None, None));

    Ok(GameRatingStats {
        total_ratings: total,
        avg_difficulty: avg_diff,
        success_rate: success_rate,
    })
}

// ─── Game Requirements ───

/// Insert or update game requirements
pub async fn upsert_game_requirements(
    pool: &SqlitePool,
    game_id: i64,
    min_ram_gb: Option<i64>,
    rec_ram_gb: Option<i64>,
    min_cpu: Option<String>,
    rec_cpu: Option<String>,
    min_gpu: Option<String>,
    rec_gpu: Option<String>,
    disk_space_gb: Option<i64>,
    requires_directx: Option<String>,
    requires_dotnet: Option<String>,
    requires_vcredist: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO game_requirements (game_id, min_ram_gb, rec_ram_gb, min_cpu, rec_cpu, min_gpu, rec_gpu, disk_space_gb, requires_directx, requires_dotnet, requires_vcredist)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(game_id) DO UPDATE SET
            min_ram_gb = excluded.min_ram_gb,
            rec_ram_gb = excluded.rec_ram_gb,
            min_cpu = excluded.min_cpu,
            rec_cpu = excluded.rec_cpu,
            min_gpu = excluded.min_gpu,
            rec_gpu = excluded.rec_gpu,
            disk_space_gb = excluded.disk_space_gb,
            requires_directx = excluded.requires_directx,
            requires_dotnet = excluded.requires_dotnet,
            requires_vcredist = excluded.requires_vcredist"
    )
    .bind(game_id)
    .bind(min_ram_gb)
    .bind(rec_ram_gb)
    .bind(min_cpu)
    .bind(rec_cpu)
    .bind(min_gpu)
    .bind(rec_gpu)
    .bind(disk_space_gb)
    .bind(requires_directx)
    .bind(requires_dotnet)
    .bind(requires_vcredist)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get game requirements
pub async fn get_game_requirements(pool: &SqlitePool, game_id: i64) -> Result<Option<GameRequirement>, sqlx::Error> {
    sqlx::query_as::<_, GameRequirement>(
        "SELECT * FROM game_requirements WHERE game_id = ?"
    )
    .bind(game_id)
    .fetch_optional(pool)
    .await
}
