# macOS Optimizer - Code Style and Conventions

## Rust Code Style
- **Formatting**: Standard Rust formatting (use `cargo fmt`)
- **Naming**:
  - Snake_case for functions and variables: `get_system_info`, `memory_stats`
  - PascalCase for types and structs: `SystemInfo`, `CleanableFile`
  - SCREAMING_SNAKE_CASE for constants
- **Attributes**: Use `#[derive(Debug, Serialize, Deserialize)]` for data structures
- **Error Handling**: Return `Result<T, String>` for Tauri commands
- **Tauri Commands**: Mark with `#[tauri::command]` attribute
- **Module Structure**: Separate concerns into modules (system_info, file_cleaner, memory_optimizer)
- **Public API**: Mark public items with `pub`

## JavaScript Code Style
- **ES6+ Modern JavaScript**: Use const/let, arrow functions, template literals
- **No Framework**: Vanilla JavaScript with DOM manipulation
- **Async/Await**: For Tauri command invocations
- **Error Handling**: Try-catch blocks for async operations
- **Constants**: UPPER_CASE for configuration values
- **Functions**: camelCase naming
- **DOM Queries**: Cache elements when used multiple times
- **Event Handling**: addEventListener pattern

## HTML/CSS Style
- **Semantic HTML5**: Use appropriate tags
- **CSS Variables**: For theming (--color-primary, etc.)
- **BEM-like Naming**: For CSS classes (card, card-header, card-body)
- **Responsive Design**: Flexbox and Grid layouts
- **Dark Mode Support**: CSS variables for theme switching

## Project Structure Conventions
- **Frontend**: All web assets in `src/` directory
- **Backend**: Rust code in `src-tauri/src/`
- **Modules**: Logical separation (file_cleaner/, memory_optimizer/)
- **Types**: Define in separate types.rs files
- **Safety**: Implement safety checks in dedicated modules

## Safety Principles
- Never delete critical system files
- Always move to trash instead of permanent deletion
- Require user confirmation for destructive actions
- Use safe memory optimization techniques only