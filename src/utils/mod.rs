use std::process::Command;

/// Generate a unique connection ID
pub fn generate_connection_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let random = rand::random::<u32>();
    format!("conn_{timestamp:x}_{random:x}")
}

/// Format duration in a human-readable way
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    if total_secs == 0 {
        format!("{millis}ms")
    } else if total_secs < 60 {
        format!("{total_secs}.{millis:03}s")
    } else if total_secs < 3600 {
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{minutes}m {seconds}s")
    } else {
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        format!("{hours}h {minutes}m {seconds}s")
    }
}

/// Distill a model using Model2Vec and PCA
///
/// This function distills a model by reducing its dimensions using PCA.
/// The distilled model is saved to the specified output directory.
///
/// # Arguments
/// * `model_name` - The name of the model to distill
/// * `pca_dims` - The number of dimensions to reduce to
/// * `output_dir` - The directory to save the distilled model
///
/// # Example
/// ```
/// use static_embed_tool::utils;
/// utils::distill("my_model", 128, "./output");
/// ```
/// # Panics
pub fn distill(
    model_name: &str,
    pca_dims: usize,
    output_path: Option<std::path::PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    use std::env;
    use std::fs;

    let output = output_path.unwrap_or_else(|| {
        // Only create default path if no path was provided
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        
        PathBuf::from(home).join("ai/models/model2vec").join(model_name)
    });

    // Create parent directories if they don't exist
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    // Auto-version if file already exists to avoid overwriting
    let final_output = if output.exists() {
        let file_stem = output.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model");
        let extension = output.extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{}", s))
            .unwrap_or_default();
        
        let parent = output.parent().unwrap_or_else(|| std::path::Path::new("."));
        
        // Find the next available version number
        let mut version = 2;
        let versioned_path = loop {
            let candidate = parent.join(format!("{}_v{}{}", file_stem, version, extension));
            if !candidate.exists() {
                break candidate;
            }
            version += 1;
            
            // Safety check to prevent infinite loop
            if version > 9999 {
                return Err("Too many versions of this model exist (>9999)".into());
            }
        };
        
        println!("⚠️  File exists, saving as: {}", versioned_path.display());
        versioned_path
    } else {
        output
    };

    // Distill the model using PCA to reduce dimensions via command line
    use std::process::Command;
    
    println!("Distilling model '{}' with {} PCA dimensions...", model_name, pca_dims);
    
    let mut cmd = Command::new("model2vec");
    cmd.arg("distill")
        .arg(model_name)
        .arg(pca_dims.to_string());
    
    // Check if model2vec is available
    let output = cmd.output()
        .map_err(|e| format!("Failed to execute model2vec command. Is model2vec installed? Error: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "model2vec distillation failed with exit code {:?}\nStderr: {}\nStdout: {}", 
            output.status.code(), 
            stderr.trim(), 
            stdout.trim()
        ).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        println!("model2vec output: {}", stdout.trim());
    }

    println!("✓ Model distilled successfully to: {}", final_output.display());
    Ok(())
}

pub fn calculate_total(numbers: &[i32]) -> i32 {
    numbers.iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_list() {
        assert_eq!(calculate_total(&[]), 0);
    }

    #[test]
    fn test_single_positive() {
        assert_eq!(calculate_total(&[5]), 5);
    }

    #[test]
    fn test_single_negative() {
        assert_eq!(calculate_total(&[-3]), -3);
    }

    #[test]
    fn test_multiple_numbers() {
        assert_eq!(calculate_total(&[1, 2, 3, 4]), 10);
    }

    #[test]
    fn test_with_zeros() {
        assert_eq!(calculate_total(&[0, 0, 5]), 5);
    }
}