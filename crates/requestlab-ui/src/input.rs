//! A real, editable text input built on GPUI's input handler.
//!
//! Supports single-line and multi-line modes, click and drag selection,
//! keyboard navigation, clipboard operations and IME composition. It is used by
//! every text field in the application: URL bar, headers, params, bodies,
//! environment variables and search boxes.

use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    actions, div, fill, point, px, relative, size, App, Bounds, ClipboardItem, Context,
    CursorStyle, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle,
    Focusable, GlobalElementId, InspectorElementId, IntoElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, Render, SharedString,
    Style, TextAlign, TextRun, UTF16Selection, Window,
};
use unicode_segmentation::UnicodeSegmentation;

actions!(
    requestlab_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Newline,
        ShowCharacterPalette,
    ]
);

/// Key context used to scope text-editing key bindings.
pub const INPUT_KEY_CONTEXT: &str = "RequestLabInput";

/// The key bindings every text input needs. Registered once at startup.
pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(INPUT_KEY_CONTEXT);
    vec![
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("ctrl-a", SelectAll, context),
        KeyBinding::new("ctrl-v", Paste, context),
        KeyBinding::new("ctrl-c", Copy, context),
        KeyBinding::new("ctrl-x", Cut, context),
        KeyBinding::new("ctrl-alt-space", ShowCharacterPalette, context),
    ]
}

struct Measured {
    lines: Vec<SharedString>,
    shapes: Vec<gpui::ShapedLine>,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
}

// ---------------------------------------------------------------- utilities
//
// Pure string geometry, kept as free functions so it can be unit tested
// without a window or an App context.

/// Split a byte offset into (line index, offset within that line).
pub fn line_col_at(content: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(content.len());
    let before = &content[..offset];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
    (line, offset - line_start)
}

/// The largest char boundary at or below `offset`.
pub fn floor_boundary_in(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while offset > 0 && !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The smallest char boundary at or above `offset`.
pub fn ceil_boundary_in(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while offset < content.len() && !content.is_char_boundary(offset) {
        offset += 1;
    }
    offset
}

/// Clamp a byte range onto `content`.
///
/// Ranges arrive from the platform (IME, accessibility, stale selection
/// snapshots) and may not describe this content at all.
pub fn clamp_range_in(content: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let start = floor_boundary_in(content, range.start);
    let end = ceil_boundary_in(content, range.end.max(start));
    start..end
}

/// Byte offset where `line_index` begins.
pub fn line_start_at(content: &str, line_index: usize) -> usize {
    if line_index == 0 {
        return 0;
    }
    content
        .match_indices('\n')
        .nth(line_index - 1)
        .map(|(index, _)| index + 1)
        .unwrap_or(content.len())
}

pub fn previous_grapheme_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .rev()
        .find_map(|(index, _)| (index < offset).then_some(index))
        .unwrap_or(0)
}

pub fn next_grapheme_boundary(content: &str, offset: usize) -> usize {
    content
        .grapheme_indices(true)
        .find_map(|(index, _)| (index > offset).then_some(index))
        .unwrap_or(content.len())
}

pub fn utf8_to_utf16_offset(content: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut utf8 = 0;
    for ch in content.chars() {
        if utf8 >= offset {
            break;
        }
        utf8 += ch.len_utf8();
        utf16 += ch.len_utf16();
    }
    utf16
}

pub fn utf16_to_utf8_offset(content: &str, offset: usize) -> usize {
    let mut utf16 = 0;
    let mut utf8 = 0;
    for ch in content.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += ch.len_utf16();
        utf8 += ch.len_utf8();
    }
    utf8
}

/// The editable input state.
pub struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    multiline: bool,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    is_selecting: bool,
    selection_color: gpui::Hsla,
    cursor_color: gpui::Hsla,
    placeholder_color: gpui::Hsla,
    last_measured: Option<Measured>,
}

