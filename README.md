# htmlbox

HTML/CSS editor and renderer in Rust using `tiny-skia`.

`htmlbox` is a high-performance, lightweight library for parsing, styling, and rendering HTML and CSS directly in Rust. Built on top of `tiny-skia`, it provides a cross-platform foundation for rendering rich text, layouts, and web content in your native desktop applications, without the overhead of a full browser engine like Chromium.

## Features

- **Rendering Engine:** Fast, software-based rendering via `tiny-skia`.
- **Styling & Layout:** Core CSS cascade and layout support.
- **Interactivity:** Content editing (`contenteditable`), event handling, and transitions.
- **Cross-Platform Windowing:** Integration with `winit`.
- **Accessibility:** Built-in `accesskit` support for screen readers.

## Getting Started

Add `htmlbox` to your `Cargo.toml`:

```toml
[dependencies]
htmlbox = "0.2.0"
```

## Examples

The repository includes several examples demonstrating the capabilities of `htmlbox`. You can run them using `cargo run --example <name>`:

- `animation_demo`: CSS animations
- `transitions_demo`: CSS transitions
- `contenteditable_demo`: Interactive text editing
- `markdown_demo`: Rendering markdown to HTML
- `print_demo`: Print layouts
- `layout_features`: Advanced CSS layout features
- `cascade_features`: CSS cascade rules
- `transform_filter_demo`: CSS transforms and filters
- `event_playground`: Interactive event handling

```bash
cargo run --example contenteditable_demo
```

## License

This project is dual-licensed to offer both open-source and commercial use:

**1. Open Source (GPLv3)**
If you are building an open-source application under a compatible license, you may use `htmlbox` under the terms of the [GNU General Public License v3.0](LICENSE-GPL) (or later).x

**2. Commercial License**
If you want to use `htmlbox` in a proprietary, closed-source commercial product without the requirements of the GPLv3, you must purchase a Commercial License. Please contact us for pricing and details.
