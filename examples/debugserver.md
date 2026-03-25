# debugserver — Remote Debug Server for rhtmledit

A headless rendering server that loads HTML pages and accepts JSON commands over TCP or stdin.
Enables remote inspection, interaction, and screenshot capture without a GUI.

## Quick Start

```bash
# Build
cargo build --example debugserver

# Start with a URL (TCP mode, default port 9222)
cargo run --example debugserver -- https://example.com

# Start with a local file
cargo run --example debugserver -- file:///path/to/page.html

# Start in stdin mode (for piping commands)
cargo run --example debugserver -- https://example.com --stdin
```

## CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--url <url>` | (positional) | URL or file path to render |
| `--port <n>` | `9222` | TCP port to listen on |
| `--stdin` | off | Read commands from stdin instead of TCP |
| `--width <px>` | `1280` | Viewport width in CSS pixels |
| `--height <px>` | `900` | Viewport height for vh units |
| `--max-height <px>` | `4000` | Max render height for screenshots |
| `--scale <n>` | `1` | Device pixel ratio (HiDPI) |
| `--out <file.png>` | `snapshot.png` | Default screenshot output path |
| `--no-images` | off | Skip image fetching |
| `--no-cache` | off | Always fetch from network |
| `--cache-dir <dir>` | `snapshot_cache` | Cache directory |

## Sending Commands

### TCP (default)
```bash
# One-shot via helper script
./examples/debugclient.sh send 9222 '{"cmd":"screenshot"}'

# Interactive session
./examples/debugclient.sh 9222
```

### Stdin mode
```bash
echo '{"cmd":"find","selector":"h1"}' | cargo run --example debugserver -- page.html --stdin
```

### Python one-liner
```python
import socket
s = socket.socket(); s.connect(('127.0.0.1', 9222))
s.sendall(b'{"cmd":"screenshot"}\n')
print(s.recv(65536).decode())
s.close()
```

## Command Reference

All commands are JSON objects with a `"cmd"` field. Responses are single-line JSON with `"ok": true/false`.

### Navigation & Rendering

#### `screenshot` — Render page to PNG
```json
{"cmd":"screenshot"}
{"cmd":"screenshot","out":"/tmp/page.png"}
```
Response: `{"ok":true,"path":"snapshot.png","width":1280,"height":4000}`

#### `navigate` — Load a new URL
```json
{"cmd":"navigate","url":"https://example.com"}
{"cmd":"navigate","url":"file:///tmp/test.html"}
```

#### `resize` — Change viewport dimensions and re-layout
```json
{"cmd":"resize","width":800}
{"cmd":"resize","width":800,"height":600}
```

#### `scroll` — Scroll the viewport
```json
{"cmd":"scroll","dy":100}
{"cmd":"scroll","dy":-200}
```

### Finding Elements

#### `find` — List elements matching a CSS selector
```json
{"cmd":"find","selector":"h1"}
{"cmd":"find","selector":"div.container"}
{"cmd":"find","selector":"#main"}
{"cmd":"find","selector":"table td"}
```
Response: `{"ok":true,"count":3,"elements":[{"tag":"h1","id":"","class":"","x":8,"y":0,"w":1264,"h":38}]}`

Selector syntax: `tag`, `#id`, `.class`, `tag.class`, `tag#id`, `.class1.class2`

#### `text` — Get text content of elements
```json
{"cmd":"text","selector":"h1"}
```
Response: `{"ok":true,"count":1,"texts":["Page Title"]}`

#### `attr` — Get attribute value
```json
{"cmd":"attr","selector":"a","name":"href"}
```

#### `html` — Get serialized HTML of an element
```json
{"cmd":"html","selector":"#nav"}
```

#### `dom-path` / `path` — Get full CSS selector path
```json
{"cmd":"path","selector":"h1"}
```
Response: `{"ok":true,"count":1,"paths":["html > body > div.container > h1"]}`

#### `parent` — Get ancestor chain
```json
{"cmd":"parent","selector":"td"}
```
Response includes full parent chain with tag/id/class/position for each ancestor.

### Inspection

#### `inspect` — Box model and key styles
```json
{"cmd":"inspect","selector":"div.sidebar"}
```
Returns: content/padding/margin rects, display, position, font-size, color, background, resolved spacing.

