# macOS Optimizer - Development Commands

## Development Commands
- `npm install` - Install Node.js dependencies
- `npm run dev` or `npm run tauri dev` - Run in development mode with hot reload
- `npm run build` or `npm run tauri build` - Build for production
- `./build.sh` - Production build script (creates .app bundle)

## Rust/Cargo Commands (in src-tauri/)
- `cargo build` - Build Rust backend
- `cargo build --release` - Build optimized release version
- `cargo check` - Check code for errors without building
- `cargo clippy` - Run Rust linter for code quality
- `cargo fmt` - Format Rust code

## Testing & Quality
- No test suite currently configured
- Recommended for new code:
  - `cargo test` - Run Rust tests (when added)
  - `cargo clippy -- -W clippy::all` - Strict linting
  - `cargo fmt --check` - Check formatting

## Debugging
- `RUST_LOG=debug npm run tauri dev` - Run with debug logging
- Check console for JavaScript errors
- Check terminal for Rust/Tauri errors

## macOS System Commands
- `git status` - Check repository status
- `git diff` - View changes
- `git add .` - Stage changes
- `git commit -m "message"` - Commit changes
- `ls -la` - List files with details
- `cd [path]` - Change directory
- `pwd` - Show current directory
- `grep -r "pattern" .` - Search in files
- `find . -name "*.rs"` - Find files by pattern

## Build Output
Production app location: `src-tauri/target/release/bundle/macos/macOS Optimizer.app`