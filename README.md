# openspec-tui

A terminal UI for browsing [OpenSpec](https://github.com/Fission-AI/OpenSpec) changes and artifacts. Built with Rust using [ratatui](https://github.com/ratatui/ratatui) and [crossterm](https://github.com/crossterm-rs/crossterm).

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) toolchain (edition 2024)
- [openspec](https://github.com/Fission-AI/OpenSpec) CLI installed and available on `PATH`

## Build & Run

### Using Nix (recommended)

The project includes a Nix flake that provides a complete development environment with Rust, openspec, and related tools:

```sh
nix develop
cargo run
```

### Manual Setup

```sh
cargo build
cargo run
```

## Usage

Run `openspec-tui` from a directory that contains an `openspec/` folder with changes.

### Screens

The application has three screens:

1. **Change List** -- Lists all active openspec changes with task progress.
2. **Artifact Menu** -- Shows available artifacts (Proposal, Design, Tasks, Specs) for a selected change.
3. **Artifact View** -- Displays the content of a selected artifact with scrolling.

### Keyboard Shortcuts

| Key              | Action                              |
|------------------|-------------------------------------|
| `j` / `Down`     | Move selection down / scroll down   |
| `k` / `Up`       | Move selection up / scroll up       |
| `Enter`          | Open selected item                  |
| `Esc`            | Go back to previous screen          |
| `q`              | Quit                                |

## License

Not yet specified.
