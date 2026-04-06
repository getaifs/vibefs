# Research: macOS FSKit + Rust Bindings (April 2026)

## Context
VibeFS v0.2 deferred FSKit due to lack of stable Rust bindings. This document
reassesses the landscape as of April 2026.

## TL;DR
The situation has improved significantly. `fskit-rs` (crates.io, Nov 2025) +
`FSKitBridge` provide a working Rust→FSKit bridge. macFUSE 5 also now uses
FSKit natively on macOS 26. Our NFS approach remains viable.

---

## 1. fskit-rs + FSKitBridge

- **Crate:** https://crates.io/crates/fskit-rs
- **Bridge:** https://github.com/debox-network/FSKitBridge
- **Architecture:** Swift appex handles FSKit/XPC ↔ Protobuf over TCP localhost ↔ Rust backend
- **Rust API:** Implement `fskit_rs::Filesystem` trait (lookup, read, write, readdir, etc.)
- **No Swift expertise needed** beyond bundling the .appex shim

### How it maps to VibeFS
Our `NFSFileSystem` trait implementation in `src/nfs/mod.rs` would map almost
directly to the `fskit_rs::Filesystem` trait. The read/write cascade logic
(session delta → repo → git ODB) stays the same — only the transport layer changes.

## 2. macFUSE 5 (FSKit backend on macOS 26)

- macFUSE 5 supports macOS 12–26
- On macOS 26 (Tahoe): uses **native FSKit backend** — no kext, no recovery mode
- Existing FUSE code works unmodified
- Makes `fuser` crate viable again on modern macOS

## 3. FUSE-T (kext-less FUSE via NFS/FSKit)

- https://www.fuse-t.org/
- Translates FUSE protocol to NFS v4 / SMB / FSKit
- No kernel extension on any macOS version
- Drop-in macFUSE replacement
- On macOS 26+: native FSKit backend

## 4. Prior Art: agent-harbor (blocksense-network)

- https://github.com/blocksense-network/agent-harbor
- Agent isolation platform with FSKit + Rust implementation
- Uses Rust FsCore → C FFI → Swift FSKit extension
- XPC control plane for CLI management
- **Same use case as VibeFS** — validates the architecture

## 5. Current NFS Approach Assessment

Our NFSv3-over-localhost approach remains the most portable and dependency-free
option. Known trade-offs:
- (+) No external dependencies, cross-platform
- (+) No root/kext/recovery mode
- (-) xattr workarounds needed (build artifact symlinks)
- (-) No Finder/Spotlight integration
- (-) NFS mount semantics differ from local FS

## Recommendation

| Approach | Effort | Benefit |
|----------|--------|---------|
| Stay on NFS (current) | None | Working, cross-platform |
| Add FSKit via fskit-rs | Medium | Native macOS FS, Finder, no xattr hacks |
| macFUSE 5 / FUSE-T | Low-Med | Richer FS API, FSKit under the hood on 26 |

**Suggested path:** Keep NFS as default/Linux backend. Add FSKit support on macOS
behind a feature flag using `fskit-rs`. The `Filesystem` trait maps closely enough
to our NFS impl that we could share the core read/write cascade logic.

## Key Links
- fskit-rs crate: https://crates.io/crates/fskit-rs
- FSKitBridge: https://github.com/debox-network/FSKitBridge
- macFUSE releases: https://github.com/macfuse/macfuse/releases
- FUSE-T: https://www.fuse-t.org/
- agent-harbor (prior art): https://github.com/blocksense-network/agent-harbor
- Apple FSKit docs: https://developer.apple.com/documentation/FSKit
