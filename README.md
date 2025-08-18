# macOS Optimizer

A powerful, native macOS optimization application built with Rust and Tauri. This app helps you optimize RAM usage, clean unnecessary files, and monitor system performance with a beautiful, modern interface.

## Features

### 🚀 System Optimization
- **RAM Management**: Real-time memory monitoring and optimization
- **Memory Pressure Relief**: Advanced memory compression and cache purging
- **Process Management**: View and manage running processes

### 🗑️ Storage Cleaner
- **Smart File Detection**: Automatically identifies cleanable files
- **Safe Cleaning**: Protects important system and user files
- **Categories**:
  - System and browser caches
  - Old downloads
  - Temporary files
  - Development caches (Xcode, npm, pip, Homebrew)
  - Trash management

### 📊 System Monitoring
- **Real-time Dashboard**: Live system statistics
- **CPU Monitoring**: Track CPU usage and performance
- **Disk Usage**: Monitor storage space
- **System Information**: Detailed hardware and OS information

### 🎨 Modern UI
- **Native macOS Design**: Follows Apple's design guidelines
- **Dark/Light Mode**: Automatic theme switching
- **Responsive Layout**: Clean and intuitive interface

## Installation

### Prerequisites
- macOS 10.15 or later
- Node.js 16+ and npm
- Rust (will be installed automatically if needed)

### Development Setup

1. Clone the repository:
```bash
git clone https://github.com/yourusername/macos-optimizer.git
cd macos-optimizer
```

2. Install dependencies:
```bash
npm install
```

3. Run in development mode:
```bash
npm run tauri dev
```

### Building for Production

1. Make the build script executable:
```bash
chmod +x build.sh
```

2. Run the build script:
```bash
./build.sh
```

3. The built application will be available in:
```
src-tauri/target/release/bundle/macos/macOS Optimizer.app
```

4. Drag the app to your Applications folder to install.

## Usage

### Dashboard
The main dashboard provides an overview of your system's current state:
- Memory usage and pressure
- CPU utilization
- Disk space
- System uptime

Click "Quick Optimize" for one-click optimization.

### Memory Management
Navigate to the Memory tab to:
- View detailed memory statistics
- Optimize memory with advanced techniques
- Clear inactive memory
- Monitor swap usage

### Storage Cleaner
In the Storage tab:
1. Click "Scan System" to identify cleanable files
2. Review the files by category
3. Select files to clean
4. Click "Clean Selected" to free up space

### Process Manager
The Processes tab allows you to:
- View all running processes
- Sort by CPU or memory usage
- Search for specific processes
- Terminate unresponsive processes

## Technical Details

### Architecture
- **Frontend**: Vanilla JavaScript with modern ES6+
- **Backend**: Rust with Tauri framework
- **System Integration**: Native macOS APIs via sysinfo crate

### Key Technologies
- **Tauri**: Cross-platform desktop app framework
- **Rust**: Systems programming for performance
- **sysinfo**: System information gathering
- **walkdir**: Efficient file system traversal
- **libc**: Low-level system calls

### Security
- Sandboxed application environment
- Safe file deletion with trash support
- Protected system files cannot be deleted
- Memory optimization uses safe techniques

## Safety Features

The app includes multiple safety mechanisms:
- **Protected Files**: Never deletes critical system files
- **Trash Support**: Files are moved to trash, not permanently deleted
- **Confirmation Dialogs**: Requires user confirmation for destructive actions
- **Safe Memory Management**: Uses macOS-approved optimization techniques

## Performance

- **Low Resource Usage**: Minimal CPU and memory footprint
- **Fast Scanning**: Efficient file system traversal
- **Real-time Updates**: Live system statistics without performance impact
- **Native Performance**: Rust backend ensures optimal speed

## Troubleshooting

### Common Issues

1. **Permission Errors**: Some operations may require administrator privileges
2. **Build Errors**: Ensure you have the latest Xcode Command Line Tools
3. **Memory Optimization Limited**: Some features work better with sudo access

### Debug Mode

Run with debug logging:
```bash
RUST_LOG=debug npm run tauri dev
```

## Contributing

Contributions are welcome! Please:
1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Open a Pull Request

## License

MIT License - See LICENSE file for details

## Acknowledgments

- Built with [Tauri](https://tauri.app/)
- System information via [sysinfo](https://github.com/GuillaumeGomez/sysinfo)
- Icons and design inspired by macOS Big Sur

## Disclaimer

This application modifies system files and processes. While safety measures are in place, use at your own risk. Always ensure you have backups of important data before cleaning files or optimizing memory.

## Support

For issues, questions, or suggestions, please open an issue on GitHub.

---

Made with ❤️ for macOS
