# mdview

[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![crates.io](https://img.shields.io/crates/v/mdviewer.svg)](https://crates.io/crates/mdviewer)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](#license)

Read Markdown without leaving your terminal.

`mdview` turns a local Markdown document—or Markdown piped through standard
input—into a focused, read-only reading experience. It renders the document's
structure instead of showing its source, with an outline, search, Vim-like
navigation, text selection, syntax highlighting, and link following.

## Features

- Semantic Markdown rendering with responsive terminal layout
- Navigable outline that follows the current section
- Vim-like motions, counts, scrolling, and jump history
- Literal text search with next and previous match navigation
- Characterwise and rowwise selection with clipboard support
- Syntax highlighting for fenced code blocks
- GFM tables, task lists, footnotes, alerts, and front matter
- Safe, inert rendering: document content is never executed or fetched
- Local-file reload that preserves your reading position when possible
- Native terminal selection remains available—`mdview` does not capture the
  mouse

## Installation

`mdview` requires Rust 1.88 or later and supports macOS and Linux.

Install from [crates.io](https://crates.io/crates/mdviewer):

```console
cargo install mdviewer --locked
```

Or install directly from this repository:

```console
cargo install --git https://github.com/xenoninja/markdown-viewer.git --locked
```

Or install from a local source checkout:

```console
git clone https://github.com/xenoninja/markdown-viewer.git
cd markdown-viewer
cargo install --path . --locked
```

These commands install the `mdview` executable. The crate is published as
`mdviewer` because [`mdview`](https://crates.io/crates/mdview) on crates.io is a
different project.

## Quick start

Open a local document:

```console
mdview README.md
```

Read Markdown from standard input:

```console
git show HEAD:README.md | mdview
```

Use `-` to make standard input explicit, or `--` before a path that starts with
`-`:

```console
generate-docs | mdview -
mdview -- -notes.md
```

`mdview` reads the complete input before opening and requires an interactive
output terminal. Run `mdview --help` for the full command-line reference.

## Keyboard shortcuts

Press `?` at any time to open the built-in key reference.

| Area | Keys | Action |
| --- | --- | --- |
| Navigation | `h` `j` `k` `l` | Move left, down, up, or right |
| Navigation | `w` `b` | Move by word |
| Navigation | `0` `^` `$` | Move to row start, first non-blank, or row end |
| Navigation | `gg` `G` | Go to the start or end of the document |
| Navigation | `{` `}` | Move by paragraph |
| Navigation | number + motion | Repeat a motion |
| Scrolling | `Ctrl-u` `Ctrl-d` | Move half a page up or down |
| Scrolling | `Ctrl-b` `Ctrl-f` | Move a page up or down |
| Scrolling | `Ctrl-y` `Ctrl-e` | Scroll the viewport up or down |
| Outline | `o` | Toggle the outline |
| Outline | `Ctrl-w h` `Ctrl-w l` | Focus the outline or document |
| Outline | `j` `k` | Move the outline selection |
| Outline | `h` `l` | Collapse or expand a branch |
| Outline | `Enter` | Jump to the selected section |
| Search | `/`, then `Enter` | Search rendered text |
| Search | `n` `N` | Go to the next or previous match |
| Selection | `v` `V` | Start characterwise or rowwise selection |
| Selection | motions, then `y` | Extend and copy the selection |
| Links | `gx` | Follow a fragment or open an HTTP(S) link |
| Links | `Ctrl-o` `Ctrl-i` | Move backward or forward through jump history |
| Document | `r` | Reload a local document |
| Application | `Esc` | Cancel or dismiss the active mode |
| Application | `q` `Ctrl-c` | Quit |

The keymap is intentionally fixed. `mdview` borrows familiar navigation from
Vim, but it is a reader rather than a Vim emulator or editor.

## Markdown support

`mdview` supports CommonMark plus commonly used GitHub-flavored extensions:

- headings, paragraphs, block quotes, thematic breaks, links, and images;
- ordered, unordered, nested, and task lists;
- emphasis, strong emphasis, strikethrough, and inline code;
- fenced code blocks with syntax highlighting and plain-text fallback;
- GFM tables, footnotes, and note/tip/important/warning/caution alerts;
- leading YAML front matter; and
- literal raw HTML.

Images are represented by their alt text and target. Raw HTML is displayed as
text, not interpreted. MDX, math, diagrams, wikilinks, definition lists,
heading attributes, and application-specific Markdown extensions are not
currently supported.

## Clipboard, color, and links

Selections copy rendered text rather than Markdown authoring syntax. On macOS,
`mdview` uses `pbcopy`. On Linux, it tries `wl-copy`, `xclip`, and `xsel`, then
falls back to OSC 52 when native clipboard integration is unavailable.

Set [`NO_COLOR`](https://no-color.org/) to disable color. The layout remains
usable in monochrome.

Document content is treated as inert data: control characters are escaped, raw
HTML is not executed, and images, styles, includes, and links are never fetched
automatically. Only an explicit `gx` on an `http` or `https` link can launch the
system browser. Relative document links are display-only.

## Development

Clone the repository and run the test suite:

```console
git clone https://github.com/xenoninja/markdown-viewer.git
cd markdown-viewer
cargo test --locked
```

Before opening a pull request, format the code and run the checks:

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Bug reports and pull requests are welcome through
[GitHub Issues](https://github.com/xenoninja/markdown-viewer/issues).

## Project scope

`mdview` is intentionally a small, single-document reader. It does not provide
editing, configuration, a raw-source mode, mouse capture, automatic file
watching, network fetching, or a stable public Rust library API. Native Windows
support is currently best-effort.

## License

`mdview` is available under the MIT License.