impl TextInput {
    pub fn new(
        cx: &mut Context<Self>,
        placeholder: impl Into<SharedString>,
        multiline: bool,
    ) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            multiline,
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            is_selecting: false,
            selection_color: gpui::rgba(0x8CA9FF33).into(),
            cursor_color: gpui::rgb(0x8CA9FF).into(),
            placeholder_color: gpui::rgb(0x6C717B).into(),
            last_measured: None,
        }
    }

    pub fn with_colors(
        mut self,
        selection: gpui::Hsla,
        cursor: gpui::Hsla,
        placeholder: gpui::Hsla,
    ) -> Self {
        self.selection_color = selection;
        self.cursor_color = cursor;
        self.placeholder_color = placeholder;
        self
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    /// Replace the whole value, e.g. when loading a saved request.
    pub fn set_content(&mut self, content: impl Into<SharedString>) {
        self.content = content.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
    }

    pub fn clear(&mut self) {
        self.set_content("");
    }

    pub fn focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    // ------------------------------------------------------------- navigation

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = self.previous_boundary(self.cursor_offset());
            self.move_to(offset, cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let offset = self.next_boundary(self.cursor_offset());
            self.move_to(offset, cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            return;
        }
        self.move_vertical(-1, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            return;
        }
        self.move_vertical(1, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.cursor_offset();
        let line_start = self.content[..offset]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        self.move_to(line_start, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.cursor_offset();
        let line_end = self.content[offset..]
            .find('\n')
            .map(|index| offset + index)
            .unwrap_or(self.content.len());
        self.move_to(line_end, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_boundary(self.cursor_offset()), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = if self.multiline {
                text.replace("\r\n", "\n")
            } else {
                text.replace(['\n', '\r'], " ")
            };
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn move_vertical(&mut self, direction: i32, cx: &mut Context<Self>) {
        let Some(measured) = self.last_measured.as_ref() else {
            return;
        };
        let (line_index, column_bytes) = self.line_col(self.cursor_offset());
        let target_line = line_index as i32 + direction;
        if target_line < 0 {
            self.move_to(0, cx);
            return;
        }
        if target_line as usize >= measured.lines.len() {
            self.move_to(self.content.len(), cx);
            return;
        }
        let shapes = &measured.shapes;
        let lines = &measured.lines;
        let current_x = shapes
            .get(line_index)
            .map(|shape| shape.x_for_index(column_bytes))
            .unwrap_or_default();
        let target = target_line as usize;
        let offset_in_line = shapes
            .get(target)
            .map(|shape| shape.closest_index_for_x(current_x))
            .unwrap_or(0)
            .min(lines.get(target).map(|line| line.len()).unwrap_or(0));
        let line_start = self.line_start_offset(target);
        self.move_to(line_start + offset_in_line, cx);
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn line_start_offset(&self, line_index: usize) -> usize {
        if line_index == 0 {
            return 0;
        }
        self.content
            .match_indices('\n')
            .nth(line_index - 1)
            .map(|(index, _)| index + 1)
            .unwrap_or(self.content.len())
    }

    /// Byte offset of the cursor, split into line index and offset within it.
    fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = floor_boundary_in(&self.content, offset);
        let before = &self.content[..offset];
        let line_index = before.matches('\n').count();
        let line_start = before.rfind('\n').map(|index| index + 1).unwrap_or(0);
        (line_index, offset - line_start)
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(measured) = self.last_measured.as_ref() else {
            return 0;
        };
        if position.y < measured.bounds.top() {
            return 0;
        }
        let relative_y = position.y - measured.bounds.top();
        let line_index = ((relative_y / measured.line_height) as usize)
            .min(measured.lines.len().saturating_sub(1));
        let Some(shape) = measured.shapes.get(line_index) else {
            return self.content.len();
        };
        let x = (position.x - measured.bounds.left()).max(px(0.0));
        let offset_in_line = shape
            .closest_index_for_x(x)
            .min(measured.lines[line_index].len());
        self.line_start_offset(line_index) + offset_in_line
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.is_selecting = true;
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.move_to(offset, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.index_for_mouse_position(event.position);
            self.select_to(offset, cx);
        }
    }

    // ----------------------------------------------------------- conversions

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }
        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;
        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }
        utf16_offset
    }

    fn clamp_range(&self, range: Range<usize>) -> Range<usize> {
        clamp_range_in(&self.content, range)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(index, _)| (index < offset).then_some(index))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(index, _)| (index > offset).then_some(index))
            .unwrap_or(self.content.len())
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.clamp_range(self.range_from_utf16(&range_utf16));
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let range = self.clamp_range(range);

        let new_text = if self.multiline {
            new_text.to_string()
        } else {
            new_text.replace(['\n', '\r'], " ")
        };

        self.content =
            (self.content[0..range.start].to_owned() + &new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_range.clone());
        let range = self.clamp_range(range);

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        if new_text.is_empty() {
            self.marked_range = None;
        } else {
            self.marked_range = Some(range.start..range.start + new_text.len());
        }
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .map(|new_range| {
                range.start + new_range.start..range.start + new_range.end.min(new_text.len())
            })
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let measured = self.last_measured.as_ref()?;
        let range = self.clamp_range(self.range_from_utf16(&range_utf16));
        let (line_index, _) = self.line_col(range.start);
        let shape = measured.shapes.get(line_index)?;
        let line_start = self.line_start_offset(line_index);
        let start_x = shape.x_for_index(range.start.saturating_sub(line_start));
        let end_x = shape.x_for_index(range.end.saturating_sub(line_start));
        let top = bounds.top() + measured.line_height * line_index as f32;
        Some(Bounds::from_corners(
            point(bounds.left() + start_x, top),
            point(bounds.left() + end_x, top + measured.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let offset = self.index_for_mouse_position(point);
        Some(self.offset_to_utf16(offset))
    }
}

struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    shapes: Vec<gpui::ShapedLine>,
    selection: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    measured: Measured,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        // An empty multiline field paints its placeholder, so the height has to
        // fit the placeholder's lines too.
        let shown = if input.content.is_empty() {
            input.placeholder.as_ref()
        } else {
            input.content.as_ref()
        };
        let line_count = if input.multiline {
            shown.matches('\n').count() + 1
        } else {
            1
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (window.line_height() * line_count.max(1)).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let style = window.text_style();
        let font = style.font();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let selection = input.selected_range.clone();
        let cursor_position = input.cursor_offset();
        // A focused empty field shows the caret, so the placeholder must step
        // aside: otherwise there is no typing indicator at all.
        let focused = input.focus_handle.is_focused(window);

        let (lines, show_placeholder): (Vec<SharedString>, bool) = if content.is_empty() {
            if focused {
                (vec![SharedString::default()], false)
            } else {
                // The placeholder may span several lines (a JSON body hint, for
                // example) and each shape_line call must receive a single line.
                (
                    input
                        .placeholder
                        .split('\n')
                        .map(|line| SharedString::from(line.to_string()))
                        .collect(),
                    true,
                )
            }
        } else {
            (
                content
                    .split('\n')
                    .map(|line| SharedString::from(line.to_string()))
                    .collect(),
                false,
            )
        };

        let text_color = if show_placeholder {
            input.placeholder_color
        } else {
            style.color
        };

        let mut shapes = Vec::with_capacity(lines.len());
        for line in &lines {
            let run = TextRun {
                len: line.len(),
                font: font.clone(),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            shapes.push(
                window
                    .text_system()
                    .shape_line(line.clone(), font_size, &[run], None),
            );
        }

        let mut selection_quads = Vec::new();
        if !selection.is_empty() {
            let mut line_start = 0usize;
            for (index, line) in lines.iter().enumerate() {
                let line_end = line_start + line.len();
                // A line is touched when the selection reaches into it or spans
                // its trailing newline.
                let touched =
                    selection.start < line_end.max(line_start + 1) && selection.end > line_start;
                if touched {
                    if let Some(shape) = shapes.get(index) {
                        let from = selection.start.max(line_start).min(line_end);
                        let to = selection.end.min(line_end).max(from);
                        let start_x = shape.x_for_index(from - line_start);
                        let width = if to > from {
                            shape.x_for_index(to - line_start) - start_x
                        } else {
                            px(6.0)
                        };
                        let top = bounds.top() + line_height * index as f32;
                        selection_quads.push(fill(
                            Bounds::new(
                                point(bounds.left() + start_x, top),
                                size(width, line_height),
                            ),
                            input.selection_color,
                        ));
                    }
                }
                line_start = line_end + 1;
            }
        }

        let cursor = if show_placeholder {
            None
        } else {
            let (line_index, column) = line_col_at(&content, cursor_position);
            shapes.get(line_index).map(|shape| {
                let x = shape.x_for_index(column);
                let top = bounds.top() + line_height * line_index as f32;
                fill(
                    Bounds::new(point(bounds.left() + x, top), size(px(1.5), line_height)),
                    input.cursor_color,
                )
            })
        };

        PrepaintState {
            shapes,
            selection: selection_quads,
            cursor,
            measured: Measured {
                lines,
                shapes: Vec::new(),
                bounds,
                line_height,
            },
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }

        let line_height = window.line_height();
        let shapes = std::mem::take(&mut prepaint.shapes);
        for (index, shape) in shapes.iter().enumerate() {
            let origin = point(bounds.left(), bounds.top() + line_height * index as f32);
            let _ = shape.paint(origin, line_height, TextAlign::Left, None, window, cx);
        }

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        let measured = Measured {
            lines: std::mem::take(&mut prepaint.measured.lines),
            shapes,
            bounds: prepaint.measured.bounds,
            line_height: prepaint.measured.line_height,
        };
        self.input.update(cx, |input, _cx| {
            input.last_measured = Some(measured);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .key_context(INPUT_KEY_CONTEXT)
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .w_full()
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(TextElement { input: cx.entity() })
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_boundaries_move_by_grapheme() {
        let content = "héllo";
        assert_eq!(previous_grapheme_boundary(content, 3), 1);
        assert_eq!(next_grapheme_boundary(content, 1), 3);
        assert_eq!(next_grapheme_boundary(content, 6), 6);
        assert_eq!(previous_grapheme_boundary(content, 0), 0);
    }

    #[test]
    fn line_geometry_is_computed_from_content() {
        let content = "one\ntwo\nthree";
        assert_eq!(line_col_at(content, 0), (0, 0));
        assert_eq!(line_col_at(content, 4), (1, 0));
        assert_eq!(line_col_at(content, 6), (1, 2));
        assert_eq!(line_col_at(content, 9), (2, 1));
        assert_eq!(line_col_at(content, 8), (2, 0));
        assert_eq!(line_start_at(content, 0), 0);
        assert_eq!(line_start_at(content, 1), 4);
        assert_eq!(line_start_at(content, 2), 8);
    }

    #[test]
    fn out_of_range_platform_ranges_do_not_panic() {
        // GPUI hands the input handler ranges taken from its own, possibly
        // stale, selection snapshot; an unclamped range used to crash the app.
        assert_eq!(clamp_range_in("", 0..2), 0..0);
        assert_eq!(clamp_range_in("", 5..9), 0..0);
        assert_eq!(clamp_range_in("héllo", 0..3), 0..3);
        assert_eq!(clamp_range_in("héllo", 2..4), 1..4);
        assert_eq!(clamp_range_in("héllo", 4..99), 4..6);
    }

    #[test]
    fn utf16_conversions_handle_multibyte_text() {
        let content = "aé😀b";
        assert_eq!(utf8_to_utf16_offset(content, 1), 1);
        assert_eq!(utf8_to_utf16_offset(content, 3), 2);
        assert_eq!(utf8_to_utf16_offset(content, 7), 4);
        assert_eq!(utf16_to_utf8_offset(content, 4), 7);
    }

    #[test]
    fn bindings_cover_editing_keys() {
        let bindings = key_bindings();
        assert!(bindings.len() >= 14);
    }
}
