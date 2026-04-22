use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Widget, Wrap,
};
use ratatui::Frame;

use super::app::{format_task_detail, ActiveView, AgentStatus, App, Focus, VIEW_ENTRIES};
use super::blocks::{block_fingerprint, AnchorScratch, ChatButton, ScrollAnchor};
use super::tasks_view::{FormField, TaskForm, TaskFormKind, TaskRow, TasksPrompt, TasksScreen};

const APP_NAME: &str = "aciddog";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(f: &mut Frame, app: &mut App) {
    // Fresh frame: reset interactive regions.
    app.layout.chat_buttons.clear();
    app.layout.task_form_fields.clear();
    app.layout.task_form_picker_rows.clear();
    app.layout.task_form_picker_outer = None;
    app.layout.task_form_picker_buttons.clear();
    app.layout.child_overlay_rows.clear();
    app.layout.child_overlay_outer = None;

    // Input height: in Chat view, grow from 3 to 7 rows (1–5 usable +
    // 2 for borders) based on the wrapped line count of the current
    // chat's input buffer. Tasks view keeps the fixed 3 rows so
    // `render_tasks_input` is unaffected. Channel view uses the same
    // fixed 3 rows as Tasks — the channel input is one-line-ish for v1.
    let input_rows = match app.active_view {
        ActiveView::Chat => {
            let inner_width = f.area().width.saturating_sub(2);
            let usable = app
                .current_chat()
                .map(|c| {
                    let rows = c.input_visual_rows(inner_width);
                    (rows.len() as u16).clamp(1, 5)
                })
                .unwrap_or(1);
            usable + 2
        }
        ActiveView::Tasks => 3,
        ActiveView::Channel(_) => 3,
    };

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),          // title
            Constraint::Min(10),            // chat + agents
            Constraint::Length(input_rows), // input
            Constraint::Length(1),          // status
        ])
        .split(f.area());

    let title_area = root[0];
    let body_area = root[1];
    let chat_input_area = root[2];
    let status_area = root[3];

    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(body_area);

    let main_area = body_cols[0];
    let right_col = body_cols[1];

    // Right column: Views list on top (small), Agents in the middle,
    // Channels list at the bottom (if any channels are declared).
    // Each list's height = border (2) + one line per entry. Agents
    // takes whatever's left.
    let views_height = (VIEW_ENTRIES.len() as u16).saturating_add(2).max(3);
    let channels_len = app.channels.len() as u16;
    let channels_height = if channels_len > 0 {
        channels_len.saturating_add(2).max(3)
    } else {
        0
    };
    let right_rows = if channels_height > 0 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(views_height),
                Constraint::Min(5),
                Constraint::Length(channels_height),
            ])
            .split(right_col)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(views_height), Constraint::Min(5)])
            .split(right_col)
    };
    let views_area = right_rows[0];
    let agents_area = right_rows[1];
    let channels_area = if channels_height > 0 {
        Some(right_rows[2])
    } else {
        None
    };

    render_title(f, title_area, app);

    let main_inner = match app.active_view {
        ActiveView::Chat => render_agent_chat(f, main_area, app),
        ActiveView::Tasks => render_tasks_screen(f, main_area, app),
        ActiveView::Channel(idx) => render_channel_chat(f, main_area, app, idx),
    };
    let views_inner = render_views_list(f, views_area, app);
    let list_inner = render_agent_list(f, agents_area, app);
    let channels_inner = channels_area.map(|a| render_channels_list(f, a, app));
    let input_inner = render_chat_input(f, chat_input_area, app);
    render_status(f, status_area, app);

    // Modal overlays (only meaningful in Tasks view).
    if app.active_view == ActiveView::Tasks {
        if app.tasks_screen.form.is_some() {
            render_task_form(f, main_area, app);
        } else if let Some(id) = app.tasks_screen.detail {
            render_task_detail(f, main_area, &*app, id);
        } else if app.tasks_screen.child_overlay.is_some() {
            render_child_overlay(f, main_area, app);
        }
    }

    app.layout.agent_list_inner = Some(list_inner);
    app.layout.chat_history_inner = Some(main_inner);
    app.layout.chat_input_inner = Some(input_inner);
    app.layout.views_list_inner = Some(views_inner);
    app.layout.channels_list_inner = channels_inner;
}

fn focus_border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_title(f: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(
            format!(" {APP_NAME} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  v{VERSION}  ")),
        Span::styled(
            format!("team: {}", app.config.team.id),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("({})", app.config.team.description),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]);
    f.render_widget(Paragraph::new(title), area);
}

