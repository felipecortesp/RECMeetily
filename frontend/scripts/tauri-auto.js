#!/usr/bin/env node
/**
 * Run Tauri dev/build on macOS (Apple Silicon) with CoreML acceleration.
 */

const { execSync } = require('child_process');

const command = process.argv[2];
if (!command || !['dev', 'build'].includes(command)) {
  console.error('Usage: node tauri-auto.js [dev|build] [extra tauri args]');
  process.exit(1);
}

const extraArgs = process.argv.slice(3);
const feature = process.env.TAURI_GPU_FEATURE || 'coreml';

const args = ['tauri', command, ...extraArgs, '--', '--features', feature];
console.log(`🚀 Running: ${args.join(' ')}`);
console.log('');

try {
  execSync(args.join(' '), { stdio: 'inherit' });
} catch (err) {
  process.exit(err.status || 1);
}
