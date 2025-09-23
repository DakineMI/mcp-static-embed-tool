# Authentication

This document explains the current token validation implementation for the Static Embedding Tool server and what you need to do to achieve full validation including all claims (audience, expiration, issued at, subject).

## How authentication works

1. **Middleware integration**
   - Axum middleware for HTTP embedding endpoints
   - Proper 401 responses with WWW-Authenticate headers
   - Health and metadata endpoints bypass authentication

2. **JWE Token header validation**
   - Validates the 5-part structure (header.encrypted_key.iv.ciphertext.tag)
   - Checks algorithm (`dir`), encryption (`A256GCM`), and issuer
   - Validates the header structure and issuer against configured values

3. **JWT Token full validation**
   - Validates 3-part structure (header.payload.signature)
   - Validates issuer, audience, expiration, and issued at claims
   - Supports RSA and EC algorithms

4. **JWE Token full validation**
   - We can only validate the header structure without the decryption key
   - The actual claims (audience, expiration, issued at, subject) are encrypted
   - When a decryption key is provided we decrypt the token and validate the claims
   - Full validation requires the decryption key from the authentication provider
   
## How to perform full token validation

1. **JWE Token Validation**

JWE tokens are validated by checking the header structure and issuer. The server validates that the token uses the expected algorithm ("dir") and encryption method ("A256GCM") and that the issuer matches the expected value.

## Configuration options

The Static Embedding Tool server supports various authentication configuration options that can be specified via command-line arguments, configuration files, or environment variables:

### Server URL

Specify the local server URL for authentication callback:

- **CLI argument**: `--server-url`
- **Environment Variable**: `EMBED_TOOL_SERVER_URL`
- **Config file**: `server.url`
- **Default**: `http://localhost:8080`

**Example:**
```bash
# Command-line
embed-tool server start --server-url "http://localhost:8080"

# Environment variable
export EMBED_TOOL_SERVER_URL="http://localhost:8080"
embed-tool server start
```

### Authentication server

Specify the authentication server URL for JWKS endpoint:

- **CLI argument**: `--auth-server`
- **Environment Variable**: `EMBED_TOOL_AUTH_SERVER`
- **Config file**: `auth.server_url`
- **Default**: `https://auth.example.com`

**Example:**

```bash
# Command-line
embed-tool server start --auth-server "https://auth.example.com"

# Environment variable
export EMBED_TOOL_AUTH_SERVER="https://auth.example.com"
embed-tool server start
```

### Authentication audience

Specify the audience for embedding API authentication tokens:

- **CLI argument**: `--auth-audience`
- **Environment Variable**: `EMBED_TOOL_AUTH_AUDIENCE`
- **Config file**: `auth.audience`
- **Default**: `https://embed.example.com`
- **Default**: `embedding-api`

**Example:**

```bash
# Command-line
embed-tool server start --auth-audience "embedding-api"

# Environment variable
export EMBED_TOOL_AUTH_AUDIENCE="embedding-api"
embed-tool server start
```

### JWKS URL

Specify the JSON Web Key Set endpoint for token validation:

- **CLI argument**: `--jwks-url`
- **Environment Variable**: `EMBED_TOOL_JWKS_URL`
- **Config file**: `auth.jwks_url`

**Example:**

```bash
# Command-line
embed-tool server start --jwks-url "https://auth.example.com/.well-known/jwks.json"

# Environment variable  
export EMBED_TOOL_JWKS_URL="https://auth.example.com/.well-known/jwks.json"
embed-tool server start
```

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

### Complete configuration example

Here's an example of using multiple authentication options together:

```bash
# Using command-line arguments
embed-tool server start \
  --server-url "http://localhost:8080" \
  --auth-server "https://auth.example.com" \
  --auth-audience "embedding-api" \
  --jwks-url "https://auth.example.com/.well-known/jwks.json"

# Using environment variables
export EMBED_TOOL_SERVER_URL="http://localhost:8080"
export EMBED_TOOL_AUTH_SERVER="https://auth.example.com"
export EMBED_TOOL_AUTH_AUDIENCE="embedding-api"
export EMBED_TOOL_JWKS_URL="https://auth.example.com/.well-known/jwks.json"
embed-tool server start
```

**Note:** When authentication is disabled (`--auth-disabled` or `EMBED_TOOL_AUTH_DISABLED=true`), the server will not validate any tokens and will accept all requests. This is useful for local development but should never be used in production.

## Security considerations

1. **Token validation**
   - All embedding endpoints validate bearer tokens when authentication is enabled
   - Health check and metadata endpoints bypass authentication
   - Invalid or missing tokens return 401 Unauthorized

2. **JWKS endpoint security**
   - Ensure JWKS endpoint is accessible and secure
   - Consider implementing JWKS caching and rotation
   - Monitor JWKS endpoint availability

3. **Audience validation**
   - Ensure the audience matches your embedding API's expected audience
   - Prevents token reuse across different services
   - Configure audience consistently across all clients

4. **Rate limiting integration**
   - Authentication works alongside IP-based rate limiting
   - Consider implementing per-token rate limiting for production
   - Monitor authentication failures and potential abuse

5. **Production deployment**
   - Always enable authentication in production environments
   - Use strong, unique audiences for different deployments
   - Implement proper logging and monitoring for authentication events
   - Consider implementing token refresh mechanisms