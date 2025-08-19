# macOS Optimizer - Task Completion Checklist

## When Completing a Task

### For Rust Code Changes:
1. **Format Code**: Run `cargo fmt` in src-tauri/ directory
2. **Check for Errors**: Run `cargo check`
3. **Lint Code**: Run `cargo clippy` to catch common issues
4. **Build Check**: Run `cargo build` to ensure compilation
5. **Test Changes**: Run the app with `npm run tauri dev` to verify functionality

### For JavaScript Changes:
1. **Test in Browser Console**: Check for JavaScript errors
2. **Verify Tauri Commands**: Ensure all invoke() calls work
3. **Check UI Responsiveness**: Test all interactive elements
4. **Cross-browser Testing**: Verify in Tauri's webview

### Before Committing:
1. **Review Changes**: `git diff` to review all modifications
2. **Test Full Application**: Run complete user workflows
3. **Check Error Handling**: Verify error messages display correctly
4. **Memory Leaks**: Monitor for memory issues during testing
5. **Performance**: Ensure no performance regressions

### Quality Checks:
- Code follows existing patterns and conventions
- No hardcoded values that should be configurable
- Proper error handling implemented
- User-facing messages are clear and helpful
- Security: No exposed sensitive data or unsafe operations
- Documentation: Update comments if behavior changed

### Final Steps:
1. Stage changes: `git add .`
2. Commit with descriptive message: `git commit -m "feat: description"`
3. Test production build if major changes: `npm run tauri build`

## Important Notes:
- Always test on actual macOS system
- Verify admin vs non-admin functionality separately
- Check file permissions for system operations
- Ensure trash operations work correctly