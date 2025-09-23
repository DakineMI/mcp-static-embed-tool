#!/bin/bash

echo "Testing API Key Authentication Integration"
echo "========================================"

# Build the project
echo "Building project..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "Build failed!"
    exit 1
fi

echo "Build successful!"

# Start server in background
echo "Starting server in background..."
./target/release/static-embedding-server server start --port 8080 --bind 127.0.0.1 &
SERVER_PID=$!

# Wait for server to start
sleep 3

echo "Testing API endpoints..."

# Test health check (should work without API key)
echo "1. Testing health check..."
curl -s http://127.0.0.1:8080/health

# Test registration endpoint
echo -e "\n2. Testing API key registration..."
RESPONSE=$(curl -s -X POST http://127.0.0.1:8080/api/register \
  -H "Content-Type: application/json" \
  -d '{"name": "test-app"}')
echo $RESPONSE

# Extract API key from response
API_KEY=$(echo $RESPONSE | grep -o '"key":"[^"]*"' | cut -d'"' -f4)
echo "Generated API key: $API_KEY"

# Test embeddings endpoint without API key (should fail)
echo -e "\n3. Testing embeddings without API key (should fail)..."
curl -s -X POST http://127.0.0.1:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{"input": ["hello world"], "model": "potion-32M"}'

# Test embeddings endpoint with API key (should work)
echo -e "\n4. Testing embeddings with API key (should work)..."
curl -s -X POST http://127.0.0.1:8080/v1/embeddings \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{"input": ["hello world"], "model": "potion-32M"}'

# Test models endpoint with API key
echo -e "\n5. Testing models endpoint..."
curl -s http://127.0.0.1:8080/v1/models \
  -H "Authorization: Bearer $API_KEY"

# Clean up
echo -e "\nCleaning up..."
kill $SERVER_PID
wait $SERVER_PID 2>/dev/null

echo "Test complete!"