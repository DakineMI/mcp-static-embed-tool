# Authentication

This document explains the API key authentication implementation for the Static Embedding Tool server, designed for OpenAI compatibility.

## How authentication works

1. **Middleware integration**
   - Axum middleware for HTTP embedding endpoints
   - Proper 401 responses with error details
   - Health and metadata endpoints bypass authentication

2. **API Key Validation**
   - Extracts API key from Authorization header (Bearer <api_key> or direct embed- prefix)
   - Validates against stored hashed keys in SQLite database
   - Updates last_used timestamp on successful validation
   - Supports self-registration for new API keys
   - Enforces per-key rate limiting

3. **OpenAI Compatibility**
   - Uses standard Authorization: Bearer <api_key> header
   - API endpoints match OpenAI embedding API structure
   - Returns standard error formats for invalid/missing keys

## API Key Management

### Self-Registration
Clients can register for an API key via the `/api/register` endpoint:

**Request:**
```json
POST /api/register
Content-Type: application/json

{
  "client_name": "My Application",
  "description": "Description of your application",
  "email": "contact@example.com"
}
```

**Response:**
```json
{
  "api_key": "embed-ABC123def456GHI789jkl012MNO345pqr678STU901vwx234YZA567bcd890efGHI",
  "key_info": {
    "id": "uuid-here",
    "client_name": "My Application",
    "created_at": 1699123456,
    "last_used": null,
    "rate_limit_tier": "standard",
    "max_requests_per_minute": 1000,
    "active": true,
    "description": "Description of your application"
  }
}
```

**Note:** The full API key is only returned once during registration. Store it securely.

### Listing API Keys
Authenticated users can list their API keys:

```bash
curl -H "Authorization: Bearer <your-api-key>" http://localhost:8080/api/list
```

### Revoking API Keys
Revoke a specific API key:

```json
POST /api/revoke
Content-Type: application/json
Authorization: Bearer <your-api-key>

{
  "key_id": "uuid-to-revoke"
}
```

## Configuration options

The Static Embedding Tool server supports authentication configuration via command-line arguments, configuration files, or environment variables:

### Disabling authentication

To disable authentication completely (useful for development):

- **CLI argument**: `--auth-disabled`
- **Environment Variable**: `EMBED_TOOL_AUTH_DISABLED`
- **Config file**: `auth.require_auth = false`
- **Default**: `false`

**Example:**
```bash
# Command-line
embed-tool server start --auth-disabled

# Environment variable
export EMBED_TOOL_AUTH_DISABLED=true
embed-tool server start
```

**Note:** When authentication is disabled, the server accepts all requests without validation. Use only for local development, never in production.

### Rate Limiting

API keys have built-in rate limiting based on tier:

- **Development/Test**: 100 requests/minute
- **Standard**: 1000 requests/minute  
- **Premium/Enterprise**: 5000 requests/minute

Global rate limiting can be configured:

- **CLI argument**: `--rate-limit-rps` (requests per second), `--rate-limit-burst`
- **Environment Variable**: `EMBED_TOOL_RATE_LIMIT_RPS`, `EMBED_TOOL_RATE_LIMIT_BURST`
- **Default**: 100 RPS, 200 burst

### Complete configuration example

```bash
# Using command-line arguments
embed-tool server start \
  --server-url "http://localhost:8080" \
  --auth-disabled=false \
  --rate-limit-rps 100 \
  --rate-limit-burst 200

# Using environment variables
export EMBED_TOOL_SERVER_URL="http://localhost:8080"
export EMBED_TOOL_AUTH_DISABLED=false
export EMBED_TOOL_RATE_LIMIT_RPS=100
export EMBED_TOOL_RATE_LIMIT_BURST=200
embed-tool server start
```

## Security considerations

1. **API Key Security**
   - Store API keys securely (never in client-side code)
   - Use HTTPS for all API requests
   - Rotate keys regularly and revoke compromised keys
   - API keys are hashed with SHA-256 before storage

2. **Rate Limiting**
   - Per-API-key rate limiting prevents abuse
   - Global rate limiting protects against DDoS
   - Monitor usage patterns for anomalies

3. **Database Security**
   - API keys stored in SQLite (`data/api_keys.db`)
   - Database file should have restricted permissions
   - Consider encrypting the database for production

4. **Production Deployment**
   - Always enable authentication in production
   - Use environment-specific API keys
   - Implement proper logging and monitoring
   - Backup the API keys database regularly
   - Consider migrating to a managed secrets service for enterprise deployments

5. **OpenAI Client Compatibility**
   - Use `openai` Python client with custom base URL:
     ```python
     from openai import OpenAI
     
     client = OpenAI(
         api_key="embed-your-key-here",
         base_url="http://localhost:8080/v1"
     )
     
     response = client.embeddings.create(
         input="Your text here",
         model="text-embedding-ada-002"
     )
     ```
   - JavaScript/Node.js: Use `openai` npm package with baseURL option
   - The server returns OpenAI-compatible JSON responses

## Error Responses

Invalid or missing API keys return standard OpenAI-style errors:

```json
{
  "error": {
    "message": "Invalid or missing API key. Include your API key in the Authorization header as 'Bearer <your-api-key>'.",
    "type": "authentication_error",
    "code": "invalid_api_key",
    "param": null,
    "status": 401
  }
}
```

## Troubleshooting

1. **401 Unauthorized**: Check that your API key is correct and active. Verify the Authorization header format.

2. **Rate limit exceeded**: Your API key has exceeded its request limit. Wait and retry, or request a higher tier.

3. **Database errors**: Ensure the `data/` directory is writable and the SQLite database isn't corrupted.

4. **Self-registration issues**: The `/api/register` endpoint has no rate limiting. Consider adding CAPTCHA or manual approval for production.