#### `deep` — Comprehensive element dump
```json
{"cmd":"deep","selector":"td"}
```
Returns everything `inspect` does plus:
- All HTML attributes
- Border rect
- CSS property raw values (before resolution)
- Line cache (text layout lines with positions)
- Child elements summary
- Text node content and positions

#### `computed` — All computed CSS properties
```json
{"cmd":"computed","selector":"h1"}
```
Returns the full computed style: display, position, float, visibility, opacity, overflow, box-sizing, width/height/min/max, font properties, text properties, colors, flex properties, table properties, border styles/colors/radii, positioning (top/right/bottom/left), z-index, plus resolved box model values and matched rules count.

#### `css` — Query specific CSS properties
```json
{"cmd":"css","selector":"td","props":"vertical-align,padding-top,cell-padding"}
```
Supported property names:
- Standard CSS: `display`, `position`, `float`, `vertical-align`, `text-align`, `font-size`, `color`, `background-color`, `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`, `overflow-x`, `overflow-y`, `box-sizing`, `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`, `align-items`, `align-self`, `justify-content`, `padding-top/right/bottom/left`, `margin-top/right/bottom/left`, `border-collapse`, `cell-padding`, `border-spacing`
- Resolved layout values: `resolved-padding`, `resolved-margin`, `resolved-border`, `content-rect`, `padding-rect`, `margin-rect`, `border-rect`, `line-count`

#### `matched-rules` / `rules` — CSS cascade info
```json
{"cmd":"rules","selector":"h1"}
```
Returns all matched CSS rules for each element:
```json
{
  "rules": [
    {
      "selector": "h1",
      "specificity": 1,
      "source": "ua",
      "declarations": {"font-weight": "bold", "font-size": "2em"}
    },
    {
      "selector": "h1",
      "specificity": 1,
      "source": "",
      "declarations": {"color": "#2c3e50"}
    }
  ]
}
```

#### `highlight` — Screenshot with box model overlay
```json
{"cmd":"highlight","selector":"h1","out":"/tmp/highlight.png"}
```
Renders the page with margin (orange), padding (green), and content (blue) overlays on matching elements. Like Chrome DevTools element highlight.

### Interaction

#### `click` — Simulate mouse click
```json
{"cmd":"click","x":640,"y":200}
{"cmd":"click","selector":"#submit-btn"}
```
Clicking by selector targets the center of the element's content rect.

#### `hover` — Simulate mouse hover
```json
{"cmd":"hover","x":100,"y":50}
{"cmd":"hover","selector":".dropdown-trigger"}
```
Triggers hover styles. Returns whether styles changed.

#### `type` — Type text into focused element
```json
{"cmd":"type","text":"hello world"}
```
Types each character as a keypress into the currently focused element.

#### `key` — Send a key press
```json
{"cmd":"key","key":"Enter"}
{"cmd":"key","key":"Tab"}
{"cmd":"key","key":"Backspace"}
```
Supported keys: `Enter`, `Tab`, `Backspace`, `Delete`, `Escape`, `ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`, `Home`, `End`, `Space`, or any single character.

### Mutation

#### `setstyle` — Modify CSS property and re-layout
```json
{"cmd":"setstyle","selector":"h1","prop":"color","value":"red"}
{"cmd":"setstyle","selector":"div","prop":"display","value":"flex"}
```

#### `setattr` — Modify HTML attribute
```json
{"cmd":"setattr","selector":"img","name":"src","value":"new.png"}
```

### Debug

#### `tree` — Dump the box tree
```json
{"cmd":"tree"}
{"cmd":"tree","selector":"table"}
```
Returns a text dump of the layout tree showing tag, id, class, display type, content rect, and margin rect for each node.

### Performance

#### `perf` — Show load timing breakdown
```json
{"cmd":"perf"}
```
Response:
```json
{
  "load_timing": {
    "total_ms": 432.64,
    "fetch_html": 0.29,
    "parse_html": 12.63,
    "fetch_css": 0.00,
    "cascade": 7.20,
    "layout": 409.27,
    "nodes": 1130,
    "text_nodes": 730,
    "css_rules": 200,
    "fetch_images": 3.13
  }
}
```
Shows timing for each stage of the last page load: HTML fetch, parse, CSS fetch/parse, cascade, layout, image fetch. Also includes document stats (node count, text nodes, CSS rules).