fn render_agent_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Agents;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" agents ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);

    // The "selected agent" highlight only marks who's currently
    // rendered in the main area. When the user is looking at Tasks
    // or a Channel the agent list shouldn't claim one of its rows
    // as active — only the sidebar panel whose content is currently
    // displayed gets the persistent highlight. The keyboard cursor
    // (focus) still shows while navigating with j/k.
    let active_here = app.active_view == ActiveView::Chat;

    let order = app.visible_order();
    let items: Vec<ListItem> = order
        .iter()
        .map(|&(i, depth)| {
            let c = &app.chats[i];
            let dot = match c.status {
                AgentStatus::Ready => "🟢",
                AgentStatus::Working => "🟡",
                AgentStatus::Error => "🔴",
                AgentStatus::Hibernated => "💤",
            };
            let name_style = if i == app.selected && (active_here || focused) {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut spans: Vec<Span> = Vec::with_capacity(4);
            if depth > 0 {
                // Indent once per ancestor, with a single "↳ " on the
                // last level so the reader can see which row is the
                // child entry.
                if depth > 1 {
                    spans.push(Span::raw("  ".repeat(depth - 1)));
                }
                spans.push(Span::styled("↳ ", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::raw(format!("{dot} ")));
            spans.push(Span::styled(c.display_name.clone(), name_style));
            if c.parent_idx.is_some() {
                spans.push(Span::styled(
                    " [worker]",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);

    let mut state = ListState::default();
    if let Some(sel_row) = order.iter().position(|(i, _)| *i == app.selected) {
        state.select(Some(sel_row));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_agent_chat(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Chat;
    let selected_agent = app.selected;

    let title = match app.chats.get(selected_agent) {
        Some(chat) => {
            // Show role/model for configured agents; for workers,
            // fall back to "worker · <template>" since the chat
            // itself carries the display name and template, not the
            // role/model pair from an `agent` config block.
            if let Some(template) = &chat.worker_template {
                if let Some(w) = app.config.worker(template) {
                    let model = app
                        .config
                        .effective_ai_core(w.ai_core.as_deref())
                        .and_then(|c| c.runtime_model())
                        .unwrap_or(&w.model);
                    format!(" {} · worker · {} · {} ", chat.display_name, w.role, model)
                } else {
                    format!(" {} · worker ", chat.display_name)
                }
            } else if let Some(a) = app.config.agents.get(selected_agent) {
                let model = app
                    .config
                    .effective_ai_core(a.ai_core.as_deref())
                    .and_then(|c| c.runtime_model())
                    .unwrap_or(&a.model);
                format!(" {} · {} · {} ", a.id, a.role, model)
            } else {
                format!(" {} ", chat.display_name)
            }
        }
        None => " chat ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split borrows so we can mutate both the current chat and the button list.
    let frame_tick = app.frame_tick;
    let App {
        chats,
        layout,
        config,
        ..
    } = app;
    let Some(chat) = chats.get_mut(selected_agent) else {
        let para = Paragraph::new("no agents defined in aciddog.wcl")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(para, inner);
        return inner;
    };

    // Append a virtual 1-row "thinking" entry after the real blocks
    // while this chat is `Working`. Not inserted into `chat.blocks` so
    // history, selection, and copy logic are untouched; anchor math
    // treats it as a trailing block of height 1 so sticky-bottom works.
    let thinking_visible =
        chat.status == AgentStatus::Working && !config.thinking_messages.is_empty();

    if chat.blocks.is_empty() && !thinking_visible {
        return inner;
    }

    // Lazy per-index height cache. The old eager
    // `blocks.iter().map(height).collect()` cost O(total blocks) in
    // wrap math per frame — painful once a long-running chat
    // accumulated hundreds of blocks and the 10 Hz thinking ticker
    // kept it redrawing. Both `reconcile_anchor` and the render loop
    // only touch O(visible) indices in practice (sticky-bottom walks
    // back from the last block; the render loop stops as soon as it
    // fills the viewport), so computing on demand keeps per-frame
    // work bounded by viewport height.
    let real_block_count = chat.blocks.len();
    let mut heights = LazyHeights::new(&chat.blocks, inner.width, thinking_visible);

    reconcile_anchor(
        &mut chat.anchor,
        chat.sticky_bottom,
        chat.blocks.len(),
        &mut heights,
        inner.height,
    );

    let start_idx = chat.anchor.block_idx;
    let within = chat.anchor.within;
    let mut y = inner.y;
    let end_y = inner.y.saturating_add(inner.height);

    let total_rows = heights.len();
    for i in start_idx..total_rows {
        if y >= end_y {
            break;
        }
        let h = heights.get(i);
        let is_anchor = i == start_idx && within > 0 && within < h;
        let natural_h = if is_anchor { h - within } else { h };
        let visible_h = natural_h.min(end_y - y);
        if visible_h == 0 {
            continue;
        }

        if is_anchor {
            // Full-block render into a scratch buffer so we can blit
            // rows [within, within + visible_h) to the frame at
            // (inner.x, y). Cached on the chat — re-rendering a 500-row
            // expanded prompt each mouse-wheel tick pegs the UI thread
            // in debug builds, so we only re-render when dimensions or
            // content actually change.
            if i < real_block_count {
                let fingerprint = block_fingerprint(&chat.blocks[i]);
                let cache_hit = chat
                    .anchor_scratch
                    .as_ref()
                    .is_some_and(|c| {
                        c.block_idx == i
                            && c.width == inner.width
                            && c.height == h
                            && c.fingerprint == fingerprint
                    });
                if !cache_hit {
                    let scratch_area = Rect::new(0, 0, inner.width, h);
                    let mut scratch = Buffer::empty(scratch_area);
                    let mut scratch_btns: Vec<ChatButton> = Vec::new();
                    let selected = chat.selected_block == Some(i);
                    chat.blocks[i].render(
                        &mut scratch,
                        scratch_area,
                        &mut scratch_btns,
                        i,
                        selected,
                    );
                    chat.anchor_scratch = Some(AnchorScratch {
                        block_idx: i,
                        width: inner.width,
                        height: h,
                        fingerprint,
                        buffer: scratch,
                        buttons: scratch_btns,
                    });
                }
                let cache = chat.anchor_scratch.as_ref().expect("just populated above");
                blit_rows(&cache.buffer, within, visible_h, f.buffer_mut(), inner.x, y);
                translate_visible_buttons(
                    &cache.buttons,
                    within,
                    visible_h,
                    inner.x,
                    y,
                    &mut layout.chat_buttons,
                );
            } else {
                // Thinking row animates via `frame_tick`, so caching
                // would freeze the spinner. Render straight into a
                // one-shot scratch buffer instead.
                let scratch_area = Rect::new(0, 0, inner.width, h);
                let mut scratch = Buffer::empty(scratch_area);
                render_thinking_row(
                    &mut scratch,
                    scratch_area,
                    &config.thinking_messages,
                    chat.thinking_message_idx,
                    frame_tick,
                );
                blit_rows(&scratch, within, visible_h, f.buffer_mut(), inner.x, y);
            }
        } else {
            let rect = Rect::new(inner.x, y, inner.width, visible_h);
            if i < real_block_count {
                let selected = chat.selected_block == Some(i);
                chat.blocks[i].render(f.buffer_mut(), rect, &mut layout.chat_buttons, i, selected);
            } else {
                render_thinking_row(
                    f.buffer_mut(),
                    rect,
                    &config.thinking_messages,
                    chat.thinking_message_idx,
                    frame_tick,
                );
            }
        }

        y = y.saturating_add(visible_h);
    }
    inner
}

/// Render a single channel's transcript in the main area. Much
/// simpler than `render_agent_chat` — channel blocks don't nest or
/// collapse, there are no tool blocks or thinking indicator, and
/// only one scroll anchor matters: sticky-bottom. For v1 we just
/// render the last N blocks that fit vertically, bottom-aligned.
fn render_channel_chat(f: &mut Frame, area: Rect, app: &mut App, idx: usize) -> Rect {
    let focused = app.focus == Focus::Chat;
    let title = match app.channels.get(idx) {
        Some(c) => format!(" #{} ", c.id),
        None => " channel ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(ch) = app.channels.get(idx) else {
        return inner;
    };
    if inner.width == 0 || inner.height == 0 || ch.blocks.is_empty() {
        return inner;
    }

    // Find the window of blocks that fits at the bottom of the
    // viewport, computing heights lazily from the end. Historically
    // this pass computed heights for the whole transcript up-front,
    // which turned into a per-frame O(N) wrap-math tax once a channel
    // loaded its full 500-line tail from disk — noticeable as lag
    // whenever the ticker fires at 10 Hz for a Working agent.
    let mut window: Vec<(usize, u16)> = Vec::new();
    let mut total_rows: u32 = 0;
    for i in (0..ch.blocks.len()).rev() {
        let h = ch.blocks[i].height(inner.width);
        total_rows = total_rows.saturating_add(h as u32);
        window.push((i, h));
        if total_rows >= inner.height as u32 {
            break;
        }
    }
    window.reverse();

    // If the visible run overflows the viewport, clip rows off the
    // top of the first block so the last row aligns to the bottom.
    let mut clip_rows = if total_rows > inner.height as u32 {
        (total_rows - inner.height as u32) as u16
    } else {
        0
    };
    let mut y = inner.y;
    let end_y = inner.y.saturating_add(inner.height);
    let mut tmp_buttons: Vec<ChatButton> = Vec::new();
    for &(block_idx, h) in &window {
        if y >= end_y {
            break;
        }
        // By construction clip_rows < heights[first] once we reach
        // here (we stopped accumulating as soon as we filled the
        // viewport), so at most the first block gets a partial crop.
        let real_h = h.saturating_sub(clip_rows);
        if real_h == 0 {
            continue;
        }
        let visible_h = real_h.min(end_y - y);
        if clip_rows > 0 {
            let scratch_area = Rect::new(0, 0, inner.width, h);
            let mut scratch = Buffer::empty(scratch_area);
            ch.blocks[block_idx].render(&mut scratch, scratch_area, &mut tmp_buttons, 0, false);
            blit_rows(&scratch, clip_rows, visible_h, f.buffer_mut(), inner.x, y);
            clip_rows = 0;
        } else {
            let rect = Rect::new(inner.x, y, inner.width, visible_h);
            ch.blocks[block_idx].render(f.buffer_mut(), rect, &mut tmp_buttons, 0, false);
        }
        y = y.saturating_add(visible_h);
    }
    inner
}

/// Copy `rows` rows (starting at `y_from` in `src`) to `dst` at `(dst_x, dst_y)`.
/// Clips to both buffers' areas via `cell` / `cell_mut`.
fn blit_rows(src: &Buffer, y_from: u16, rows: u16, dst: &mut Buffer, dst_x: u16, dst_y: u16) {
    let width = src.area.width;
    for dy in 0..rows {
        let sy = src.area.y.saturating_add(y_from).saturating_add(dy);
        let ty = dst_y.saturating_add(dy);
        for dx in 0..width {
            let sx = src.area.x.saturating_add(dx);
            let tx = dst_x.saturating_add(dx);
            let Some(cell) = src.cell((sx, sy)).cloned() else {
                continue;
            };
            if let Some(slot) = dst.cell_mut((tx, ty)) {
                *slot = cell;
            }
        }
    }
}

/// Translate buttons registered against a scratch buffer at `(0, 0)` into
/// frame coordinates at `(dst_x, dst_y)`, keeping only those whose row falls
/// inside the visible slice `[skip_y, skip_y + visible_h)`.
fn translate_visible_buttons(
    src: &[ChatButton],
    skip_y: u16,
    visible_h: u16,
    dst_x: u16,
    dst_y: u16,
    out: &mut Vec<ChatButton>,
) {
    let view_start = skip_y;
    let view_end = skip_y.saturating_add(visible_h);
    for b in src {
        let b_top = b.rect.y;
        let b_bot = b.rect.y.saturating_add(b.rect.height);
        if b_bot <= view_start || b_top >= view_end {
            continue;
        }
        // Clip the button rect to the visible slice, then shift into frame coords.
        let clipped_top = b_top.max(view_start);
        let clipped_bot = b_bot.min(view_end);
        let new_y = dst_y.saturating_add(clipped_top - view_start);
        let new_h = clipped_bot - clipped_top;
        let mut translated = b.clone();
        translated.rect =
            Rect::new(b.rect.x.saturating_add(dst_x), new_y, b.rect.width, new_h);
        out.push(translated);
    }
}

const THINKING_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn render_thinking_row(
    buf: &mut Buffer,
    area: Rect,
    pool: &[String],
    idx: Option<usize>,
    frame_tick: u64,
) {
    if pool.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let spinner = THINKING_SPINNER_FRAMES[(frame_tick as usize) % THINKING_SPINNER_FRAMES.len()];
    let message_idx = idx.unwrap_or(0).min(pool.len().saturating_sub(1));
    let message = pool[message_idx].as_str();
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            spinner,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{message}…"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]);
    Paragraph::new(line).render(area, buf);
}

/// Make sure the scroll anchor points somewhere sensible given the block
/// list and viewport height. Handles sticky-bottom and clamps the anchor
/// inside valid bounds.
///
/// Takes the anchor + sticky flag + block count as separate args
/// rather than `&mut AgentChat` so the caller can hold an
/// immutable borrow of `chat.blocks` (via `LazyHeights`) at the
/// same time. Uses `LazyHeights` so callers pay only for the
/// block heights this function actually inspects — the
/// sticky-bottom walk touches O(visible) entries from the end,
/// and the anchor roll-forward usually touches one.
fn reconcile_anchor(
    anchor: &mut ScrollAnchor,
    sticky_bottom: bool,
    blocks_len: usize,
    heights: &mut LazyHeights<'_>,
    viewport_h: u16,
) {
    if blocks_len == 0 || viewport_h == 0 {
        *anchor = ScrollAnchor::default();
        return;
    }

    let last = blocks_len - 1;

    // Clamp anchor to valid block index.
    if anchor.block_idx > last {
        anchor.block_idx = last;
        anchor.within = 0;
    }

    // Roll forward if `within` has overshot the current block's height.
    loop {
        let h = heights.get(anchor.block_idx);
        if anchor.within < h {
            break;
        }
        if anchor.block_idx == last {
            anchor.within = h.saturating_sub(1);
            break;
        }
        anchor.within -= h;
        anchor.block_idx += 1;
    }

    if sticky_bottom {
        // Pick the anchor so the last block ends flush with the viewport's bottom.
        let mut remaining = viewport_h;
        let mut idx = last;
        loop {
            let h = heights.get(idx);
            if h >= remaining {
                anchor.block_idx = idx;
                anchor.within = h - remaining;
                return;
            }
            remaining -= h;
            if idx == 0 {
                anchor.block_idx = 0;
                anchor.within = 0;
                return;
            }
            idx -= 1;
        }
    }
}

/// Per-index height cache used by the agent-chat renderer and
/// `reconcile_anchor`. Heights are computed on first `get()` call
/// for an index and memoized for the rest of the frame. The
/// `thinking_visible` flag bumps the virtual length by one so
/// callers can treat the animated thinking row as a trailing
/// block of height 1 without having to special-case it everywhere.
struct LazyHeights<'a> {
    blocks: &'a [super::blocks::ChatBlock],
    width: u16,
    cache: Vec<Option<u16>>,
    thinking_visible: bool,
}

impl<'a> LazyHeights<'a> {
    fn new(blocks: &'a [super::blocks::ChatBlock], width: u16, thinking_visible: bool) -> Self {
        Self {
            blocks,
            width,
            cache: vec![None; blocks.len()],
            thinking_visible,
        }
    }

    fn len(&self) -> usize {
        self.blocks.len() + if self.thinking_visible { 1 } else { 0 }
    }

    fn get(&mut self, i: usize) -> u16 {
        // Thinking row (or out-of-bounds query on an empty chat):
        // always height 1, matches the old eager `heights.push(1)`.
        if i >= self.blocks.len() {
            return 1;
        }
        if let Some(h) = self.cache[i] {
            return h;
        }
        let h = self.blocks[i].height(self.width).max(1);
        self.cache[i] = Some(h);
        h
    }
}

fn render_chat_input(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    // In Tasks view the bottom box does double duty: it shows
    // operation hints when the screen is idle, field-nav hints when
    // a form is open, and the current input buffer when a
    // single-line prompt (note body / delete confirmation) is
    // active.
    if app.active_view == ActiveView::Tasks {
        return render_tasks_input(f, area, app);
    }
    if let ActiveView::Channel(idx) = app.active_view {
        return render_channel_input(f, area, app, idx);
    }

    let focused = app.focus == Focus::Chat;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" input ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);

    let Some(chat) = app.current_chat_mut() else {
        f.render_widget(Paragraph::new("").block(block), area);
        return inner;
    };

    // Cache the current layout on the chat so key handlers wrap the
    // buffer the same way the renderer does.
    chat.input_wrap_width = inner.width;
    chat.input_visible_rows = inner.height;

    // Unfocused + empty: show placeholder on a single line.
    if !focused && chat.input.is_empty() {
        let placeholder = Line::from(Span::styled(
            "click here or press Tab to chat",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
        f.render_widget(Paragraph::new(placeholder).block(block), area);
        return inner;
    }

    let rows = chat.input_visual_rows(inner.width);
    let total = rows.len() as u16;
    let visible = inner.height.max(1);
    let max_scroll = total.saturating_sub(visible);
    if chat.input_scroll > max_scroll {
        chat.input_scroll = max_scroll;
    }
    let scroll = chat.input_scroll;

    // Compute the caret's (row, col) so we can inject the cursor span.
    let (cursor_row, _cursor_row_start, cursor_col) = chat.input_cursor_visual(inner.width);

    let first = scroll as usize;
    let last = (first + visible as usize).min(rows.len());
    let mut lines: Vec<Line> = Vec::with_capacity(last - first);
    for (i, &(r_start, r_end)) in rows[first..last].iter().enumerate() {
        let row_idx = first + i;
        let row_text = &chat.input[r_start..r_end];
        if focused && row_idx == cursor_row {
            // Split the row text at the caret column (in chars) and
            // inject the caret glyph between the two spans.
            let char_count = row_text.chars().count();
            let caret_col = cursor_col.min(char_count);
            let caret_byte: usize = row_text
                .char_indices()
                .nth(caret_col)
                .map(|(off, _)| off)
                .unwrap_or(row_text.len());
            let before = &row_text[..caret_byte];
            let after = &row_text[caret_byte..];
            lines.push(Line::from(vec![
                Span::raw(before.to_string()),
                Span::styled(
                    "▏",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(after.to_string()),
            ]));
        } else {
            lines.push(Line::from(Span::raw(row_text.to_string())));
        }
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);

    // Vertical scrollbar when content overflows the visible rows.
    if total > visible {
        let mut sb_state = ScrollbarState::new(total as usize)
            .viewport_content_length(visible as usize)
            .position(scroll as usize);
        let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);
        let sb_area = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };
        f.render_stateful_widget(sb, sb_area, &mut sb_state);
    }

    inner
}

/// Input box rendered under a channel view. Simpler than the
/// agent-chat input — single-line feel with a caret and an obvious
/// placeholder. Multi-line input is still possible via Shift+Enter
/// but we don't grow the box for v1.
fn render_channel_input(f: &mut Frame, area: Rect, app: &App, idx: usize) -> Rect {
    let focused = app.focus == Focus::Chat;
    let Some(ch) = app.channels.get(idx) else {
        let block = Block::default().borders(Borders::ALL).title(" input ");
        let inner = block.inner(area);
        f.render_widget(block, area);
        return inner;
    };
    let title = format!(" message #{} ", ch.id);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);

    if !focused && ch.input.is_empty() {
        let placeholder = Line::from(Span::styled(
            "click here or press Tab to talk in the channel",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ));
        f.render_widget(Paragraph::new(placeholder).block(block), area);
        return inner;
    }

    // Single-line caret injection. Build a Line by splitting the
    // input at the byte cursor.
    let (before, after) = ch.input.split_at(ch.input_cursor.min(ch.input.len()));
    let mut spans = vec![Span::raw(before.to_string())];
    if focused {
        spans.push(Span::styled(
            "▏",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::raw(after.to_string()));
    let para = Paragraph::new(Line::from(spans))
        .block(block)
        .wrap(Wrap { trim: false });
    f.render_widget(para, area);
    inner
}

fn render_tasks_input(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Tasks;
    let (title, line) = if let Some(prompt) = &app.tasks_screen.prompt {
        match prompt {
            TasksPrompt::Note { body, .. } => (
                " note body ".to_string(),
                Line::from(vec![
                    Span::raw(body.clone()),
                    Span::styled(
                        "▏",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ),
            TasksPrompt::ConfirmDelete { .. } => (
                " delete? ".to_string(),
                Line::from(Span::styled(
                    "press y to delete, n or esc to cancel",
                    Style::default().fg(Color::Yellow),
                )),
            ),
        }
    } else if let Some(form) = app.tasks_screen.form.as_ref() {
        let hint = if form.picker.is_some() {
            "↑/↓ nav · enter pick · esc cancel"
        } else if form.blocked_by_picker.is_some() {
            "↑/↓ nav · space toggle · enter save · esc cancel"
        } else {
            "tab/↓ next · shift-tab/↑ prev · enter submit (or open dropdown/list) · esc cancel"
        };
        (
            " form ".to_string(),
            Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))),
        )
    } else if let Some(msg) = &app.tasks_screen.status_msg {
        (
            " status ".to_string(),
            Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Yellow),
            )),
        )
    } else {
        (
            " tasks ".to_string(),
            Line::from(Span::styled(
                "a add · e edit · d del · t take · n note · s status · ⏎ detail · esc back",
                Style::default().fg(Color::DarkGray),
            )),
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(Paragraph::new(line).block(block), area);
    inner
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let focus_label = match app.focus {
        Focus::Agents => "agents",
        Focus::Chat => match app.active_view {
            ActiveView::Channel(_) => "channel",
            _ => "chat",
        },
        Focus::Views => "views",
        Focus::Tasks => "tasks",
        Focus::Channels => "channels",
    };
    let mut hint_spans: Vec<Span> = match app.focus {
        Focus::Agents => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(" navigate  "),
            Span::styled("tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next panel  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ],
        Focus::Chat => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("enter", Style::default().fg(Color::Cyan)),
            Span::raw(" send  "),
            Span::styled("shift/ctrl-j+⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" newline  "),
            Span::styled("alt+↑↓", Style::default().fg(Color::Cyan)),
            Span::raw(" select  "),
            Span::styled("alt+⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" toggle  "),
            Span::styled("alt+c", Style::default().fg(Color::Cyan)),
            Span::raw(" copy  "),
            Span::styled("ctrl-c", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ],
        Focus::Views => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(" nav  "),
            Span::styled("⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" activate  "),
            Span::styled("tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next panel"),
        ],
        Focus::Tasks => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(" add  "),
            Span::styled("e", Style::default().fg(Color::Cyan)),
            Span::raw(" edit  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" del  "),
            Span::styled("t", Style::default().fg(Color::Cyan)),
            Span::raw(" take  "),
            Span::styled("n", Style::default().fg(Color::Cyan)),
            Span::raw(" note  "),
            Span::styled("s", Style::default().fg(Color::Cyan)),
            Span::raw(" status  "),
            Span::styled("⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" detail  "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
        Focus::Channels => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(" nav  "),
            Span::styled("⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" open  "),
            Span::styled("tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next panel"),
        ],
    };

    if app.copied_recently() {
        hint_spans.push(Span::raw("   "));
        hint_spans.push(Span::styled(
            "✓ copied",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(hint_spans)).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

fn render_views_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Views;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" views ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    let items: Vec<ListItem> = VIEW_ENTRIES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let active = matches!((*name, app.active_view), ("Tasks", ActiveView::Tasks));
            // Persistent highlight on the currently-displayed view so
            // clicking it gives visible feedback even once focus
            // leaves the panel (mirrors the agent list's always-on
            // selected-agent highlight). The same highlight tracks
            // the keyboard cursor when the panel is focused.
            let style = if active || (focused && i == app.selected_view) {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled((*name).to_string(), style)))
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    state.select(Some(app.selected_view));
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_channels_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Channels;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" channels ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    let items: Vec<ListItem> = app
        .channels
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let active = matches!(app.active_view, ActiveView::Channel(j) if j == i);
            // Persistent highlight on the currently-open channel so
            // clicking it gives visible feedback once focus leaves
            // the panel (mirrors the agent list's always-on selected
            // highlight). The same highlight tracks the keyboard
            // cursor when the panel is focused.
            let style = if active || (focused && i == app.selected_channel) {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(format!("#{}", c.id), style)))
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    if !app.channels.is_empty() {
        state.select(Some(app.selected_channel.min(app.channels.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_tasks_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Tasks;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" tasks ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = TasksScreen::rows(&app.tasks);
    // Guard against a stale selected_row after store changes.
    app.tasks_screen.reclamp(&rows);

    if rows.iter().all(|r| matches!(r, TaskRow::Header(_))) && app.tasks.is_empty() {
        let p = Paragraph::new("No tasks yet. Press `a` to add one.")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        return inner;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| match row {
            TaskRow::Header(status) => {
                let label = format!(" ── {} ──", status.as_str());
                ListItem::new(Line::from(Span::styled(
                    label,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )))
            }
            TaskRow::Task(id) => {
                let selected = i == app.tasks_screen.selected_row && focused;
                render_task_row(app, *id, selected)
            }
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default();
    state.select(Some(app.tasks_screen.selected_row));
    f.render_stateful_widget(list, inner, &mut state);

    inner
}

fn render_task_row(app: &App, id: uuid::Uuid, selected: bool) -> ListItem<'static> {
    let Some(t) = app.tasks.get(id) else {
        return ListItem::new(Line::from(Span::raw("(missing)")));
    };
    let short = t.id.to_string();
    let short = short.get(..8).unwrap_or(&short).to_string();
    let blocked = app.tasks.is_blocked(t);
    let deps_label = if t.blocked_by.is_empty() {
        String::new()
    } else if blocked {
        format!(" [blocked({})] ", t.blocked_by.len())
    } else {
        format!(" [ready({})] ", t.blocked_by.len())
    };
    let assignee_label = format!(" {} ", t.assignee.as_deref().unwrap_or("-"));
    let mut base_style = if blocked {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    if selected {
        base_style = base_style
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);
    }
    let line = Line::from(vec![
        Span::styled(format!("  {short}"), base_style),
        Span::styled(
            format!(" ·{}·", assignee_label),
            if selected {
                base_style
            } else {
                Style::default().fg(Color::Yellow)
            },
        ),
        Span::styled(deps_label, base_style),
        Span::styled(t.title.clone(), base_style),
    ]);
    ListItem::new(line)
}

/// Centered rect helper: size = `(w, h)` clamped to `area`.
fn centered_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Rect::new(x, y, w, h)
}

fn render_task_form(f: &mut Frame, area: Rect, app: &mut App) {
    // Clone the task slice upfront so we can mutably borrow the
    // form without clashing with reads of the store. Tasks lists
    // are small and this only runs while the modal is visible.
    let all_tasks = app.tasks.all().to_vec();
    let Some(form) = app.tasks_screen.form.as_mut() else {
        return;
    };
    let title = match form.kind {
        TaskFormKind::Add => " new task ",
        TaskFormKind::Edit(_) => " edit task ",
    };
    let order = form.tab_order();

    // Split the tab_order into editable fields and trailing buttons.
    let value_fields: Vec<FormField> = order.iter().copied().filter(|f| !f.is_button()).collect();
    let button_fields: Vec<FormField> = order.iter().copied().filter(|f| f.is_button()).collect();

    // Near-fullscreen modal: 2-character margin on every side.
    // Minimum width of 40 so the layout never collapses on a tiny
    // terminal; if the area is smaller than that the caller gets a
    // clamped rect via `centered_rect`.
    let w = area.width.saturating_sub(4).max(40);
    let h = area.height.saturating_sub(4).max(12);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    // Collect field hit-rects into a local buffer so we can drop the
    // immutable borrow on `app` before writing them back.
    let mut hit_rects: Vec<(FormField, Rect)> = Vec::new();

    // Compute how many rows to reserve for the Description scroll
    // box. The box grows to use whatever vertical space is left
    // after the simpler (single-line-value) fields + BlockedBy list
    // + buttons + spacer + error line.
    let simple_value_fields = value_fields
        .iter()
        .filter(|f| **f != FormField::Description && **f != FormField::BlockedBy)
        .count() as u16;
    let blocked_by_block_rows: u16 = {
        // label + list (up to 4 entries + possible "…more", or the
        // "(none)" placeholder). Always reserve 6 rows so the box
        // doesn't bounce around on content change.
        6
    };
    let overhead = simple_value_fields * 2
        + blocked_by_block_rows
        + 1 /* spacer */
        + 1 /* err */
        + 1 /* buttons */
        + 1 /* description label */;
    let desc_target_rows = inner.height.saturating_sub(overhead).max(8);

    // ---- Field rows ----
    let mut y = inner.y;
    for field in &value_fields {
        if y >= inner.y + inner.height {
            break;
        }
        let is_focused = *field == form.focused_field;
        let is_dropdown = TaskForm::is_dropdown(*field);
        let is_blocked_by = *field == FormField::BlockedBy;
        let is_description = *field == FormField::Description;

        // Label line.
        let label_style = if is_focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let label_spans = vec![
            Span::styled(format!("{}:", field.label()), label_style),
            if is_focused && is_blocked_by {
                Span::styled(
                    "   enter to manage",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )
            } else if is_focused && is_description {
                Span::styled(
                    "   enter for newline · tab next field",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )
            } else {
                Span::raw("")
            },
        ];
        let label_rect = Rect::new(inner.x, y, inner.width, 1);
        f.render_widget(Paragraph::new(Line::from(label_spans)), label_rect);
        y += 1;
        if y >= inner.y + inner.height {
            break;
        }

        // Row-height for this value block.
        let start_y = y;
        let rows_drawn: u16;

        if is_description {
            rows_drawn =
                render_description_scroll_box(f, inner, y, form, is_focused, desc_target_rows);
            y += rows_drawn;
        } else if is_blocked_by {
            // Render each current dep on its own line (up to 4 before
            // "…N more"). Each line looks up the task for its title
            // + status so dropped tasks render as "(missing)".
            rows_drawn = render_blocked_by_value_rows(f, inner, y, &*form, &all_tasks);
            y += rows_drawn;
        } else {
            rows_drawn = 1;
            let raw_value = form.field(*field).to_string();
            let display_value = if is_dropdown {
                if raw_value.is_empty() && *field == FormField::Assignee {
                    "(unassigned)".to_string()
                } else if raw_value.is_empty() {
                    "-".to_string()
                } else {
                    raw_value.clone()
                }
            } else {
                raw_value.clone()
            };
            let value_line = if is_dropdown {
                let value_style = if is_focused {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let mut spans = vec![
                    Span::raw("  [ "),
                    Span::styled(display_value, value_style),
                    Span::raw(" ▾ ]"),
                ];
                if is_focused {
                    spans.push(Span::styled(
                        "   enter to pick",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
                Line::from(spans)
            } else if is_focused {
                Line::from(vec![
                    Span::raw(format!("  {display_value}")),
                    Span::styled(
                        "▏",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ])
            } else if display_value.is_empty() {
                Line::from(Span::styled(
                    "  (empty)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ))
            } else {
                Line::from(Span::raw(format!("  {display_value}")))
            };
            let value_rect = Rect::new(inner.x, y, inner.width, 1);
            f.render_widget(Paragraph::new(value_line), value_rect);
            y += 1;
        }

        // Hit-rect covers label + all value rows.
        hit_rects.push((
            *field,
            Rect::new(inner.x, start_y - 1, inner.width, rows_drawn + 1),
        ));
    }

    // ---- Error line ----
    if let Some(err) = form.error.as_deref() {
        if y < inner.y + inner.height {
            let err_rect = Rect::new(inner.x, y, inner.width, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("error: {err}"),
                    Style::default().fg(Color::Red),
                ))),
                err_rect,
            );
            y += 1;
        }
    }

    // ---- Button row ----
    // Bottom-align the button row so it always sits at the last
    // inner line regardless of how many fields we drew.
    let button_y = inner.y + inner.height.saturating_sub(1);
    let mut bx = inner.x + 2;
    for field in &button_fields {
        let label = format!(" {} ", field.label());
        let is_focused = *field == form.focused_field;
        let style = if is_focused {
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };
        let btn_w = label.chars().count() as u16 + 2; // brackets
        let btn_rect = Rect::new(bx, button_y, btn_w, 1);
        let text = format!("[{label}]");
        f.render_widget(Paragraph::new(Span::styled(text, style)), btn_rect);
        hit_rects.push((*field, btn_rect));
        bx += btn_w + 2;
    }

    let _ = y; // silence "assigned but never read" when err is absent.

    // Commit hit rects.
    app.layout.task_form_fields = hit_rects;

    // Picker popup overlays the form when open. Single-select
    // (assignee/status) uses the dropdown picker; BlockedBy uses
    // the multi-select variant.
    if app
        .tasks_screen
        .form
        .as_ref()
        .and_then(|f| f.picker.as_ref())
        .is_some()
    {
        render_dropdown_picker(f, area, app);
    } else if app
        .tasks_screen
        .form
        .as_ref()
        .and_then(|f| f.blocked_by_picker.as_ref())
        .is_some()
    {
        render_blocked_by_picker(f, area, app);
    }
}

fn render_dropdown_picker(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(form) = app.tasks_screen.form.as_ref() else {
        return;
    };
    let Some(picker) = form.picker.as_ref() else {
        return;
    };
    let picker_field = picker.field;
    let picker_selected = picker.selected;
    let options = picker.options.clone();

    // Size: wide enough for the longest option + padding, tall
    // enough for all options + header + border (clamped to area).
    let longest = options
        .iter()
        .map(|s| {
            if s.is_empty() && picker_field == FormField::Assignee {
                "(unassigned)".len()
            } else {
                s.len()
            }
        })
        .max()
        .unwrap_or(0);
    let w = (longest as u16 + 8).clamp(24, area.width);
    let h = (options.len() as u16 + 4).clamp(6, area.height);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);

    let title = match picker_field {
        FormField::Assignee => " pick assignee ",
        FormField::Status => " pick status ",
        _ => " pick ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let items: Vec<ListItem> = options
        .iter()
        .map(|o| {
            let label = if o.is_empty() && picker_field == FormField::Assignee {
                "(unassigned)".to_string()
            } else if o.is_empty() {
                "-".to_string()
            } else {
                o.clone()
            };
            ListItem::new(Line::from(Span::raw(label)))
        })
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(picker_selected));
    f.render_stateful_widget(list, inner, &mut state);

    // Emit per-row hit rects so the mouse handler can find which
    // option was clicked. Rows are 1-line tall and start at the top
    // of the inner rect.
    let mut rows: Vec<Rect> = Vec::with_capacity(options.len());
    for i in 0..options.len() as u16 {
        if i >= inner.height {
            break;
        }
        rows.push(Rect::new(inner.x, inner.y + i, inner.width, 1));
    }
    app.layout.task_form_picker_rows = rows;
}

fn render_task_detail(f: &mut Frame, area: Rect, app: &App, id: uuid::Uuid) {
    let Some(t) = app.tasks.get(id) else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format_task_detail(t, &app.tasks, now);

    let w = 80.min(area.width);
    let h = area.height.saturating_sub(2).max(10);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" task detail (press any key to close) ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = body
        .lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect();
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}

/// Render the multi-line Description scroll box inside the form.
/// Uses a bordered inner block; total screen rows = the visible
/// text height + 2 for the border. The cursor is drawn as an
/// inverted-cell span on the cursor's row when focused.
fn render_description_scroll_box(
    f: &mut Frame,
    inner: Rect,
    start_y: u16,
    form: &mut super::tasks_view::TaskForm,
    focused: bool,
    target_rows: u16,
) -> u16 {
    // Use whatever vertical space the caller reserved, clamped to
    // what's actually free on screen.
    let max_space = (inner.y + inner.height).saturating_sub(start_y);
    let total_rows = target_rows.min(max_space);
    if total_rows < 3 {
        return 0;
    }
    let rect = Rect::new(
        inner.x + 2,
        start_y,
        inner.width.saturating_sub(2),
        total_rows,
    );
    let border_color = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let box_inner = block.inner(rect);
    f.render_widget(block, rect);

    let visible = box_inner.height;
    let wrap_width = box_inner.width;

    // Keep the form's idea of wrap width / visible rows in sync
    // with what we just drew so key handlers navigate correctly.
    form.last_wrap_width = wrap_width;
    form.last_visible_lines = visible;

    // Clamp scroll once the box width changes so we don't end up
    // with an orphaned offset.
    let all_lines = super::tasks_view::wrap_description(&form.description, wrap_width);
    let total_visual = all_lines.len() as u16;
    let max_scroll = total_visual.saturating_sub(visible);
    if form.description_scroll > max_scroll {
        form.description_scroll = max_scroll;
    }
    let scroll = form.description_scroll;

    // Figure out where the cursor sits in visual space so we can
    // draw an inverted cell when focused.
    let (cursor_v_idx, cursor_col) = super::tasks_view::cursor_visual_pos(
        &form.description,
        &all_lines,
        form.description_cursor,
    );

    for row in 0..visible {
        let v_idx = scroll as usize + row as usize;
        if v_idx >= all_lines.len() {
            break;
        }
        let seg = &all_lines[v_idx];
        let text = &form.description[seg.byte_start..seg.byte_end];
        let y = box_inner.y + row;
        let rect_row = Rect::new(box_inner.x, y, box_inner.width, 1);
        if focused && v_idx == cursor_v_idx {
            let col = cursor_col as usize;
            let chars: Vec<char> = text.chars().collect();
            let before: String = chars.iter().take(col).collect();
            let at: String = chars
                .get(col)
                .map(|c| c.to_string())
                .unwrap_or_else(|| " ".into());
            let after: String = chars.iter().skip(col + 1).collect();
            let cursor_style = Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD);
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw(before),
                    Span::styled(at, cursor_style),
                    Span::raw(after),
                ])),
                rect_row,
            );
        } else if !text.is_empty() {
            f.render_widget(
                Paragraph::new(Line::from(Span::raw(text.to_string()))),
                rect_row,
            );
        }
    }

    // Scroll hints at top-right / bottom-right of the box.
    if scroll > 0 {
        let hint_rect = Rect::new(
            box_inner.x + box_inner.width.saturating_sub(1),
            box_inner.y,
            1,
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled("▲", Style::default().fg(Color::DarkGray))),
            hint_rect,
        );
    }
    if scroll + visible < total_visual {
        let hint_rect = Rect::new(
            box_inner.x + box_inner.width.saturating_sub(1),
            box_inner.y + box_inner.height.saturating_sub(1),
            1,
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled("▼", Style::default().fg(Color::DarkGray))),
            hint_rect,
        );
    }

    total_rows
}

/// Render the value rows for the `Blocked by` field inside
/// `render_task_form`. Returns the number of screen rows drawn so
/// the caller can advance `y`. Shows up to 4 entries, then a
/// "+N more" row, then an "(none)" placeholder if the list is
/// empty.
fn render_blocked_by_value_rows(
    f: &mut Frame,
    inner: Rect,
    start_y: u16,
    form: &super::tasks_view::TaskForm,
    all_tasks: &[crate::tasks::Task],
) -> u16 {
    const MAX_VISIBLE: usize = 4;
    let mut y = start_y;
    let max_y = inner.y + inner.height;

    if form.blocked_by.is_empty() {
        if y < max_y {
            let rect = Rect::new(inner.x, y, inner.width, 1);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  (none)",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ))),
                rect,
            );
            y += 1;
        }
        return y - start_y;
    }

    let shown = form.blocked_by.len().min(MAX_VISIBLE);
    for id in form.blocked_by.iter().take(shown) {
        if y >= max_y {
            break;
        }
        let (title, status_str) = match all_tasks.iter().find(|t| t.id == *id) {
            Some(t) => (t.title.clone(), t.status.as_str().to_string()),
            None => ("(missing)".to_string(), "-".to_string()),
        };
        let rect = Rect::new(inner.x, y, inner.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  • "),
                Span::raw(title),
                Span::styled(
                    format!("   [{}]", status_str),
                    Style::default().fg(Color::DarkGray),
                ),
            ])),
            rect,
        );
        y += 1;
    }
    if form.blocked_by.len() > shown && y < max_y {
        let extra = form.blocked_by.len() - shown;
        let rect = Rect::new(inner.x, y, inner.width, 1);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  …and {extra} more"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ))),
            rect,
        );
        y += 1;
    }
    y - start_y
}

/// Overlay that appears over the form when the multi-select
/// blocked-by picker is open.
fn render_blocked_by_picker(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(form) = app.tasks_screen.form.as_mut() else {
        return;
    };
    let Some(picker) = form.blocked_by_picker.as_mut() else {
        return;
    };
    // Snapshot what we need for drawing before releasing the
    // mutable borrow later.
    let options = picker.options.clone();
    let checked = picker.checked.clone();
    let cursor = picker.cursor;

    // Near-fullscreen picker: 4-char horizontal + 3-row vertical
    // margin from the area it overlays. Stays inside the parent
    // form modal so the visual hierarchy is obvious.
    let w = area.width.saturating_sub(8).max(50);
    let h = area.height.saturating_sub(6).max(10);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" blocked by ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    // Reserve: 1 row buttons + 1 row hint at the bottom, leaving
    // the rest for the row list.
    let list_height = inner.height.saturating_sub(2);
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height);
    let button_y = inner.y + list_height;
    let hint_y = button_y + 1;

    // Auto-clamp scroll so the cursor stays inside the visible
    // window. Handles both "cursor moved below viewport" and
    // "cursor jumped above viewport".
    let visible = list_height as usize;
    if cursor < picker.scroll {
        picker.scroll = cursor;
    } else if visible > 0 && cursor >= picker.scroll + visible {
        picker.scroll = cursor + 1 - visible;
    }
    // Also clamp if the list shrank since last open.
    let max_scroll = options.len().saturating_sub(visible);
    if picker.scroll > max_scroll {
        picker.scroll = max_scroll;
    }
    let scroll = picker.scroll;

    // Render visible rows.
    let mut row_rects: Vec<Rect> = Vec::new();
    for (i, row) in options.iter().enumerate().skip(scroll).take(visible) {
        let display_row = (i - scroll) as u16;
        let y = list_area.y + display_row;
        let rect_row = Rect::new(list_area.x, y, list_area.width, 1);
        let is_cursor = i == cursor;
        let box_glyph = if *checked.get(i).unwrap_or(&false) {
            "[✓]"
        } else {
            "[ ]"
        };
        let style = if is_cursor {
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let short = row.id.to_string();
        let short = short.get(..8).unwrap_or(&short).to_string();
        let line = Line::from(vec![Span::styled(
            format!(
                "  {box_glyph} {}  [{}]  ({})",
                row.title,
                row.status.as_str(),
                short
            ),
            style,
        )]);
        f.render_widget(Paragraph::new(line), rect_row);
        row_rects.push(rect_row);
    }

    // Scroll indicators on the right edge.
    if scroll > 0 {
        let hint_rect = Rect::new(
            list_area.x + list_area.width.saturating_sub(1),
            list_area.y,
            1,
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled("▲", Style::default().fg(Color::DarkGray))),
            hint_rect,
        );
    }
    if scroll + visible < options.len() {
        let hint_rect = Rect::new(
            list_area.x + list_area.width.saturating_sub(1),
            list_area.y + list_area.height.saturating_sub(1),
            1,
            1,
        );
        f.render_widget(
            Paragraph::new(Span::styled("▼", Style::default().fg(Color::DarkGray))),
            hint_rect,
        );
    }

    // Bottom hint line.
    let hint = "↑/↓ nav · space toggle · enter/ok save · esc/cancel discard";
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect::new(inner.x, hint_y, inner.width, 1),
    );

    // OK / Cancel buttons on the button row.
    let ok_label = " OK ";
    let cancel_label = " Cancel ";
    let ok_w = ok_label.chars().count() as u16 + 2;
    let cancel_w = cancel_label.chars().count() as u16 + 2;
    let ok_rect = Rect::new(inner.x + 2, button_y, ok_w, 1);
    let cancel_rect = Rect::new(inner.x + 2 + ok_w + 2, button_y, cancel_w, 1);
    let btn_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    f.render_widget(
        Paragraph::new(Span::styled(format!("[{ok_label}]"), btn_style)),
        ok_rect,
    );
    f.render_widget(
        Paragraph::new(Span::styled(format!("[{cancel_label}]"), btn_style)),
        cancel_rect,
    );

    app.layout.task_form_picker_rows = row_rects;
    app.layout.task_form_picker_outer = Some(rect);
    app.layout.task_form_picker_buttons =
        vec![(FormField::Ok, ok_rect), (FormField::Cancel, cancel_rect)];
}

fn render_child_overlay(f: &mut Frame, area: Rect, app: &mut App) {
    use super::tasks_view::{ChildOverlay, ChildPromptKind};

    let Some(overlay) = app.tasks_screen.child_overlay.clone() else {
        return;
    };
    let task_id = overlay.task_id();
    let title_short = app
        .tasks
        .get(task_id)
        .map(|t| t.title.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Decide how big the popup needs to be and collect the row
    // content up front (so we don't re-borrow `app.tasks` while we
    // populate the layout rects afterwards).
    let (title, row_texts, cursor_opt, hint): (String, Vec<(String, Style)>, Option<usize>, &str) =
        match &overlay {
            ChildOverlay::Todos { cursor, .. } => {
                let todos = app
                    .tasks
                    .todos_of(task_id)
                    .map(|v| v.to_vec())
                    .unwrap_or_default();
                let rows: Vec<(String, Style)> = todos
                    .iter()
                    .map(|td| {
                        let short = td.id.to_string();
                        let short = short.get(..8).unwrap_or(&short).to_string();
                        let line = format!("  [{}] {}  {}", td.status.glyph(), short, td.body);
                        let style = if td.status == crate::tasks::TodoStatus::Completed {
                            Style::default()
                                .fg(Color::DarkGray)
                                .add_modifier(Modifier::CROSSED_OUT)
                        } else {
                            Style::default()
                        };
                        (line, style)
                    })
                    .collect();
                let ttl = format!(" todos — {} ", title_short);
                let hint = "a add · e edit · d del · space cycle status · esc close";
                (ttl, rows, Some(*cursor), hint)
            }
            ChildOverlay::Comments { cursor, .. } => {
                let comments = app
                    .tasks
                    .comments_of(task_id)
                    .map(|v| v.to_vec())
                    .unwrap_or_default();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let rows: Vec<(String, Style)> = comments
                    .iter()
                    .map(|c| {
                        let line = format!(
                            "  [{} · {}] {}",
                            super::app::format_age(now, c.timestamp_secs),
                            c.author,
                            c.body
                        );
                        (line, Style::default())
                    })
                    .collect();
                let ttl = format!(" comments — {} ", title_short);
                let hint = "a add · e edit · d del · esc close";
                (ttl, rows, Some(*cursor), hint)
            }
            ChildOverlay::Prompt {
                task_id: _,
                kind,
                buffer,
            } => {
                let (ttl, line) = match kind {
                    ChildPromptKind::AddTodo => (
                        format!(" new todo — {} ", title_short),
                        format!("  {buffer}▏"),
                    ),
                    ChildPromptKind::EditTodo(_) => (
                        format!(" edit todo — {} ", title_short),
                        format!("  {buffer}▏"),
                    ),
                    ChildPromptKind::ConfirmDeleteTodo(_) => (
                        format!(" delete todo — {} ", title_short),
                        "  press y to delete, n or esc to cancel".to_string(),
                    ),
                    ChildPromptKind::AddComment => (
                        format!(" new comment — {} ", title_short),
                        format!("  {buffer}▏"),
                    ),
                    ChildPromptKind::EditComment(_) => (
                        format!(" edit comment — {} ", title_short),
                        format!("  {buffer}▏"),
                    ),
                    ChildPromptKind::ConfirmDeleteComment(_) => (
                        format!(" delete comment — {} ", title_short),
                        "  press y to delete, n or esc to cancel".to_string(),
                    ),
                };
                let hint = "enter submit · esc cancel";
                (ttl, vec![(line, Style::default())], None, hint)
            }
        };

    // Figure out the popup size. Prompt forms are tight; browse
    // forms want room for every row plus a hint line.
    let content_rows = row_texts.len().max(1) as u16;
    let h = (content_rows + 4).min(area.height.saturating_sub(2)).max(5);
    let w = 72.min(area.width);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    // Draw rows + collect hit rects.
    let mut overlay_rows: Vec<Rect> = Vec::new();
    let list_area = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(1),
    );
    if row_texts.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  (none — press `a` to add one)",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ))),
            list_area,
        );
    } else {
        let max_rows = list_area.height as usize;
        for (i, (text, base_style)) in row_texts.iter().take(max_rows).enumerate() {
            let y = list_area.y + i as u16;
            let rect_row = Rect::new(list_area.x, y, list_area.width, 1);
            let is_cursor = Some(i) == cursor_opt;
            let style = if is_cursor {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                *base_style
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(text.clone(), style))),
                rect_row,
            );
            overlay_rows.push(rect_row);
        }
    }

    // Hint line at the bottom of the inner rect.
    let hint_y = inner.y + inner.height.saturating_sub(1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect::new(inner.x, hint_y, inner.width, 1),
    );

    app.layout.child_overlay_rows = overlay_rows;
    app.layout.child_overlay_outer = Some(rect);
}
