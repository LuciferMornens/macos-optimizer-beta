# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

### Development
```bash
# Install dependencies (first time setup)
npm install

# Run in development mode with hot reload
npm run tauri dev

# Run with debug logging
RUST_LOG=debug npm run tauri dev
```

### Production Build
```bash
# Make build script executable (first time only)
chmod +x build.sh

# Build for production
./build.sh
# or directly:
npm run tauri build

# Output location: src-tauri/target/release/bundle/macos/macOS Optimizer.app
```

### Rust-specific Commands
```bash
# Build Rust backend only
cd src-tauri && cargo build

# Run tests
cd src-tauri && cargo test

# Check for compilation errors
cd src-tauri && cargo check

# Format Rust code
cd src-tauri && cargo fmt
```

## Architecture Overview

### Technology Stack
- **Frontend**: Vanilla JavaScript (ES6+) in `src/` directory
  - `index.html`: Main UI with tab-based navigation
  - `main.js`: Frontend logic and Tauri API invocations
  - `styles.css`: Modern macOS-style UI styling
  
- **Backend**: Rust with Tauri framework in `src-tauri/src/`
  - `lib.rs`: Main entry point with all Tauri command handlers and state management
  - `system_info.rs`: System monitoring functionality using sysinfo crate
  - `memory_optimizer.rs`: Memory optimization and pressure management
  - `file_cleaner.rs`: Safe file cleaning with trash support
  - `main.rs`: Application bootstrap

### Key Architectural Patterns

1. **State Management**: Uses Tauri's `State<AppState>` with Mutex-protected components:
   - `SystemMonitor`: Live system statistics
   - `FileCleaner`: File scanning and cleaning operations  
   - `MemoryOptimizer`: Memory management operations

2. **IPC Communication**: Frontend-backend communication via Tauri commands:
   - All commands defined in `lib.rs` with `#[tauri::command]` attribute
   - Frontend invokes via `window.__TAURI__.core.invoke()`
   - Error handling with `Result<T, String>` return types

3. **Modular Backend Structure**: Each module is self-contained:
   - Public structs and methods exposed via module exports
   - Serde serialization for JS interop
   - Error propagation using `Result` types

### Critical Safety Considerations

1. **File Operations**: 
   - Files moved to trash, not permanently deleted
   - Protected paths hardcoded in `file_cleaner.rs`
   - Never modify system-critical directories

2. **Memory Operations**:
   - Uses safe macOS APIs (vm_stat, purge)
   - Admin operations require explicit user authentication
   - Process killing restricted to non-system processes

3. **Permission Model**:
   - Most operations work without admin privileges
   - Admin-required operations use GUI authentication prompts
   - Capability restrictions defined in `capabilities/default.json`

## Development Workflow

When modifying this codebase:

1. **Frontend Changes**: Edit files in `src/`, changes auto-reload in dev mode
2. **Backend Changes**: Edit Rust files in `src-tauri/src/`, requires restart
3. **Adding New Commands**: 
   - Define in appropriate module
   - Add command handler in `lib.rs`
   - Register in `invoke_handler!` macro
   - Call from frontend via `invoke()`

4. **Testing Changes**:
   - Use development mode for quick iteration
   - Test memory operations carefully (they affect system)
   - Verify file cleaning in dry-run before actual deletion

## Module Dependencies

- `sysinfo`: System and process information
- `walkdir`: Efficient directory traversal
- `libc`: Low-level system calls for memory operations
- `dirs`: Platform-specific directory paths
- `chrono`: Timestamp handling
- `bytesize`: Human-readable byte formatting
- `tokio`: Async runtime (currently for future features)