#!/usr/bin/env node

// Test MCP client for static embed tool
import { spawn } from 'child_process';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

async function testMCPTools() {
  console.log('🧪 Testing MCP Static Embed Tool\n');

  // Start the MCP server
  const serverProcess = spawn('cargo', ['run', '--features', 'mcp', '--', 'server', 'start', '--mcp'], {
    cwd: path.join(__dirname, '../../static_embed_tool'),
    stdio: ['pipe', 'pipe', 'inherit']
  });

  let serverReady = false;
  const serverOutput = [];

  // Listen for server ready signal
  serverProcess.stdout.on('data', (data) => {
    const output = data.toString();
    serverOutput.push(output);
    console.log('Server:', output.trim());

    if (output.includes('MCP server running') || output.includes('started successfully')) {
      serverReady = true;
    }
  });

  serverProcess.stderr.on('data', (data) => {
    console.error('Server Error:', data.toString().trim());
  });

  // Wait for server to start
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('Server startup timeout'));
    }, 10000);

    const checkReady = () => {
      if (serverReady) {
        clearTimeout(timeout);
        resolve();
      } else {
        setTimeout(checkReady, 100);
      }
    };
    checkReady();
  });

  console.log('✅ Server started successfully\n');

  // Test MCP protocol - send list_tools request
  const listToolsRequest = {
    jsonrpc: '2.0',
    id: 1,
    method: 'tools/list',
    params: {}
  };

  console.log('📋 Testing tools/list...');
  serverProcess.stdin.write(JSON.stringify(listToolsRequest) + '\n');

  // Wait a bit for response
  await new Promise(resolve => setTimeout(resolve, 1000));

  // Test embed tool
  const embedRequest = {
    jsonrpc: '2.0',
    id: 2,
    method: 'tools/call',
    params: {
      name: 'embed',
      arguments: {
        input: 'Hello world',
        model: 'potion-32M'
      }
    }
  };

  console.log('🔍 Testing tools/call (embed)...');
  serverProcess.stdin.write(JSON.stringify(embedRequest) + '\n');

  // Wait for responses
  await new Promise(resolve => setTimeout(resolve, 2000));

  console.log('\n✅ MCP testing completed');
  serverProcess.kill();
}

testMCPTools().catch(console.error);