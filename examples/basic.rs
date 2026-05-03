use auralog::{Auralog, AuralogConfig};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Auralog::init(
        AuralogConfig::builder()
            .api_key(std::env::var("AURALOG_API_KEY").unwrap_or_else(|_| "aura_your_key".into()))
            .environment("production")
            .build()?,
    )?;

    client.info("user signed in", json!({ "user_id": "123" }));
    client.error("payment failed", json!({ "order_id": "abc" }));
    client.shutdown();

    Ok(())
}
