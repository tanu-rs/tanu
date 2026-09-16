# TUI

The TUI lets you browse tests, run all or some of them, and inspect every HTTP or gRPC call they made, without leaving the terminal.

```bash
cargo run -- tui
```

![tanu TUI](assets/tui.png){ .tanu-shot }

## Layout

The screen is split into three panes. Press `Tab` or click a pane to focus it.

| Pane | Contents |
|---|---|
| **Test list** | Tests grouped by project and module, with their latest result. |
| **Info** | Details of the selected test in four tabs: **Call**, **Headers**, **Payload**, and **Error**. |
| **Logger** | Log output from tanu and from your tests. |

## Key bindings

### Global

| Key | Action |
|---|---|
| `Tab` | Focus the next pane |
| `Shift+Tab` | Switch to the next Info tab |
| `z` | Maximize or restore the focused pane |
| `q` / `Esc` | Quit |

### Test list

| Key | Action |
|---|---|
| `j` / `↓` | Move cursor down |
| `k` / `↑` | Move cursor up |
| `g` / `Home` | Jump to the top |
| `G` / `End` | Jump to the bottom |
| `Ctrl+D` / `Ctrl+U` | Scroll down / up half a screen |
| `Enter` | Expand or collapse a project or module |
| `h` / `←`, `l` / `→` | Switch to the previous / next Info tab |
| `1` | Run all tests |
| `2` | Run the selected project, module, or test |

### Info

| Key | Action |
|---|---|
| `h` / `←`, `l` / `→` | Previous / next tab |
| `j` / `↓`, `k` / `↑` | Scroll down / up |
| `g` / `Home`, `G` / `End` | Scroll to the top / bottom |
| `Ctrl+D` / `Ctrl+U` | Scroll down / up half a screen |
| `1` | Run all tests |

### Logger

| Key | Action |
|---|---|
| `j` / `↓`, `k` / `↑` | Select a log target |
| `h` / `←`, `l` / `→` | Decrease / increase the log level shown for the selected target |
| `Space` | Toggle hiding of targets that are filtered out |
| `H` | Show or hide the target selector |
| `F` | Show only the selected target |

## Configuration

- Concurrency defaults to the number of logical CPU cores; change it with `-c` or `runner.concurrency`.
- Payload colors come from `[tui] payload.color_theme`. See [TUI theme](configuration.md#tui-theme).
- Credential masking and `runner.max_body_size` apply to the Payload view just as they do to CLI output.

## Tips

- Maximize the Info pane with `z` to read large payloads.
- Press `2` on a module to run just that module while iterating on it.
- Use `--log-level` and `--tanu-log-level` to control how much appears in the Logger pane. See [Command Line Options](command-line-option.md#tui).
