# mdview

`mdview` is a read-only terminal Viewer for one Markdown Document. It presents
Markdown as a reflowing Rendered Document with an Outline, search, Selection,
links, and a persistent Reading Cursor. It is a reader, not an editor or a
source-code pager.

## Install

`mdview` supports stable Rust on macOS and Linux:

```console
cargo install mdview
```

From a source checkout, install the same executable with:

```console
cargo install --path . --locked
```

The installed command is `mdview`.

## Usage

Open one local Document:

```console
mdview README.md
```

Read a complete Document from standard input, either automatically or
explicitly:

```console
generate-docs | mdview
generate-docs | mdview -
```

`mdview` requires an interactive output terminal. It reads all input before the
Reading Session starts and obtains keyboard input from the controlling terminal
when standard input carries the Document.

```text
Usage: mdview [OPTIONS] [<DOCUMENT> | -]

Options:
  -h, --help
  -V, --version
  --              Stop option processing
```

A Reading Session contains exactly one Document. Multiple paths are rejected.
Use `--` before a path beginning with `-`. Help, version output, and normal exit
return status 0. Invalid usage, input failures, terminal failures, and Reading
Session errors return status 1.

## Fixed keymap

Press `?` in the Viewer for the compact key reference.

| Area | Keys | Action |
| --- | --- | --- |
| Navigation | `h` `j` `k` `l` | Move left, down, up, or right |
| Navigation | `w` `b` | Move by word |
| Navigation | `0` `^` `$` | Move to row start, first non-blank, or row end |
| Navigation | `gg` `G` | Move to the start or end of the Document |
| Navigation | `{` `}` | Move by paragraph |
| Navigation | a numeric count | Repeat the following motion |
| Navigation | `Ctrl-u` `Ctrl-d` | Move half a page up or down |
| Navigation | `Ctrl-b` `Ctrl-f` | Move a page up or down |
| Navigation | `Ctrl-y` `Ctrl-e` | Scroll the viewport up or down |
| Outline | `o` | Toggle the Outline |
| Outline | `Ctrl-w h` `Ctrl-w l` | Focus the Outline or Document |
| Outline | `j` `k` | Move the Outline Selection |
| Outline | `h` `l` | Collapse or expand the selected branch |
| Outline | `Enter` | Jump to the Outline Selection |
| Search | `/`, then `Enter` | Search rendered text literally |
| Search | `n` `N` | Move to the next or previous match |
| Search | `Esc` | Cancel the search prompt |
| Selection | `v` `V` | Begin characterwise or rendered-row Selection |
| Selection | motions, then `y` | Extend and copy the Selection |
| Selection | `Esc` | Cancel the Selection |
| Links | `gx` | Follow a fragment or explicitly open an HTTP(S) link |
| Links | `Ctrl-o` `Ctrl-i` | Move backward or forward through Jump History |
| Reload | `r` | Reload a local Document |
| Application | `?` | Open or dismiss help |
| Application | `q` `Ctrl-c` | Quit |
| Application | `Esc` | Dismiss help or return to the Document |

The keymap is fixed. `mdview` does not implement editing operators, registers,
macros, marks, Vim configuration, or a colon-command language. It does not
capture the mouse, so native terminal drag selection remains available.

## Markdown support

The Viewer supports CommonMark and the selected GitHub-compatible features:

- headings, paragraphs, block quotes, thematic breaks, links, and images;
- ordered, unordered, nested, and task lists;
- emphasis, strong emphasis, strikethrough, and inline code;
- fenced code with lazy syntax highlighting and safe plain-text fallback;
- GFM tables, footnotes, and note/tip/important/warning/caution Alerts;
- leading YAML Front Matter; and
- literal raw HTML.

Images are represented by alt text and their target; they are not downloaded or
displayed with a terminal graphics protocol. Raw HTML is shown as text, not
interpreted. MDX, math, diagrams, wikilinks, definition lists, heading
attributes, and application-specific Markdown extensions are not supported.
The Viewer has no raw Markdown source mode or source line numbers.

## Reload and clipboard behavior

Press `r` to Reload a local Document from the same path. Reload is manual, never
file-watched. A successful Reload tries to retain the Current Section and nearby
Reading Cursor position. A failed Reload leaves the last valid Document visible.
Standard-input Documents cannot be reloaded.

`v` or `V` begins a Selection and `y` copies its rendered text. Markdown
authoring markers and decorative borders are omitted, while semantic list
markers and code indentation are retained. On macOS, `mdview` tries `pbcopy`.
On Linux it tries `wl-copy`, `xclip`, then `xsel`. If native integration is
unavailable, it tries OSC 52 through the terminal. The status bar reports
success or failure; terminal policy can still block OSC 52.

## Platform, safety, and boundaries

macOS and Linux terminals are supported. Native Windows behavior is best-effort
and is not guaranteed. Set `NO_COLOR` to disable color; the semantic layout
remains usable in monochrome. Terminals smaller than 40 columns by 10 rows show
a recoverable resize message.

Document content is inert. Control characters are replaced or visibly escaped,
raw HTML is not executed, and referenced images, styles, includes, and links are
never fetched automatically. Only an explicit `gx` on an `http` or `https` link
may launch the system browser. Relative Document links remain display-only.
Inputs above the design scale of about 10 MiB or 100,000 lines are attempted with
a warning rather than rejected.

`mdview` deliberately has no configuration system, source mode, mouse capture,
network fetcher, multi-Document Reading Session, Windows support guarantee,
prebuilt binary, package-manager formula, shell completion, or man page. The
crate contains an internal library to support the executable and its tests, but
does not promise a stable public Rust library API.
