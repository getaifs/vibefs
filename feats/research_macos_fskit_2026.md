# Research: macOS FSKit + Rust Bindings (April 2026)

## Context
VibeFS v0.2 deferred FSKit due to lack of stable Rust bindings. This document
reassesses the landscape as of April 2026.

## TL;DR
Rust bindings for FSKit now exist (`fskit-rs` + `FSKitBridge`, Nov 2025), and
macFUSE 5 uses FSKit natively on macOS 26. However, **FSKit is a poor fit for
VibeFS** — it's designed for disk-backed local filesystems (`FSUnaryFileSystem`),
not virtual/network filesystems serving from Git ODB. Our NFS approach remains
the best option.

---

## 1. fskit-rs + FSKitBridge

- **Crate:** https://crates.io/crates/fskit-rs
- **Bridge:** https://github.com/debox-network/FSKitBridge
- **Architecture:** Swift appex handles FSKit/XPC ↔ Protobuf over TCP localhost ↔ Rust backend
- **Rust API:** Implement `fskit_rs::Filesystem` trait (lookup, read, write, readdir, etc.)
- **No Swift expertise needed** beyond bundling the .appex shim

### Critical Limitation for VibeFS
FSKit uses `FSUnaryFileSystem` — one physical disk resource maps to one volume.
It is designed for **traditional disk-backed local filesystems**, not virtual or
network filesystems. VibeFS serves files from Git's object database with no
underlying disk resource, making FSKit a fundamental mismatch. Additional issues:
- No process attribution (unlike FUSE)
- Read-only filesystem support is problematic
- Volumes auto-unmount on app updates without cleanup
- Permission issues with real (non-RAM) disks via `fskitd`

The `fskit_rs::Filesystem` trait maps structurally to our `NFSFileSystem` impl,
but the FSKit runtime assumptions about backing storage don't fit our model.

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
- Similar use case to VibeFS, though their FS model may be more disk-like

## 5. Current NFS Approach Assessment

Our NFSv3-over-localhost approach remains the most portable and dependency-free
option. Known trade-offs:
- (+) No external dependencies, cross-platform
- (+) No root/kext/recovery mode
- (-) xattr workarounds needed (build artifact symlinks)
- (-) No Finder/Spotlight integration
- (-) NFS mount semantics differ from local FS

## Recommendation

| Approach | Fit for VibeFS | Notes |
|----------|---------------|-------|
| **NFS (current)** | **Best** | Zero deps, cross-platform, works for virtual FS |
| macFUSE 5 + `fuser` | Good | Kext-free on macOS 26, but requires user install |
| FSKit via fskit-rs | **Poor** | Designed for disk-backed FS, not virtual/Git-backed |
| FUSE-T | Decent | Kext-less on all macOS, but extra dependency |

**Conclusion:** Stay on NFS. It was the right architectural call and remains the
best option for a virtual filesystem backed by Git ODB. FSKit's disk-centric model
is a fundamental mismatch. If FSKit ever expands to support virtual filesystems,
`fskit-rs` + FSKitBridge provides a ready migration path.

## Swift-Rust Interop (for reference)
If direct FSKit interop were ever needed (bypassing the TCP bridge):
- **swift-bridge** (v0.1.59, Jan 2026): Generates FFI glue, supports async bidirectionally
- **UniFFI** (v0.31.0, Mozilla): Multi-language bindings, production Swift support
- The FSKitBridge TCP approach is far simpler for most use cases

## Also Noted
- `objc2-fs-kit` crate exists (part of madsmtm/objc2) — raw Obj-C bindings to FSKit,
  but low-level and awkward since FSKit's API is Swift-centric
- `nfsserve` (our dependency) also used by ZeroFS (zerofs_nfsserve fork) — confirms
  the NFS-in-Rust pattern has legs
- XetHub wrote about choosing NFS over FUSE for the same reasons we did:
  https://xethub.com/blog/nfs-fuse-why-we-built-nfs-server-rust

## Key Links
- fskit-rs crate: https://crates.io/crates/fskit-rs
- FSKitBridge: https://github.com/debox-network/FSKitBridge
- objc2-fs-kit docs: https://docs.rs/objc2-fs-kit/
- macFUSE releases: https://github.com/macfuse/macfuse/releases
- FUSE-T: https://www.fuse-t.org/
- agent-harbor (prior art): https://github.com/blocksense-network/agent-harbor
- Apple FSKit docs: https://developer.apple.com/documentation/FSKit
- swift-bridge: https://github.com/chinedufn/swift-bridge
- UniFFI: https://github.com/mozilla/uniffi-rs
