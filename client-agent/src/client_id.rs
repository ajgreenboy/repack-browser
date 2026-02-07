use uuid::Uuid;

pub fn generate_or_load_client_id(config: &mut crate::config::Config) -> String {
    // If config already has an ID, use it
    if !config.client.id.is_empty() {
        return config.client.id.clone();
    }

    // Generate new UUID
    let id = Uuid::new_v4().to_string();
    config.client.id = id.clone();

    // Save config with new ID
    if let Err(e) = config.save() {
        eprintln!("Warning: Failed to save client ID: {}", e);
    }

    id
}
