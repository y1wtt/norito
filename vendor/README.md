# Iced patches

Base: crates.io `iced_core 0.14.0`, `iced_graphics 0.14.0` (commit `3997291f318a8bc06fa522f5579836fb3feb94df`) and `iced_widget 0.14.2` (commit `54abf81d13fec06d4d9ac754b03ce2c3313e8a1f`). See `LICENSE-ICED` for the upstream MIT license, sourced from https://github.com/iced-rs/iced/blob/0.14.0/LICENSE.

Local extension: `text::highlighter::Format::size` accepts per-span font sizes; graphics forwards these sizes to cosmic-text metrics. This makes headings editable in the same continuous editor as body text. The default `None` retains upstream behavior. No Markdown or application logic belongs here.

Per-span line heights use the editor's configured line-height ratio, so typography settings remain in the consuming widget.

`Format::line_height` can override this ratio. Empty-line defaults support paragraph gaps without adding characters to the document. The first span supplies the line's default attributes so input at a heading's start inherits its typography.

`Editor::with_preedit` clones the styled layout and inserts IME composition only into that temporary editor. TextEditor draws it in place, underlines the composition and reports the actual caret rectangle to the OS, suppressing the runtime's separate preedit overlay. Commit uses the normal edit path; empty Commit events are ignored so cancellation preserves selected text. Only empty spacers with smaller-than-body metrics are reset to body metrics when composing; empty headings and code lines retain their typography. The consuming Markdown widget supplies default formatting even for headings with no visible characters. Tests cover body/heading/wrapped input, selections, cancellation and empty H1–H6 composition versus committed geometry. Windows IME body, populated headings and first input into an empty H1 were checked manually.

Additional patches: graphics uses cosmic-text's actual run coordinates for selection and caret placement, invalidating the geometry cache after styling. `Editor::caret_height` lets the widget draw a caret matching the heading size. PickList accepts `menu_width` so a compact menu trigger can open a readable dropdown.

`Content::caret_bounds` exposes local geometry for a selection formatting popover. Explicit `move_to` clears an absent selection and reveals the new cursor in the viewport, supporting outline navigation without document mutation.

Highlighting formats all remaining lines instead of estimating visible lines from body metrics. Span sizes and paragraph spacing change viewport occupancy, so this keeps heading geometry ready before scrolling or outline jumps.

`Editor::line_bounds` and `Content::line_bounds` expose visible logical-line rectangles, including wrapped runs. The generic `norito-iced-ext` decoration widget uses these coordinates for rules and inline SVGs, clipped to the editor viewport. Markdown-specific placement remains in `norito-editor-widget`.

Dependencies are patched at the workspace root. No upstream proposal has been submitted. Validate rich heading hit testing, selection across sizes, wrapping, scrolling, dropdown width and IME when upgrading. `norito-iced-ext` tests exercise layout coordinates and pointer hit testing without launching the application.

`Editor::span_bounds` / `Content::span_bounds` expose glyph ranges for inline decorations, and `span_baseline` the baseline of the run holding them, so inline SVGs can sit on the text baseline. TextEditor accepts logical-line and span backgrounds, clipped to the text viewport and drawn before glyphs. The consuming widget supplies syntax colors; the vendor layer does not parse code or Markdown. Code backgrounds must stay in the text layer: introducing a later renderer layer can cover glyphs.

TextInput ignores empty IME Commit events, matching TextEditor's cancellation handling. This prevents cancelling composition from deleting a selected table-cell value. The viewport-anchored input container is in `norito-iced-ext::inline_editor`, not in the vendor patch.

iced_tiny_skia 0.14.1 is patched in engine.rs: Editor ink always uses the local viewport clip mask, including full-damage redraws. Editor bounds are not ink bounds; partially visible lines otherwise paint outside the viewport and leave pixels that subsequent damage regions do not erase. A pixel regression test compares full and partial damage and rejects ink outside the viewport.

TextEditor block_backgrounds paints each contiguous line range as one rectangle, eliminating per-line raster seams. layout_theme applies variable line metrics during layout so anchored table inputs use the same coordinates as text and decorations in the first frame.

Editor render references retain a cached immutable Buffer snapshot. Previously an intervening layout invalidated a weak live-editor reference and cached render groups drew no text. The raster regression also mutates the live editor after collecting a render group; glyphs must still be present. Snapshots are shared within a frame and invalidated on editor mutation.

Cached text (`fill_text`) is anchored by its alignment: Center/Right shift the text left of the given position. Its visible bounds now place the text bounds at that anchor and intersect them with the local clip, and iced_tiny_skia clips ink to the same area whenever the measured text would exceed it. Previously the local clip was ignored when it lay inside the damage region, so overflowing, misanchored or partially scrolled text painted outside its viewport and left stale pixels on scroll. The pixel regression test in `norito-iced-ext` covers centered and overflowing cells cut by the viewport.
