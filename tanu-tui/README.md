# tanu-tui

[![Crates.io](https://img.shields.io/crates/v/tanu-tui)](https://crates.io/crates/tanu-tui)
[![Documentation](https://docs.rs/tanu-tui/badge.svg)](https://docs.rs/tanu-tui)
[![License](https://img.shields.io/crates/l/tanu-tui)](https://github.com/tanu-rs/tanu/blob/main/LICENSE)

Terminal User Interface (TUI) frontend for the tanu WebAPI testing framework.

## Overview

`tanu-tui` provides an interactive terminal interface for running and monitoring tanu tests. Built with [ratatui](https://github.com/ratatui-org/ratatui), it offers a rich, responsive UI for test execution, real-time monitoring, and result visualization.

## Key Features

### Interactive Test Execution
- **Real-time Test Monitoring** - Watch tests execute in real-time with live updates
- **Syntax Highlighting** - Beautiful syntax highlighting for JSON payloads and responses
- **Test Filtering** - Filter tests by name, status, or project
- **Parallel Execution** - Visual indication of concurrent test execution
- **Progress Tracking** - Progress bars and statistics for test suite execution

### Rich UI Components
- **Test Tree View** - Hierarchical view of test suites and individual tests
- **Response Viewer** - Formatted display of HTTP responses with syntax highlighting
- **Log Viewer** - Real-time log output with filtering and search
- **Statistics Panel** - Live statistics on test execution, success rates, and timing
- **Help System** - Built-in help with keyboard shortcuts and navigation

### Customization
- **Color Themes** - Configurable color schemes for different UI elements
- **Layout Options** - Flexible layout with resizable panels
- **Keyboard Navigation** - Vim-style keyboard shortcuts for efficient navigation

## Usage

The TUI is typically launched through the main tanu binary:

```bash
cargo run -- tui
```

Or if you have tanu installed:

```bash
tanu tui
```

## Configuration

Configure the TUI appearance in your `tanu.toml`:

```toml
[[projects]]
name = "my-api-tests"

[projects.tui.payload.color_theme]
keyword = "blue"
string = "green"
number = "yellow"
boolean = "magenta"
null = "red"
property = "cyan"
punctuation = "white"
```

## Keyboard Shortcuts

### Navigation
- `↑/↓` or `j/k` - Navigate test list
- `←/→` or `h/l` - Navigate between panels
- `Tab` - Switch between main panels
- `Enter` - Select test or expand/collapse tree nodes
- `Space` - Toggle test selection for execution

### Test Control
- `r` - Run selected tests
- `R` - Run all tests
- `s` - Stop test execution
- `c` - Clear test results

### View Controls
- `f` - Toggle full-screen mode for current panel
- `F` - Filter tests (opens filter dialog)
- `/` - Search tests
- `n` - Next search result
- `N` - Previous search result

### General
- `q` - Quit application
- `?` - Show help dialog
- `Ctrl+C` - Force quit

## Architecture

The TUI follows an event-driven architecture with:

### Core Components
- **App State** - Central state management using the Elm architecture pattern
- **Event System** - Async event handling for user input and test updates
- **Rendering Engine** - Efficient terminal rendering with ratatui
- **Theme System** - Customizable color schemes and styling

### UI Panels
- **Test List Panel** - Displays available tests with status indicators
- **Response Panel** - Shows HTTP response details with syntax highlighting
- **Log Panel** - Real-time log output from test execution
- **Statistics Panel** - Test execution metrics and progress

### Integration
The TUI integrates seamlessly with:
- **`tanu-core`** - For test execution and HTTP functionality
- **`tanu-derive`** - For test discovery and registration
- **Async Runtime** - Full tokio integration for responsive UI

## Syntax Highlighting

The TUI provides syntax highlighting for:
- **JSON** - Request/response bodies with proper formatting
- **HTTP Headers** - Header names and values
- **URLs** - Request URLs with different components highlighted
- **Status Codes** - Color-coded based on HTTP status class
- **Timestamps** - Formatted timestamps for test execution

## Performance

The TUI is designed for performance:
- **Efficient Rendering** - Only redraws changed portions of the screen
- **Async Processing** - Non-blocking test execution and UI updates
- **Memory Efficient** - Minimal memory usage even with large test suites
- **Responsive** - Maintains 60fps even during intensive test execution

## Error Handling

The TUI provides comprehensive error handling:
- **Test Failures** - Clear display of test failures with stack traces
- **Network Errors** - Detailed HTTP error information
- **Configuration Errors** - Validation and display of configuration issues
- **Runtime Errors** - Graceful handling of unexpected errors

## Accessibility

The TUI supports:
- **Screen Readers** - Compatible with terminal screen readers
- **High Contrast** - Configurable color schemes for visibility
- **Keyboard Only** - Full functionality without mouse
- **Responsive Layout** - Adapts to different terminal sizes

## Development

The TUI is built with:
- **Rust** - Safe, fast, and reliable
- **Ratatui** - Modern terminal UI framework
- **Crossterm** - Cross-platform terminal handling
- **Tokio** - Async runtime for responsive UI
- **Syntect** - Syntax highlighting engine

## Integration

`tanu-tui` integrates with the broader tanu ecosystem:
- Uses `tanu-core` for HTTP client and test execution
- Leverages `tanu-derive` for test discovery
- Provides visual feedback for all tanu framework features

For complete examples and documentation, see the [main tanu repository](https://github.com/tanu-rs/tanu).