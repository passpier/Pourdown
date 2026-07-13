#!/usr/bin/env node
// Sync one version string into every place Pourdown tracks it.
// Usage: node scripts/set-version.mjs 0.6.0
// The release workflow calls this from the pushed git tag (the tag is the
// single source of truth); you can also run it locally to bump versions.
import { readFileSync, writeFileSync } from 'node:fs';

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`Invalid version: ${JSON.stringify(version)} (expected numeric x.y.z, e.g. 0.6.0)`);
  process.exit(1);
}

// package.json
const pkgPath = 'package.json';
const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
pkg.version = version;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');

// src-tauri/tauri.conf.json
const confPath = 'src-tauri/tauri.conf.json';
const conf = JSON.parse(readFileSync(confPath, 'utf8'));
conf.version = version;
writeFileSync(confPath, JSON.stringify(conf, null, 2) + '\n');

// src-tauri/Cargo.toml — replace ONLY the [package] version line.
const cargoPath = 'src-tauri/Cargo.toml';
const cargo = readFileSync(cargoPath, 'utf8').replace(
  /^(\[package\][\s\S]*?^version\s*=\s*)"[^"]*"/m,
  `$1"${version}"`,
);
writeFileSync(cargoPath, cargo);

console.log(`Set version ${version} in package.json, tauri.conf.json, Cargo.toml`);
