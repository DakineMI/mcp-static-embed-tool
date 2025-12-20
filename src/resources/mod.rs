//! Model Context Protocol (MCP) resources for the embedding server.
//!
//! This module implements MCP resource endpoints that provide static and dynamic
//! documentation, configuration, and metadata through the MCP interface.
//!
//! ## Resource Types
//!
//! - **Static**: Built-in documentation, schemas, and help text
//! - **Dynamic**: Runtime configuration, model lists, metrics
//! - **Template**: Parameterized content with variable substitution
//!
//! ## Available Resources
//!
//! - `docs://api` - OpenAI-compatible API documentation
//! - `docs://mcp` - MCP interface documentation
//! - `config://current` - Active server configuration
//! - `models://list` - Available models with metadata
//! - `metrics://server` - Server performance metrics
//!
//! ## Resource Provider Pattern
//!
//! Resources are implemented via the `ResourceProvider` trait which defines:
//! - URI (unique identifier)
//! - Name and description
//! - MIME type
//! - Content generation
//!
//! ## Examples
//!
//! ```json
//! // MCP resource request
//! {
//!   "uri": "docs://api",
//!   "format": "text/markdown"
//! }
//! ```

#[cfg(feature = "mcp")]
use rmcp::model::{Annotated, RawResource, ReadResourceResult, Resource, ResourceContents};

/// Trait for MCP resource providers.
///
/// Implementors provide static or dynamic content accessible through the MCP
/// resource system with defined URIs, metadata, and MIME types.
pub trait ResourceProvider {
    /// Get the resource URI (e.g., "docs://api").
    fn uri(&self) -> &'static str;

    /// Get the human-readable resource name.
    fn name(&self) -> &'static str;

    /// Get the resource description for discovery.
    fn description(&self) -> &'static str;

    /// Get the resource MIME type (e.g., "text/markdown").
    fn mime_type(&self) -> &'static str;

    /// Generate the resource content.
    ///
    /// This may be static text or dynamically generated based on server state.
    fn content(&self) -> String;

    /// Get additional resource metadata (optional).
    fn metadata(&self) -> Option<serde_json::Value> {
        None
    }

    /// Get the resource metadata
    fn meta(&self) -> Resource {
        let size = self.content().len() as u32;
        let raw = RawResource {
            size: Some(size),
            uri: self.uri().to_string(),
            name: self.name().to_string(),
            mime_type: Some(self.mime_type().to_string()),
            description: Some(self.description().to_string()),
            icons: Some(vec![]),
            title: Some(self.name().to_string()),
        };
        Annotated::new(raw, None)
    }

    fn read(&self) -> ReadResourceResult {
        ReadResourceResult {
            contents: vec![ResourceContents::text(self.content(), self.uri())],
        }
    }
}

// Instructions resource
pub struct InstructionsResource;

impl ResourceProvider for InstructionsResource {
    fn uri(&self) -> &'static str {
        "embedtool://instructions"
    }

    fn name(&self) -> &'static str {
        "Static Embedding Tool Instructions"
    }

    fn mime_type(&self) -> &'static str {
        "text/markdown"
    }

    fn description(&self) -> &'static str {
        "Full instructions and guidelines for the Static Embedding Tool MCP server"
    }

    fn content(&self) -> String {
        // Prefer the repository's `copilot-instructions.md` under `.github`.
        // Fall back to a crate-local `instructions.md` if present.
        // Using `include_str!` ensures the content is compiled in and available
        // at runtime without requiring file I/O.
        include_str!("../../.github/copilot-instructions.md").to_string()
    }
}

/// Registry of all available resources
pub struct ResourceRegistry;

impl ResourceRegistry {
    /// Get all available resource providers
    pub fn get_providers() -> Vec<Box<dyn ResourceProvider>> {
        vec![Box::new(InstructionsResource)]
    }

    /// Find a resource provider by URI
    pub fn find_by_uri(uri: &str) -> Option<Box<dyn ResourceProvider>> {
        Self::get_providers().into_iter().find(|p| p.uri() == uri)
    }
}

/// List all available resources
pub fn list_resources() -> Vec<Resource> {
    ResourceRegistry::get_providers()
        .into_iter()
        .map(|p| p.meta())
        .collect()
}

pub fn read_resource(uri: &str) -> Option<ReadResourceResult> {
    ResourceRegistry::find_by_uri(uri).map(|provider| provider.read())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instructions_resource_metadata() {
        let resource = InstructionsResource;

        assert_eq!(resource.uri(), "embedtool://instructions");
        assert_eq!(resource.name(), "Static Embedding Tool Instructions");
        assert_eq!(resource.mime_type(), "text/markdown");
        assert_eq!(resource.description(), "Full instructions and guidelines for the Static Embedding Tool MCP server");

        // Content should not be empty
        let content = resource.content();
        assert!(!content.is_empty());
        assert!(content.contains("#")); // Should contain markdown headers
    }

    #[test]
    fn test_instructions_resource_meta() {
        let resource = InstructionsResource;
        let meta = resource.meta();
        
        assert_eq!(meta.raw.uri, "embedtool://instructions");
        assert_eq!(meta.raw.name, "Static Embedding Tool Instructions");
        assert_eq!(meta.raw.mime_type, Some("text/markdown".to_string()));
        assert_eq!(meta.raw.description, Some("Full instructions and guidelines for the Static Embedding Tool MCP server".to_string()));
        assert!(meta.raw.size.is_some());
        assert!(meta.raw.size.unwrap() > 0);
    }    #[test]
    fn test_instructions_resource_read() {
        let resource = InstructionsResource;
        let result = resource.read();

        assert_eq!(result.contents.len(), 1);
        // Since we can't easily pattern match on the ResourceContents enum
        // without knowing its exact structure, we'll just verify the result is created
        assert!(!result.contents.is_empty());
    }

    #[test]
    fn test_resource_registry_get_providers() {
        let providers = ResourceRegistry::get_providers();
        assert_eq!(providers.len(), 1);

        // Should contain InstructionsResource
        let provider = &providers[0];
        assert_eq!(provider.uri(), "embedtool://instructions");
    }

    #[test]
    fn test_resource_registry_find_by_uri() {
        // Found
        let provider = ResourceRegistry::find_by_uri("embedtool://instructions");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().uri(), "embedtool://instructions");

        // Not found
        let not_found = ResourceRegistry::find_by_uri("nonexistent://uri");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_list_resources() {
        let resources = list_resources();
        assert_eq!(resources.len(), 1);
        
        let resource = &resources[0];
        assert_eq!(resource.raw.uri, "embedtool://instructions");
        assert_eq!(resource.raw.name, "Static Embedding Tool Instructions");
    }    #[test]
    fn test_read_resource() {
        // Found
        let result = read_resource("embedtool://instructions");
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.contents.len(), 1);

        // Not found
        let not_found = read_resource("nonexistent://uri");
        assert!(not_found.is_none());
    }
}
