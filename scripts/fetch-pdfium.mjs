#!/usr/bin/env node
// Fetches the PDFium dynamic library Pourdown ships, from the official
// bblanchon/pdfium-binaries releases, and places it where
// `src-tauri/src/convert/pdf.rs` and `tauri.conf.json` expect it.
//
// Why this exists: the pdfium binary used to be committed to git directly.
// `pdfium-render`'s `Pdfium::bind_to_library` resolves its *entire* known
// symbol table eagerly at load time, keyed to a specific PDFium build via
// its `pdfium_<N>` cargo feature (see the crate's Cargo.toml). A stale
// binary — even a well-formed, correctly-signed one — fails with a cryptic
// "symbol not found" / GetProcAddress error the moment `pdfium-render` is
// bumped to a version whose default feature targets a newer PDFium build
// than the committed binary. That's exactly what happened here: this
// project's `Cargo.lock` was gitignored, so CI silently re-resolved
// `pdfium-render` to a newer release at build time while the vendored
// binary stayed frozen. See CLAUDE.md's PDFium gotcha entry for the full
// incident writeup.
//
// The fix: don't commit the binary at all. Fetch a pinned, hash-verified
// release here (locally on demand, or from CI before bundling), and hard-gate
// on the binary actually exporting the symbol `pdfium-render` needs — so a
// future version mismatch fails loudly at fetch time instead of silently at
// app-launch time in a user's hands.
//
// Usage:
//   node scripts/fetch-pdfium.mjs                  # host-appropriate target(s)
//   node scripts/fetch-pdfium.mjs --target mac
//   node scripts/fetch-pdfium.mjs --target win-x64 --target win-arm64
//
// IMPORTANT: whenever `pdfium-render` is upgraded in src-tauri/Cargo.toml,
// re-check its default `pdfium_latest` feature alias (in the crate's own
// Cargo.toml) against PDFIUM_VERSION below, and re-run this script's
// `--print-hashes` mode (see bottom) to refresh PINNED_SHA256 if the version
// changes.

import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, chmodSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';

// Pinned PDFium release tag (bblanchon/pdfium-binaries), as `chromium/<N>`.
// Must export every symbol the pinned `pdfium-render` version's *default*
// feature set requires — currently `FPDFTextObj_SetFontSize`, gated behind
// pdfium-render 0.9.3's `pdfium_7881`/`pdfium_future` features. Bump this in
// lockstep with src-tauri/Cargo.toml's `pdfium-render` version, never alone.
const PDFIUM_VERSION = '7881';

// A symbol that must be present in the fetched binary for it to satisfy the
// pinned pdfium-render version (see PDFIUM_VERSION comment above). Checked
// after every extraction so an incompatible or corrupted download fails here,
// not at app-launch time.
const REQUIRED_SYMBOL = 'FPDFTextObj_SetFontSize';

// asset name -> { archiveMember: path inside the .tgz, sha256: pinned hash of
// the downloaded .tgz itself, destination: repo-relative output path }
const TARGETS = {
  mac: {
    asset: 'pdfium-mac-univ.tgz',
    sha256: 'df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3',
    archiveMember: 'lib/libpdfium.dylib',
    destination: 'src-tauri/frameworks/pdfium.framework/pdfium',
  },
  'win-x64': {
    asset: 'pdfium-win-x64.tgz',
    sha256: '73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08',
    archiveMember: 'bin/pdfium.dll',
    destination: 'src-tauri/resources/pdfium-x64.dll',
  },
  'win-arm64': {
    asset: 'pdfium-win-arm64.tgz',
    sha256: 'd3035d4d2cacac6ecd1a2ece197a3d702a1b2a58466276b9f870b8cb278a9d84',
    archiveMember: 'bin/pdfium.dll',
    destination: 'src-tauri/resources/pdfium-arm64.dll',
  },
};

function parseTargets(argv) {
  const requested = [];
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--target') {
      const t = argv[++i];
      if (!TARGETS[t]) {
        console.error(`Unknown --target ${JSON.stringify(t)}. Valid: ${Object.keys(TARGETS).join(', ')}`);
        process.exit(1);
      }
      requested.push(t);
    }
  }
  if (requested.length > 0) return requested;

  // No --target given: default to whatever matches the host OS, mirroring
  // how a contributor would build/run Pourdown locally on their own machine.
  if (process.platform === 'darwin') return ['mac'];
  if (process.platform === 'win32') {
    return [process.arch === 'arm64' ? 'win-arm64' : 'win-x64'];
  }
  console.error(
    'No --target specified and host platform has no default (linux is unsupported for PDFium bundling). ' +
    `Pass --target explicitly: ${Object.keys(TARGETS).join(', ')}`
  );
  process.exit(1);
}

