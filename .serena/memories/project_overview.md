# macOS Optimizer - Project Overview

## Purpose
A native macOS optimization application that helps users optimize RAM usage, clean unnecessary files, and monitor system performance. Built with a focus on safety and performance.

## Tech Stack
- **Frontend**: Vanilla JavaScript (ES6+), HTML5, CSS3
- **Backend**: Rust with Tauri framework (v2)
- **Build System**: npm for frontend, Cargo for Rust
- **Key Dependencies**:
  - sysinfo (0.30) - System information gathering
  - walkdir (2) - File system traversal
  - tokio (1) - Async runtime
  - serde/serde_json - Serialization
  - libc (0.2) - Low-level system calls
  - bytesize, chrono, dirs - Utilities

## Main Features
1. **System Optimization**
   - Real-time memory monitoring and optimization
   - Memory pressure relief
   - Process management

2. **Storage Cleaner**
   - Smart file detection with safety levels
   - Categories: caches, downloads, temp files, dev caches, trash
   - Protected files system

3. **System Monitoring**
   - Real-time dashboard
   - CPU, memory, disk monitoring
   - Process viewer with kill functionality

## Architecture
- Desktop application using Tauri (Rust backend + Web frontend)
- IPC communication between frontend and backend via Tauri commands
- Multi-threaded backend with async support
- Sandboxed environment for security