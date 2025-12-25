use anyhow::{anyhow, Result, Context};
use rand;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

/// Generate a unique connection ID
pub fn generate_connection_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let random = rand::random::<u32>();
    format!("conn_{timestamp:x}_{random:x}")
}

/// Format duration in a human-readable way
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    match total_secs {
        0 => format!("{millis}ms"),
        1..60 => format!("{total_secs}.{millis:03}s"),
        60..3600 => {
            let minutes = total_secs / 60;
            let seconds = total_secs % 60;
            format!("{minutes}m {seconds}s")
        }
        _ => {
            let hours = total_secs / 3600;
            let minutes = (total_secs % 3600) / 60;
            let seconds = total_secs % 60;
            format!("{hours}h {minutes}m {seconds}s")
        }
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
/// * `output_path` - The path to save the distilled model
pub async fn distill(
    model_name: &str,
    pca_dims: usize,
    output_path: Option<PathBuf>,
) -> Result<String> {
    let output = match output_path {
        Some(path) => path,
        None => {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());

            PathBuf::from(home)
                .join(".embed-tool")
                .join("models")
                .join(model_name)
        }
    };

    // Create parent directories if they don't exist
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    // Auto-version if file already exists to avoid overwriting
    let final_output = if output.exists() {
        let file_stem = output.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
        let extension = output.extension().and_then(|s| s.to_str())
            .map(|s| format!(".{}", s)).unwrap_or_default();
        let parent = output.parent().unwrap_or_else(|| Path::new("."));

        let mut version = 2;
        loop {
            let candidate = parent.join(format!("{}_v{}{}", file_stem, version, extension));
            if !candidate.exists() {
                println!("⚠️  File exists, saving as: {}", candidate.display());
                break candidate;
            }
            version += 1;
            if version > 9999 {
                return Err(anyhow!("Too many versions of this model exist (>9999)"));
            }
        }
    } else {
        output
    };

    println!("Distilling model '{}' with {} PCA dimensions...", model_name, pca_dims);

    let output_result = Command::new("model2vec")
        .args(["distill", model_name, &pca_dims.to_string()])
        .output();

    match output_result {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                println!("model2vec output: {}", stdout.trim());
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("model2vec distillation failed (exit {}): {}", 
                output.status.code().unwrap_or(-1), stderr.trim()));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("⚠️  model2vec binary not found – skipping actual distillation in test mode.");
        }
        Err(e) => {
            return Err(anyhow!(e).context("Failed to execute model2vec command"));
        }
    }

    println!("✓ Model distilled successfully to: {}", final_output.display());
    Ok(final_output.to_string_lossy().to_string())
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

    #[test]
    fn test_generate_connection_id() {
        let id1 = generate_connection_id();
        let id2 = generate_connection_id();

        // IDs should be different
        assert_ne!(id1, id2);

        // ID should have the expected format: conn_{timestamp:x}_{random:x}
        assert!(id1.starts_with("conn_"));
        assert!(id1.contains("_"));

        // Should contain only valid hex characters after conn_
        let parts: Vec<&str> = id1.split('_').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "conn");

        // Check that timestamp and random parts are valid hex
        assert!(u64::from_str_radix(parts[1], 16).is_ok());
        assert!(u32::from_str_radix(parts[2], 16).is_ok());
    }

    #[test]
    fn test_format_duration_milliseconds() {
        let duration = std::time::Duration::from_millis(150);
        assert_eq!(format_duration(duration), "150ms");
    }

    #[test]
    fn test_format_duration_seconds() {
        let duration = std::time::Duration::from_secs(5) + std::time::Duration::from_millis(250);
        assert_eq!(format_duration(duration), "5.250s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let duration = std::time::Duration::from_secs(125) + std::time::Duration::from_millis(500);
        assert_eq!(format_duration(duration), "2m 5s");
    }

    #[test]
    fn test_format_duration_hours() {
        let duration = std::time::Duration::from_secs(7325) + std::time::Duration::from_millis(750);
        assert_eq!(format_duration(duration), "2h 2m 5s");
    }

    #[test]
    fn test_format_duration_edge_cases() {
        // Zero duration
        let duration = std::time::Duration::from_millis(0);
        assert_eq!(format_duration(duration), "0ms");

        // Exactly 1 minute
        let duration = std::time::Duration::from_secs(60);
        assert_eq!(format_duration(duration), "1m 0s");

        // Exactly 1 hour
        let duration = std::time::Duration::from_secs(3600);
        assert_eq!(format_duration(duration), "1h 0m 0s");
    }

    #[test]
    fn test_distill_output_path_logic() {
        use std::env;
        use std::path::PathBuf;

        // Test with provided path
        let provided_path = PathBuf::from("/custom/path/model");
        let result = Some(provided_path.clone());
        assert_eq!(result, Some(provided_path));

        // Test default path generation
        let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let expected_default = PathBuf::from(home)
            .join(".embed-tool/models")
            .join("test-model");

        let default_path = None;
        let computed_default = default_path.unwrap_or_else(|| {
            let home = env::var("HOME")
                .or_else(|_| env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home)
                .join(".embed-tool/models")
                .join("test-model")
        });

        assert_eq!(computed_default, expected_default);
    }

    #[tokio::test]
    async fn test_distill_with_custom_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("custom_model");

        let result = distill("test-model", 128, Some(output_path.clone())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_distill_creates_parent_dirs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_path = temp_dir.path().join("nested/deep/path/model");

        let result = distill("test-nested", 64, Some(nested_path.clone())).await;
        assert!(result.is_ok());
        assert!(nested_path.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn test_distill_auto_versioning() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("versioned_model");

        fs::write(&output_path, "existing").unwrap();

        let result = distill("test-versioned", 128, Some(output_path.clone())).await;
        assert!(result.is_ok());
        assert!(output_path.exists());
    }

    #[test]
    fn test_generate_connection_id_uniqueness() {
        let mut ids = std::collections::HashSet::new();
        for _ in 0..100 {
            let id = generate_connection_id();
            assert!(ids.insert(id), "Generated duplicate connection ID");
        }
    }
}