function sha256(buf) {
  return createHash('sha256').update(buf).digest('hex');
}

function download(url) {
  console.log(`  downloading ${url}`);
  // Node's built-in fetch (global since Node 18); redirects are followed
  // automatically.
  return fetch(url).then(async (res) => {
    if (!res.ok) {
      throw new Error(`download failed: HTTP ${res.status} for ${url}`);
    }
    return Buffer.from(await res.arrayBuffer());
  });
}

// Minimal tar reader: good enough for the flat, non-sparse, non-PAX archives
// bblanchon publishes. Returns the raw bytes of `memberPath`, or throws.
function extractTarMember(tarBuf, memberPath) {
  let offset = 0;
  while (offset + 512 <= tarBuf.length) {
    const header = tarBuf.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break; // end-of-archive marker

    const nameRaw = header.subarray(0, 100).toString('utf8').replace(/\0.*$/, '');
    const sizeRaw = header.subarray(124, 136).toString('utf8').replace(/\0.*$/, '').trim();
    const size = parseInt(sizeRaw, 8) || 0;
    const dataStart = offset + 512;

    if (nameRaw === memberPath || nameRaw === `./${memberPath}`) {
      return tarBuf.subarray(dataStart, dataStart + size);
    }

    offset = dataStart + Math.ceil(size / 512) * 512;
  }
  throw new Error(`member ${JSON.stringify(memberPath)} not found in archive`);
}

function requireBinaryContainsSymbol(buf, symbol) {
  // A portable substring scan over the raw bytes: works for both a Mach-O
  // dylib's string table (nm-equivalent) and a PE DLL's export name table,
  // without needing platform-specific tools (nm/dumpbin) to be present on
  // whichever OS this script runs on. `bind_to_library`/`GetProcAddress`
  // resolve by literal ASCII export name, so this check has the same
  // failure mode as the real loader.
  const needle = Buffer.from(symbol, 'ascii');
  if (buf.indexOf(needle) === -1) {
    throw new Error(
      `downloaded binary does not export ${JSON.stringify(symbol)} — ` +
      `PDFIUM_VERSION=${PDFIUM_VERSION} is incompatible with the pinned pdfium-render ` +
      `version. Check pdfium-render's Cargo.toml for its current default ` +
      `pdfium_<N> feature and bump PDFIUM_VERSION (+ hashes) to match.`
    );
  }
}

async function fetchTarget(name) {
  const t = TARGETS[name];
  console.log(`[${name}] fetching chromium/${PDFIUM_VERSION} (${t.asset})`);

  const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F${PDFIUM_VERSION}/${t.asset}`;
  const archive = await download(url);

  const actualHash = sha256(archive);
  if (actualHash !== t.sha256) {
    throw new Error(
      `[${name}] SHA-256 mismatch for ${t.asset}\n` +
      `  expected: ${t.sha256}\n` +
      `  actual:   ${actualHash}\n` +
      `Refusing to use this download — either the pin is stale or the file was tampered with.`
    );
  }
  console.log(`  sha256 verified: ${actualHash}`);

  const tarBuf = zlib.gunzipSync(archive);
  const binary = extractTarMember(tarBuf, t.archiveMember);

  requireBinaryContainsSymbol(binary, REQUIRED_SYMBOL);
  console.log(`  symbol check passed: exports ${REQUIRED_SYMBOL}`);

  const destPath = t.destination;
  // The destination dir may not exist on a fresh checkout: these DLL/dylib
  // paths are gitignored (see .gitignore's PDFium comment), and git doesn't
  // track empty directories, so `src-tauri/resources/` in particular is
  // simply absent until something creates it.
  mkdirSync(path.dirname(destPath), { recursive: true });
  writeFileSync(destPath, binary);
  chmodSync(destPath, 0o755);
  console.log(`  wrote ${destPath} (${binary.length} bytes)`);

  if (name === 'mac') {
    // Re-sign to match the framework's existing ad-hoc, linker-signed
    // identity (`Identifier=libpdfium.dylib`) so an unsigned/unnotarized
    // Pourdown build still launches under Gatekeeper the same way it did
    // with the previously-committed binary.
    execFileSync('codesign', ['--force', '--sign', '-', '--identifier', 'libpdfium.dylib', destPath]);
    console.log('  re-signed (ad-hoc, identifier=libpdfium.dylib)');
  }
}

async function main() {
  const targets = parseTargets(process.argv.slice(2));
  console.log(`Fetching PDFium chromium/${PDFIUM_VERSION} for: ${targets.join(', ')}`);
  for (const name of targets) {
    await fetchTarget(name);
  }
  console.log('Done.');
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
