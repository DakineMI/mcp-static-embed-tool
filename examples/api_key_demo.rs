use std::sync::Arc;
use crate::server::api_keys::ApiKeyManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== API Key Authentication Demo ===");
    
    // Create API key manager
    let manager = Arc::new(ApiKeyManager::new());
    
    // Register a new API key
    println!("1. Registering new API key...");
    let api_key = manager.register_key("demo-app".to_string()).await?;
    println!("   Generated API key: {}", api_key.key);
    println!("   Key ID: {}", api_key.id);
    println!("   Rate limit tier: {:?}", api_key.tier);
    
    // Validate the API key
    println!("\n2. Validating API key...");
    match manager.validate_key(&api_key.key).await {
        Ok(Some(info)) => {
            println!("   ✓ Valid API key for: {}", info.name);
            println!("   Rate limit: {:?}", info.tier);
        }
        Ok(None) => println!("   ✗ Invalid API key"),
        Err(e) => println!("   ✗ Validation error: {}", e),
    }
    
    // List all keys
    println!("\n3. Listing all API keys...");
    let keys = manager.list_keys().await?;
    for key in keys {
        println!("   - {} ({}): {}", key.name, key.id, key.key[..12].to_string() + "...");
    }
    
    // Test invalid key
    println!("\n4. Testing invalid API key...");
    match manager.validate_key("embed-invalid-key-12345").await {
        Ok(Some(_)) => println!("   ✗ Should have been invalid"),
        Ok(None) => println!("   ✓ Correctly identified as invalid"),
        Err(e) => println!("   ✗ Validation error: {}", e),
    }
    
    // Revoke the key
    println!("\n5. Revoking API key...");
    manager.revoke_key(&api_key.id).await?;
    println!("   ✓ API key revoked");
    
    // Try to validate revoked key
    println!("\n6. Testing revoked API key...");
    match manager.validate_key(&api_key.key).await {
        Ok(Some(_)) => println!("   ✗ Revoked key should be invalid"),
        Ok(None) => println!("   ✓ Revoked key correctly invalid"),
        Err(e) => println!("   ✗ Validation error: {}", e),
    }
    
    println!("\n=== Demo Complete ===");
    Ok(())
}