#### `bench` — Benchmark cascade/layout/render
```json
{"cmd":"bench","n":5}
```
Response:
```json
{
  "iterations": 5,
  "cascade_ms": {"avg": 7.46, "min": 7.31, "max": 7.88},
  "layout_ms": {"avg": 198.19, "min": 194.45, "max": 208.97},
  "render_ms": {"avg": 177.98, "min": 177.55, "max": 178.81},
  "total_avg_ms": 383.63
}
```
Runs cascade, layout, and render N times each (max 100) and reports min/avg/max. Useful for profiling optimizations.

#### Automatic `cmd_ms` on every response
Every command response automatically includes `"cmd_ms"` — the wall time for that command in milliseconds.
```json
{"ok":true,"count":1,"elements":[...],"cmd_ms":0.23}
```

#### `screenshot` timing
Screenshots include render and PNG save timing:
```json
{"timing_ms":{"render":181.84,"save_png":1431.92,"total":1625.49}}
```

#### `quit` — Shut down the server
```json
{"cmd":"quit"}
```

## Interactive Client

The `debugclient.sh` script provides an interactive REPL with shortcuts:

```
$ ./examples/debugclient.sh 9222
Connected to debugserver on port 9222

Commands:
  ss                  screenshot
  f <selector>        find elements         i <selector>   inspect element
  tx <selector>       get text content       a <sel> <attr> get attribute
  c <sel> | c x y     click                  h <sel> | h x y  hover
  k <key>             send key (Enter, Tab, etc.)
  ty <text>           type text              sc <dy>        scroll
  r <width> [height]  resize viewport        nav <url>      navigate
  t [selector]        box tree               q              quit server
  Any JSON: {"cmd":"...","key":"val"}

dbg> f h1
dbg> i .sidebar
dbg> c #submit
dbg> ss
```

## Typical Debugging Workflows

### Investigate a layout issue
```bash
# Start server
cargo run --example debugserver -- file:///path/to/page.html --port 9222 &

# Find the element
./examples/debugclient.sh send 9222 '{"cmd":"find","selector":".broken-element"}'

# Get full computed style
./examples/debugclient.sh send 9222 '{"cmd":"computed","selector":".broken-element"}'

# Check what CSS rules matched
./examples/debugclient.sh send 9222 '{"cmd":"rules","selector":".broken-element"}'

# See the parent chain
./examples/debugclient.sh send 9222 '{"cmd":"parent","selector":".broken-element"}'

# Highlight it visually
./examples/debugclient.sh send 9222 '{"cmd":"highlight","selector":".broken-element","out":"/tmp/debug.png"}'

# Try a fix live
./examples/debugclient.sh send 9222 '{"cmd":"setstyle","selector":".broken-element","prop":"display","value":"flex"}'
./examples/debugclient.sh send 9222 '{"cmd":"screenshot","out":"/tmp/after-fix.png"}'
```

### Compare before/after
```bash
# Screenshot before
./examples/debugclient.sh send 9222 '{"cmd":"screenshot","out":"/tmp/before.png"}'

# Make a change
./examples/debugclient.sh send 9222 '{"cmd":"setstyle","selector":"table","prop":"border-collapse","value":"collapse"}'

# Screenshot after
./examples/debugclient.sh send 9222 '{"cmd":"screenshot","out":"/tmp/after.png"}'
```

### Test interactions
```bash
# Click a button
./examples/debugclient.sh send 9222 '{"cmd":"click","selector":"#toggle-btn"}'

# Check what changed
./examples/debugclient.sh send 9222 '{"cmd":"screenshot","out":"/tmp/clicked.png"}'

# Hover over a dropdown
./examples/debugclient.sh send 9222 '{"cmd":"hover","selector":".menu-trigger"}'
./examples/debugclient.sh send 9222 '{"cmd":"screenshot","out":"/tmp/hovered.png"}'
```

### Profile performance
```bash
# Check page load breakdown
./examples/debugclient.sh send 9222 '{"cmd":"perf"}'

# Benchmark the pipeline (5 iterations)
./examples/debugclient.sh send 9222 '{"cmd":"bench","n":5}'

# Navigate to a heavy page and check what's slow
./examples/debugclient.sh send 9222 '{"cmd":"navigate","url":"https://heavy-page.com"}'
./examples/debugclient.sh send 9222 '{"cmd":"perf"}'

# Make a change and measure the re-render cost
./examples/debugclient.sh send 9222 '{"cmd":"setstyle","selector":"*","prop":"box-sizing","value":"border-box"}'
# cmd_ms in the response shows how long the setstyle + re-layout took
```
