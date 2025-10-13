use std::sync::Arc;
use static_embedding_server::server::api_keys::{ApiKeyManager, ApiKeyRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== API Key Authentication Demo ===");

    // Create API key manager
    let manager = Arc::new(ApiKeyManager::new("./demo_api_keys.db")?);

    // Register a new API key
    println!("1. Registering new API key...");
    let request = ApiKeyRequest {
        client_name: "demo-app".to_string(),
        description: Some("Demo application for testing".to_string()),
        email: Some("demo@example.com".to_string()),
    };
    let api_key = manager.generate_api_key(request).await?;
    println!("   Generated API key: {}", api_key.api_key);
    println!("   Key ID: {}", api_key.key_info.id);

    // Validate the API key
    println!("\n2. Validating API key...");
    match manager.validate_api_key(&api_key.api_key).await {
        Some(key_info) => {
            println!("   ✓ Valid API key for: {}", key_info.client_name);
        }
        None => println!("   ✗ Invalid API key"),
    }

    // List all keys
    println!("\n3. Listing all API keys...");
    let keys = manager.list_api_keys().await;
    for key in keys {
        println!("   - {} ({}): {}...", key.client_name, key.id, &api_key.api_key[..12]);
    }

    // Test invalid key
    println!("\n4. Testing invalid API key...");
    match manager.validate_api_key("embed-invalid-key-12345").await {
        Some(_) => println!("   ✗ Should have been invalid"),
        None => println!("   ✓ Correctly identified as invalid"),
    }

    // Revoke the key
    println!("\n5. Revoking API key...");
    let revoked = manager.revoke_api_key(&api_key.key_info.id).await;
    if revoked {
        println!("   ✓ API key revoked");
    } else {
        println!("   ✗ Failed to revoke API key");
    }

    // Try to validate revoked key
    println!("\n6. Testing revoked API key...");
    match manager.validate_api_key(&api_key.api_key).await {
        Some(_) => println!("   ✗ Revoked key should be invalid"),
        None => println!("   ✓ Revoked key correctly invalid"),
    }

    println!("\n=== Demo Complete ===");
    Ok(())
}