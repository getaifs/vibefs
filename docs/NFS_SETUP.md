# NFS Setup Guide

## Overview

VibeFS can work in **two modes**:

1. **Session Directory Mode** (default, works everywhere)
2. **NFS Mount Mode** (optional, platform-specific requirements)

## Session Directory Mode (Recommended)

Work directly in the session directory - **no setup required**.

```bash
# Spawn creates the session directory
vibe spawn my-feature

# Work directly in the session directory
cd .vibe/sessions/my-feature/
vim src/main.rs

# Promote when done
vibe promote my-feature
git merge refs/vibes/my-feature
```

**Advantages:**
- ✅ Works on all platforms
- ✅ No root/sudo required
- ✅ No special configuration
- ✅ Simple and reliable

**When to use:** Always, unless you specifically need NFS mounts.

## NFS Mount Mode (Optional)

Provides a "cleaner" mount point path, but requires platform-specific setup.

### macOS

NFS mounting works automatically (no root required):

```bash
vibe spawn my-feature
# Automatically mounts to: ~/Library/Caches/vibe/mounts/repo-my-feature
cd ~/Library/Caches/vibe/mounts/repo-my-feature
```

**How it works:** macOS allows user-space NFS mounts on high ports with the `noresvport` option.

### Linux

NFS mounting **requires root privileges**. The workflow:

```bash
# 1. Spawn starts the NFS server
vibe spawn my-feature
# Shows: NFS server running on port 12345

# 2. Mount manually (requires sudo)
sudo mount -t nfs -o vers=3,tcp,port=12345,mountport=12345,nolock \
  localhost:/ ~/.cache/vibe/mounts/repo-my-feature

# 3. Work through the mount
cd ~/.cache/vibe/mounts/repo-my-feature
vim src/main.rs

# 4. When done, unmount
sudo umount ~/.cache/vibe/mounts/repo-my-feature
```

**Why sudo?** Linux requires root for the `mount` system call, regardless of port number.

#### Optional: Passwordless Mounting (Advanced)

If you frequently use NFS mounts and want to avoid typing passwords, you can configure sudo:

```bash
# Add to /etc/sudoers.d/vibefs (using visudo):
username ALL=(root) NOPASSWD: /usr/bin/mount -t nfs *
username ALL=(root) NOPASSWD: /usr/bin/umount /home/username/.cache/vibe/mounts/*
```

**Security note:** This allows mounting any NFS filesystem without password. Only configure if you understand the implications.

## Comparison

| Feature | Session Directory | NFS Mount |
|---------|------------------|-----------|
| **Setup** | None | Platform-specific |
| **Root required** | No | Linux: Yes, macOS: No |
| **Path** | `.vibe/sessions/<id>` | `~/.cache/vibe/mounts/<id>` |
| **Reliability** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ (can have stale mounts) |
| **Works in CI/CD** | Yes | No (usually) |
| **Containers** | Yes | Depends |

## Recommendations

**For most users:** Use Session Directory Mode
- Simpler
- More reliable
- Works everywhere

**For NFS Mode:** Only if you:
- Have sudo access (Linux)
- Want the cleaner mount path
- Don't mind the extra setup

## Troubleshooting

### "NFS mounting requires root privileges"

This is expected on Linux. Either:
1. Run the mount command manually with sudo (shown in error)
2. Use Session Directory Mode instead

### Stale mounts after crash

```bash
# Linux
sudo umount -l ~/.cache/vibe/mounts/<session-id>

# macOS
diskutil unmount force ~/Library/Caches/vibe/mounts/<session-id>
```

### Working in containers (Docker/Podman/Distrobox)

**Session Directory Mode:** Works perfectly
**NFS Mode:** May not work depending on container capabilities

Use Session Directory Mode for containerized development.
