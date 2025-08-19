# macOS Optimizer - Project Structure

## Root Directory
```
macos-optimizer/
├── src/                    # Frontend code
│   ├── index.html         # Main HTML file
│   ├── main.js           # JavaScript application logic
│   ├── styles.css        # CSS styles
│   └── assets/           # Static assets (icons, images)
├── src-tauri/            # Rust backend
│   ├── src/              # Rust source code
│   │   ├── main.rs       # Entry point
│   │   ├── lib.rs        # Main library with Tauri commands
│   │   ├── system_info.rs # System information module
│   │   ├── file_cleaner.rs # File cleaning module entry
│   │   ├── file_cleaner/
│   │   │   ├── engine.rs   # Cleaning engine implementation
│   │   │   ├── safety.rs   # File safety checks
│   │   │   └── types.rs    # Type definitions
│   │   ├── memory_optimizer.rs # Memory optimization entry
│   │   └── memory_optimizer/
│   │       ├── admin.rs     # Admin-level optimizations
│   │       ├── non_admin.rs # User-level optimizations
│   │       ├── stats.rs     # Memory statistics
│   │       └── utils.rs     # Utility functions
│   ├── Cargo.toml        # Rust dependencies
│   ├── Cargo.lock        # Locked dependencies
│   ├── tauri.conf.json   # Tauri configuration
│   ├── build.rs          # Build script
│   ├── icons/            # Application icons
│   ├── capabilities/     # Tauri capabilities config
│   └── rules/            # Cleaning rules configuration
├── package.json          # Node.js dependencies
├── package-lock.json     # Locked npm dependencies
├── build.sh             # Build script for production
├── README.md            # Project documentation
└── .gitignore          # Git ignore rules
```

## Module Responsibilities

### Frontend (src/)
- **index.html**: Application layout and structure
- **main.js**: UI logic, Tauri command invocations, event handling
- **styles.css**: Visual styling, themes, responsive design

### Backend Modules (src-tauri/src/)
- **lib.rs**: Central command registry, state management
- **system_info**: OS info, CPU, memory, disk, process monitoring
- **file_cleaner**: Scan, categorize, and safely clean files
- **memory_optimizer**: RAM optimization strategies

## Key Design Patterns
1. **Command Pattern**: Tauri commands for frontend-backend communication
2. **State Management**: Shared AppState with mutex for thread safety
3. **Module Separation**: Clear boundaries between features
4. **Safety First**: Multiple layers of file safety checks
5. **Progressive Enhancement**: Basic features for all, advanced for admin