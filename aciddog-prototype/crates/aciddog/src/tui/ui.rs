use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Widget, Wrap,
};
use ratatui::Frame;

use super::app::{ActiveView, AgentStatus, App, ChatKind, Focus};
use super::blocks::{block_fingerprint, AnchorScratch, ChatButton, ScrollAnchor};
use super::cores_view::{agents_using_core, format_usage_cell, is_team_default, CoreRow};
use super::memory_view::{
    BankFocus, LinkPickerFocus, LinksOverlay, MemoryStage, NoteDetail, NoteField, NoteForm,
};
use super::repo_diff_view::render_repo_diff_screen;
use super::tasks_view::{
    FormField, ReviewPromptField, TaskDetailScreen, TaskForm, TaskFormKind, TaskRow, TasksPrompt,
    TasksScreen,
};

const APP_NAME: &str = "aciddog";
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Glyph rendered before the display name for leader-flagged agents
/// and automatons in the agent list. Centralised here so a future
/// terminal-capability fallback (ACIDDOG_NO_EMOJI=1, etc.) only has
/// to be wired up in one place. For now we ship the crown raw to
/// match the existing 9-glyph emoji set in `render_agent_list`.
fn leader_glyph() -> &'static str {
    "👑"
}

pub fn render(f: &mut Frame, app: &mut App) {
    // Fresh frame: reset interactive regions.
    app.layout.chat_buttons.clear();
    app.layout.task_form_fields.clear();
    app.layout.task_detail_tabs.clear();
    app.layout.task_detail_comment_rows.clear();
    app.layout.task_detail_todo_rows.clear();
    app.layout.task_detail_review_section = None;
    app.layout.task_form_picker_rows.clear();
    app.layout.task_form_picker_outer = None;
    app.layout.task_form_picker_buttons.clear();
    app.layout.memory_bank_rows.clear();
    app.layout.memory_result_rows.clear();
    app.layout.memory_search_rect = None;
    app.layout.memory_form_fields.clear();
    app.layout.memory_link_rows.clear();
    app.layout.memory_modal_outer = None;
    app.layout.inbox_hotspots.clear();
    app.layout.inbox_list_inner = None;
    app.layout.inbox_detail_inner = None;
    app.layout.cron_hotspots.clear();
    app.layout.repo_diff_section = None;

    // Input height: in Chat and Channel views, grow from 3 to 7 rows
    // (1–5 usable + 2 for borders) based on the wrapped line count of
    // the current input buffer. Tasks / Memory keep the fixed 3 rows
    // so their bottom-bar hints stay on a single row.
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
        ActiveView::Channel(idx) => {
            let inner_width = f.area().width.saturating_sub(2);
            let usable = app
                .channels
                .get(idx)
                .map(|ch| {
                    let rows = super::app::input_visual_rows(ch, inner_width);
                    (rows.len() as u16).clamp(1, 5)
                })
                .unwrap_or(1);
            usable + 2
        }
        ActiveView::Tasks => 3,
        ActiveView::Memory => 3,
        ActiveView::Cores => 3,
        ActiveView::Inbox => 3,
        ActiveView::Cron => 3,
        ActiveView::RepoDiff { .. } => 3,
        ActiveView::TaskDetail { .. } => 3,
        ActiveView::WorkflowForm { .. } | ActiveView::WorkflowInstance { .. } => 3,
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

    // Right column (top → bottom): Views list, Agents, Workflows (if
    // any workflows are declared), Repos (if any repositories are
    // declared), Channels (if any channels are declared). Each list's
    // height = border (2) + one line per entry; Agents takes whatever's
    // left. When a channel / workflow is selected its members (or
    // instances) nest beneath its row, so the panel can grow
    // dynamically.
    let channels_rows_len = channel_rows(app).len() as u16;
    let channels_height = if !app.channels.is_empty() {
        channels_rows_len.saturating_add(2).max(3)
    } else {
        0
    };
    let workflows_rows_len = workflow_rows(app).len() as u16;
    let workflows_height = if !app.config.workflows.is_empty() {
        workflows_rows_len.saturating_add(2).max(3)
    } else {
        0
    };
    let repos_height = if !app.repos.is_empty() {
        repo_rows(app).len() as u16 + 2
    } else {
        0
    };

    let constraints = right_column_constraints(app);
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(right_col);
    let views_area = right_rows[0];
    let agents_area = right_rows[1];
    let mut next_idx = 2;
    let workflows_area = if workflows_height > 0 {
        let r = right_rows[next_idx];
        next_idx += 1;
        Some(r)
    } else {
        None
    };
    let repos_area = if repos_height > 0 {
        let r = right_rows[next_idx];
        next_idx += 1;
        Some(r)
    } else {
        None
    };
    let channels_area = if channels_height > 0 {
        Some(right_rows[next_idx])
    } else {
        None
    };

    render_title(f, title_area, app);

    let main_inner = match app.active_view {
        ActiveView::Chat => render_agent_chat(f, main_area, app),
        ActiveView::Tasks => render_tasks_screen(f, main_area, app),
        ActiveView::Channel(idx) => render_channel_chat(f, main_area, app, idx),
        ActiveView::Memory => render_memory_screen(f, main_area, app),
        ActiveView::Cores => render_cores_screen(f, main_area, app),
        ActiveView::Inbox => render_inbox_screen(f, main_area, app),
        ActiveView::Cron => render_cron_screen(f, main_area, app),
        ActiveView::RepoDiff { .. } => render_repo_diff_screen(f, main_area, app),
        ActiveView::TaskDetail { task_id } => render_task_detail_screen(f, main_area, app, task_id),
        ActiveView::WorkflowForm { workflow_idx } => {
            render_workflow_form(f, main_area, app, workflow_idx)
        }
        ActiveView::WorkflowInstance {
            workflow_idx,
            instance_id,
        } => render_workflow_instance(f, main_area, app, workflow_idx, instance_id),
    };
    let views_inner = render_views_list(f, views_area, app);
    let list_inner = render_agent_list(f, agents_area, app);
    let workflows_inner = workflows_area.map(|a| render_workflows_list(f, a, app));
    let repos_inner = repos_area.map(|a| render_repos_list(f, a, app));
    let channels_inner = channels_area.map(|a| render_channels_list(f, a, app));
    let input_inner = render_chat_input(f, chat_input_area, app);
    render_status(f, status_area, app);

    // Task modal/prompt overlays are shared by the list and detail route.
    if matches!(
        app.active_view,
        ActiveView::Tasks | ActiveView::TaskDetail { .. }
    ) && app.tasks_screen.form.is_some()
    {
        render_task_form(f, main_area, app);
    }

    // Memory view modals: form / detail / links overlay / confirm.
    if app.active_view == ActiveView::Memory {
        render_memory_modals(f, main_area, app);
    }

    // Cron view modals: edit / add form, delete confirmation.
    if app.active_view == ActiveView::Cron {
        render_cron_modals(f, main_area, app);
    }

    app.layout.agent_list_inner = Some(list_inner);
    app.layout.chat_history_inner = Some(main_inner);
    app.layout.chat_input_inner = Some(input_inner);
    app.layout.views_list_inner = Some(views_inner);
    app.layout.channels_list_inner = channels_inner;
    app.layout.repos_list_inner = repos_inner;
    app.layout.workflows_list_inner = workflows_inner;
}

pub(super) fn right_column_constraints(app: &App) -> Vec<Constraint> {
    let views_height = (app.view_entries().len() as u16).saturating_add(2).max(3);
    let channels_rows_len = channel_rows(app).len() as u16;
    let channels_height = if !app.channels.is_empty() {
        channels_rows_len.saturating_add(2).max(3)
    } else {
        0
    };
    let workflows_rows_len = workflow_rows(app).len() as u16;
    let workflows_height = if !app.config.workflows.is_empty() {
        workflows_rows_len.saturating_add(2).max(3)
    } else {
        0
    };
    let repos_height = if !app.repos.is_empty() {
        repo_rows(app).len() as u16 + 2
    } else {
        0
    };

    let mut constraints: Vec<Constraint> = Vec::with_capacity(5);
    constraints.push(Constraint::Length(views_height));
    constraints.push(Constraint::Min(5));
    if workflows_height > 0 {
        constraints.push(Constraint::Length(workflows_height));
    }
    if repos_height > 0 {
        constraints.push(Constraint::Length(repos_height));
    }
    if channels_height > 0 {
        constraints.push(Constraint::Length(channels_height));
    }
    constraints
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

    // Walk the same visual-row layout the click handler uses so the
    // two stay in lockstep — clicking a row maps to the chat the
    // renderer drew on that line. Group headers are non-selectable;
    // the keyboard cursor still operates on chat indices via
    // `app.selected` and the visual row of the selected chat tracks
    // header insertions.
    let rows = app.agent_list_visual_rows();
    let mut items: Vec<ListItem> = Vec::with_capacity(rows.len());
    let mut sel_row: Option<usize> = None;
    for row in &rows {
        match row {
            crate::tui::app::AgentListVisualRow::GroupHeader { label } => {
                items.push(ListItem::new(Line::from(Span::styled(
                    format!("── {label} ──"),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ))));
            }
            crate::tui::app::AgentListVisualRow::Chat { idx, depth } => {
                let i = *idx;
                let depth = *depth;
                let c = &app.chats[i];
                // Automatons share the agent list but live in a distinct
                // square-family icon set so the kind is always visible —
                // not just when they're idle. Hibernation doesn't apply
                // (no AI state to park); Asleep does, since a supervisor
                // can put any fleet member to sleep.
                let dot = match (c.kind, c.status) {
                    (ChatKind::Automaton, AgentStatus::Asleep) => "🟦",
                    (ChatKind::Automaton, AgentStatus::Working) => "🟨",
                    (ChatKind::Automaton, AgentStatus::Error) => "🟥",
                    (ChatKind::Automaton, _) => "🟩",
                    (_, AgentStatus::Ready) => "🟢",
                    (_, AgentStatus::Working) => "🟡",
                    (_, AgentStatus::Error) => "🔴",
                    (_, AgentStatus::Hibernated) => "💤",
                    (_, AgentStatus::Asleep) => "🌙",
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
                // Crown the leader-flagged top-level Agent / Automaton
                // chats. Placement is "🟢 👑 name" — after the status dot,
                // before the display name. Workers and non-leader chats
                // skip this span entirely.
                if app.is_leader(i) {
                    spans.push(Span::raw(format!("{} ", leader_glyph())));
                }
                spans.push(Span::styled(c.display_name.clone(), name_style));
                if c.parent_idx.is_some() {
                    spans.push(Span::styled(
                        " [worker]",
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                if i == app.selected {
                    sel_row = Some(items.len());
                }
                items.push(ListItem::new(Line::from(spans)));
            }
        }
    }

    let list = List::new(items).block(block);

    let mut state = ListState::default();
    if let Some(row) = sel_row {
        state.select(Some(row));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_agent_chat(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Chat;
    let selected_agent = app.selected;

    let title = match app.chats.get(selected_agent) {
        Some(chat) => {
            let hash_suffix = if chat.session_hash.is_empty() {
                String::new()
            } else {
                format!(" [{}]", chat.session_hash)
            };
            // Show role/model for configured agents; for workers,
            // fall back to "worker · <template>" since the chat
            // itself carries the display name and template, not the
            // role/model pair from an `agent` config block.
            let base = if chat.kind == ChatKind::Automaton {
                let auto = app
                    .config
                    .automatons
                    .iter()
                    .find(|a| a.id == chat.display_name);
                match auto {
                    Some(a) => {
                        let desc = match &a.backend {
                            crate::config::AutomatonBackend::Script { command, args, .. } => {
                                std::iter::once(command.as_str())
                                    .chain(args.iter().map(String::as_str))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            }
                            crate::config::AutomatonBackend::Webservice { bind, .. } => {
                                match bind {
                                    crate::config::WebserviceBind::Https { address, .. } => {
                                        format!("webservice https://{address}")
                                    }
                                    crate::config::WebserviceBind::UnixSocket { path } => {
                                        format!("webservice unix:{}", path.display())
                                    }
                                }
                            }
                            crate::config::AutomatonBackend::Remote {
                                transport,
                                remote_target,
                                ..
                            } => match transport {
                                crate::config::RemoteTransport::Https { url, .. } => {
                                    format!("remote {url} → {remote_target}")
                                }
                                crate::config::RemoteTransport::UnixSocket { path } => {
                                    format!("remote unix:{} → {remote_target}", path.display())
                                }
                            },
                        };
                        format!(" {} · automaton · {} ", a.id, desc)
                    }
                    None => format!(" {} · automaton ", chat.display_name),
                }
            } else if let Some(template) = &chat.worker_template {
                if let Some(w) = app.config.worker(template) {
                    let core = app.config.effective_ai_core(w.ai_core.as_deref());
                    let model = app
                        .config
                        .effective_model(w.model.as_deref(), core)
                        .unwrap_or("(default)");
                    format!(" {} · worker · {} · {} ", chat.display_name, w.role, model)
                } else {
                    format!(" {} · worker ", chat.display_name)
                }
            } else if let Some(a) = app.config.agents.get(selected_agent) {
                let core = app.config.effective_ai_core(a.ai_core.as_deref());
                let model = app
                    .config
                    .effective_model(a.model.as_deref(), core)
                    .unwrap_or("(default)");
                format!(" {} · {} · {} ", a.id, a.role, model)
            } else {
                format!(" {} ", chat.display_name)
            };
            // Strip the trailing space so the hash sits flush with the
            // last token, then re-add a single space at the end so the
            // `Block::title` keeps the existing visual padding.
            let trimmed = base.trim_end();
            format!("{trimmed}{hash_suffix} ")
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
    // Caller passes the post-selection-shift width: ChatBlock::render reserves
    // col 0 for the selection indicator, and height arms subtract only their
    // own per-variant overhead.
    let mut heights = LazyHeights::new(
        &chat.blocks,
        inner.width.saturating_sub(1),
        thinking_visible,
    );

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
                let cache_hit = chat.anchor_scratch.as_ref().is_some_and(|c| {
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
        Some(c) if !c.session_hash.is_empty() => {
            format!(" #{}  [{}] ", c.id, c.session_hash)
        }
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
        // Caller passes the post-selection-shift width: ChatBlock::render reserves
        // col 0 for the selection indicator, and height arms subtract only their
        // own per-variant overhead.
        let h = ch.blocks[i].height(inner.width.saturating_sub(1));
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
        translated.rect = Rect::new(b.rect.x.saturating_add(dst_x), new_y, b.rect.width, new_h);
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
    if matches!(
        app.active_view,
        ActiveView::Tasks | ActiveView::TaskDetail { .. }
    ) {
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

/// Multi-line input box for a channel view. Mirrors the agent-chat
/// input: word-wraps to the box width, injects a caret span, grows
/// 3→7 rows, and shows a scrollbar when the buffer overflows the
/// visible window. The buffer caches its wrap width and visible row
/// count so key handlers stay in sync with what's rendered.
fn render_channel_input(f: &mut Frame, area: Rect, app: &mut App, idx: usize) -> Rect {
    let focused = app.focus == Focus::Chat;
    let Some(ch) = app.channels.get_mut(idx) else {
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

    // Cache the layout so the key handlers wrap the buffer the same
    // way the renderer does.
    ch.input_wrap_width = inner.width;
    ch.input_visible_rows = inner.height;

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

    let rows = super::app::input_visual_rows(ch, inner.width);
    let total = rows.len() as u16;
    let visible = inner.height.max(1);
    let max_scroll = total.saturating_sub(visible);
    if ch.input_scroll > max_scroll {
        ch.input_scroll = max_scroll;
    }
    let scroll = ch.input_scroll;

    let (cursor_row, _cursor_row_start, cursor_col) =
        super::app::input_cursor_visual(ch, inner.width);

    let first = scroll as usize;
    let last = (first + visible as usize).min(rows.len());
    let mut lines: Vec<Line> = Vec::with_capacity(last - first);
    for (i, &(r_start, r_end)) in rows[first..last].iter().enumerate() {
        let row_idx = first + i;
        let row_text = &ch.input[r_start..r_end];
        if focused && row_idx == cursor_row {
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

fn render_tasks_input(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = matches!(app.focus, Focus::Tasks | Focus::TaskDetail);
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
            TasksPrompt::AddTodo { body, .. } => (
                " new todo ".to_string(),
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
            TasksPrompt::EditTodo { body, .. } => (
                " edit todo ".to_string(),
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
            TasksPrompt::ConfirmDeleteTodo { .. } => (
                " delete todo? ".to_string(),
                Line::from(Span::styled(
                    "press y to delete, n or esc to cancel",
                    Style::default().fg(Color::Yellow),
                )),
            ),
            TasksPrompt::AddComment { body, .. } => (
                " new comment ".to_string(),
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
            TasksPrompt::EditComment { body, .. } => (
                " edit comment ".to_string(),
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
            TasksPrompt::ConfirmDeleteComment { .. } => (
                " delete comment? ".to_string(),
                Line::from(Span::styled(
                    "press y to delete, n or esc to cancel",
                    Style::default().fg(Color::Yellow),
                )),
            ),
            TasksPrompt::RejectPlan { body, .. } => (
                " reject reason ".to_string(),
                Line::from(vec![
                    Span::styled(body.clone(), Style::default().fg(Color::Red)),
                    Span::styled(
                        "▏",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                ]),
            ),
            TasksPrompt::ReviewPicker {
                options,
                selected,
                field,
                changes_made,
                demo_instructions,
                error,
                ..
            } => {
                let reviewers = if options.is_empty() {
                    "no reviewers available".to_string()
                } else {
                    options
                        .iter()
                        .enumerate()
                        .map(|(idx, option)| {
                            if idx == *selected {
                                format!("[{option}]")
                            } else {
                                option.clone()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" · ")
                };
                let cursor = Span::styled(
                    "▏",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
                let mut spans = vec![
                    Span::styled("Changes made: ", Style::default().fg(Color::Yellow)),
                    Span::raw(changes_made.clone()),
                ];
                if *field == ReviewPromptField::ChangesMade {
                    spans.push(cursor.clone());
                }
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "Demo instructions: ",
                    Style::default().fg(Color::Yellow),
                ));
                spans.push(Span::raw(demo_instructions.clone()));
                if *field == ReviewPromptField::DemoInstructions {
                    spans.push(cursor.clone());
                }
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "Reviewer: ",
                    Style::default().fg(Color::Yellow),
                ));
                let reviewer_style = if *field == ReviewPromptField::Reviewer {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Yellow)
                };
                spans.push(Span::styled(reviewers, reviewer_style));
                if let Some(error) = error {
                    spans.push(Span::styled(
                        format!("  {error}"),
                        Style::default().fg(Color::Red),
                    ));
                }
                (" submit review ".to_string(), Line::from(spans))
            }
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
        // When a Planning task assigned to user is selected, surface
        // the `y` approve hint up-front; otherwise show the standard
        // shortcut row.
        let rows = TasksScreen::rows(&app.tasks);
        let hint = match app
            .tasks_screen
            .selected_task_id(&rows)
            .and_then(|id| app.tasks.get(id))
        {
            Some(t)
                if t.status == crate::tasks::TaskStatus::Planning
                    && t.assignee.as_deref() == Some(crate::config::USER_INBOX_ID) =>
            {
                "y approve · a add · e/⏎ edit · d del · t take · n note · s status · esc back"
            }
            Some(t)
                if t.status == crate::tasks::TaskStatus::InProgress
                    && t.assignee.as_deref() == Some(super::tasks_view::OPERATOR) =>
            {
                "r submit-for-review · a add · e/⏎ edit · d del · t take · n note · s status · esc back"
            }
            _ => "a add · e/⏎ edit · d del · t take · n note · s status · esc back",
        };
        (
            " tasks ".to_string(),
            Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))),
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
        Focus::Repos => "repos",
        Focus::Memory => "memory",
        Focus::Workflows => "workflows",
        Focus::Workflow => match app.active_view {
            ActiveView::WorkflowForm { .. } => "workflow form",
            _ => "workflow",
        },
        Focus::Cores => "cores",
        Focus::Inbox => "inbox",
        Focus::Cron => "cron",
        Focus::RepoDiff => "repo diff",
        Focus::TaskDetail => "task detail",
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
            Span::raw(" next panel"),
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
            Span::raw(" copy"),
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
        Focus::Repos => vec![
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
            Span::styled("tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next panel"),
        ],
        Focus::RepoDiff => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(" scroll  "),
            Span::styled("pgup/pgdn", Style::default().fg(Color::Cyan)),
            Span::raw(" page  "),
            Span::styled("esc/q", Style::default().fg(Color::Cyan)),
            Span::raw(" close"),
        ],
        Focus::TaskDetail => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("esc/q", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
        Focus::Memory => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("/", Style::default().fg(Color::Cyan)),
            Span::raw(" search  "),
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(" add  "),
            Span::styled("e", Style::default().fg(Color::Cyan)),
            Span::raw(" edit  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" del  "),
            Span::styled("l", Style::default().fg(Color::Cyan)),
            Span::raw(" links  "),
            Span::styled("⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" detail  "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
        Focus::Workflows => vec![
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
        Focus::Workflow => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
        Focus::Cores => vec![
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
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(" refresh  "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
        Focus::Inbox => vec![
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
            Span::styled("r", Style::default().fg(Color::Cyan)),
            Span::raw(" reply  "),
            Span::styled("f", Style::default().fg(Color::Cyan)),
            Span::raw(" forward  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" delete  "),
            Span::styled("ctrl+⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" send  "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
        Focus::Cron => vec![
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
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(" add  "),
            Span::styled("e", Style::default().fg(Color::Cyan)),
            Span::raw(" edit  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" del  "),
            Span::styled("x", Style::default().fg(Color::Cyan)),
            Span::raw(" toggle  "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
    };

    if app.focus == Focus::Repos {
        if let Some(msg) = &app.repo_status_msg {
            hint_spans.push(Span::raw("   "));
            hint_spans.push(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

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
    let entries = app.view_entries();
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let active = matches!(
                (*name, app.active_view),
                ("Tasks", ActiveView::Tasks)
                    | ("Memory", ActiveView::Memory)
                    | ("Cores", ActiveView::Cores)
                    | ("Inbox", ActiveView::Inbox),
            );
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
            // Inbox shows the total-message count suffix so the
            // operator sees there's unread mail without opening
            // the view. Tasks follows the same idiom for active
            // user-assigned work. Empty inbox/tasks keeps the plain
            // label.
            let label = if *name == "Inbox" && !app.inbox.is_empty() {
                format!("Inbox ({})", app.inbox.len())
            } else if *name == "Tasks" && app.user_task_count() > 0 {
                format!("Tasks ({})", app.user_task_count())
            } else {
                (*name).to_string()
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    state.select(Some(app.selected_view));
    f.render_stateful_widget(list, area, &mut state);
    inner
}

/// A single renderable row in the channels panel. Members appear only
/// under the currently-active channel so the operator can see who's in
/// the room they just opened without a second panel.
pub(super) enum ChannelRow {
    Header {
        channel_idx: usize,
    },
    Member {
        member_id: String,
        /// Resolved chat index for this member id, if one exists. `None`
        /// means the configured agent has no live chat (never spawned),
        /// and the row renders inert and dimmed.
        agent_chat_idx: Option<usize>,
    },
}

pub(super) fn channel_rows(app: &App) -> Vec<ChannelRow> {
    let active = match app.active_view {
        ActiveView::Channel(i) => Some(i),
        _ => None,
    };
    let mut rows: Vec<ChannelRow> = Vec::with_capacity(app.channels.len());
    for (ci, ch) in app.channels.iter().enumerate() {
        rows.push(ChannelRow::Header { channel_idx: ci });
        if Some(ci) == active {
            for member_id in &ch.members {
                let agent_chat_idx = app
                    .chats
                    .iter()
                    .position(|c| c.parent_idx.is_none() && &c.display_name == member_id);
                rows.push(ChannelRow::Member {
                    member_id: member_id.clone(),
                    agent_chat_idx,
                });
            }
        }
    }
    rows
}

/// A single renderable row in the repos panel. Every declared
/// repository is always expanded so its known worktrees are visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RepoRow {
    Header {
        repo_idx: usize,
    },
    Worktree {
        repo_idx: usize,
        worktree_idx: usize,
    },
}

pub(super) fn repo_rows(app: &App) -> Vec<RepoRow> {
    let worktree_count: usize = app.repos.iter().map(|repo| repo.worktrees.len()).sum();
    let mut rows: Vec<RepoRow> = Vec::with_capacity(app.repos.len() + worktree_count);
    for (repo_idx, repo) in app.repos.iter().enumerate() {
        rows.push(RepoRow::Header { repo_idx });
        for worktree_idx in 0..repo.worktrees.len() {
            rows.push(RepoRow::Worktree {
                repo_idx,
                worktree_idx,
            });
        }
    }
    rows
}

fn repo_row_line(row_idx: usize, row: &RepoRow, app: &App, focused: bool) -> Line<'static> {
    match row {
        RepoRow::Header { repo_idx } => {
            let repo = &app.repos[*repo_idx];
            let style = if focused && row_idx == app.selected_repo_row {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let mut spans = vec![Span::styled(format!("📦 {}", repo.id), style)];
            if let Some(default_branch) = &repo.default_branch {
                spans.push(Span::styled(
                    format!(" (default: {default_branch})"),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            Line::from(spans)
        }
        RepoRow::Worktree {
            repo_idx,
            worktree_idx,
        } => {
            let worktree = &app.repos[*repo_idx].worktrees[*worktree_idx];
            let style = if focused && row_idx == app.selected_repo_row {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let branch_span = match worktree.branch.as_deref() {
                Some(branch) => Span::styled(branch.to_string(), style),
                None => {
                    let detached_style = if focused && row_idx == app.selected_repo_row {
                        Style::default()
                            .bg(Color::Cyan)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD | Modifier::ITALIC)
                    } else {
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::ITALIC)
                    };
                    Span::styled("(detached)", detached_style)
                }
            };
            Line::from(vec![
                Span::raw("  "),
                Span::styled("↳ ", Style::default().fg(Color::DarkGray)),
                branch_span,
            ])
        }
    }
}

fn render_repos_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Repos;
    let title = if app.repos.is_empty() {
        " repos ".to_string()
    } else {
        format!(" repos ({}) ", app.repos.len())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    let rows = repo_rows(app);
    let mut selected_row: Option<usize> = None;
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(row_idx, row)| {
            if row_idx == app.selected_repo_row {
                selected_row = Some(row_idx);
            }
            ListItem::new(repo_row_line(row_idx, row, app, focused))
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    if let Some(r) = selected_row {
        state.select(Some(r));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod repos_panel_tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use super::*;
    use crate::config::{AciddogConfig, Channel, Team, Workflow};
    use crate::repos::{RepoState, Worktree};

    fn config_with(workflows: Vec<Workflow>, channels: Vec<Channel>) -> AciddogConfig {
        AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
                default_idle_timeout: None,
            },
            agents: vec![],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels,
            repositories: vec![],
            cron_jobs: vec![],
            workflows,
            webhooks: vec![],
            thinking_messages: vec!["thinking".into()],
        }
    }

    fn workflow(id: &str) -> Workflow {
        Workflow {
            id: id.into(),
            description: "workflow".into(),
            max_retries: 1,
            properties: vec![],
            steps: vec![],
        }
    }

    fn channel(id: &str) -> Channel {
        Channel {
            id: id.into(),
            description: None,
            welcome: None,
            members: vec![],
        }
    }

    fn repo(id: &str, worktrees: Vec<(&str, Option<&str>)>) -> RepoState {
        let path = PathBuf::from(format!("/repos/{id}"));
        RepoState {
            id: id.into(),
            url: format!("https://example.test/{id}.git"),
            path: path.clone(),
            default_branch: Some("main".into()),
            worktrees: worktrees
                .into_iter()
                .map(|(suffix, branch)| Worktree {
                    path: path.join(suffix),
                    branch: branch.map(str::to_string),
                })
                .collect(),
            last_worktree_scan: Some(Instant::now()),
        }
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn repo_rows_include_every_declared_repo_and_worktree() {
        let mut app = App::new(config_with(vec![], vec![]));
        app.repos = vec![
            repo(
                "alpha",
                vec![
                    ("main", Some("main")),
                    ("feature-a", Some("feature-a")),
                    ("detached-a", None),
                ],
            ),
            repo(
                "beta",
                vec![
                    ("main", Some("main")),
                    ("feature-b", Some("feature-b")),
                    ("detached-b", None),
                ],
            ),
        ];

        assert_eq!(
            repo_rows(&app),
            vec![
                RepoRow::Header { repo_idx: 0 },
                RepoRow::Worktree {
                    repo_idx: 0,
                    worktree_idx: 0,
                },
                RepoRow::Worktree {
                    repo_idx: 0,
                    worktree_idx: 1,
                },
                RepoRow::Worktree {
                    repo_idx: 0,
                    worktree_idx: 2,
                },
                RepoRow::Header { repo_idx: 1 },
                RepoRow::Worktree {
                    repo_idx: 1,
                    worktree_idx: 0,
                },
                RepoRow::Worktree {
                    repo_idx: 1,
                    worktree_idx: 1,
                },
                RepoRow::Worktree {
                    repo_idx: 1,
                    worktree_idx: 2,
                },
            ]
        );
    }

    #[test]
    fn repo_worktree_rows_render_branch_labels_without_paths_or_suffixes() {
        let mut app = App::new(config_with(vec![], vec![]));
        app.repos = vec![repo(
            "alpha",
            vec![
                ("main", Some("main")),
                ("wt-feat", Some("feature-x")),
                ("wt-detached", None),
            ],
        )];

        let rows = repo_rows(&app);
        let worktree_lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .filter_map(|(row_idx, row)| match row {
                RepoRow::Worktree { .. } => Some(repo_row_line(row_idx, row, &app, false)),
                RepoRow::Header { .. } => None,
            })
            .collect();

        assert_eq!(worktree_lines.len(), 3);

        let main_text = line_text(&worktree_lines[0]);
        assert!(main_text.contains("main"));
        assert!(!main_text.contains('/'));
        assert!(!main_text.contains(" [main]"));

        let feature_text = line_text(&worktree_lines[1]);
        assert!(feature_text.contains("feature-x"));
        assert!(!feature_text.contains('/'));
        assert!(!feature_text.contains("wt-feat"));
        assert!(!feature_text.contains(" [feature-x]"));

        let detached_text = line_text(&worktree_lines[2]);
        assert!(!detached_text.contains('/'));
        assert!(!detached_text.contains("wt-detached"));
        assert_eq!(worktree_lines[2].spans[2].content.as_ref(), "(detached)");
    }

    #[test]
    fn right_column_constraints_include_repos_only_when_declared() {
        let cases = [
            (
                false,
                false,
                vec![],
                0,
                vec![Constraint::Length(6), Constraint::Min(5)],
            ),
            (
                false,
                false,
                vec![repo("alpha", vec![("main", Some("main"))])],
                4,
                vec![
                    Constraint::Length(6),
                    Constraint::Min(5),
                    Constraint::Length(4),
                ],
            ),
            (
                false,
                false,
                vec![
                    repo(
                        "alpha",
                        vec![("main", Some("main")), ("feature-a", Some("feature-a"))],
                    ),
                    repo(
                        "beta",
                        vec![("main", Some("main")), ("feature-b", Some("feature-b"))],
                    ),
                ],
                8,
                vec![
                    Constraint::Length(6),
                    Constraint::Min(5),
                    Constraint::Length(8),
                ],
            ),
            (
                true,
                false,
                vec![repo("alpha", vec![("main", Some("main"))])],
                4,
                vec![
                    Constraint::Length(6),
                    Constraint::Min(5),
                    Constraint::Length(3),
                    Constraint::Length(4),
                ],
            ),
            (
                false,
                true,
                vec![repo("alpha", vec![("main", Some("main"))])],
                4,
                vec![
                    Constraint::Length(6),
                    Constraint::Min(5),
                    Constraint::Length(4),
                    Constraint::Length(3),
                ],
            ),
            (
                true,
                true,
                vec![repo("alpha", vec![("main", Some("main"))])],
                4,
                vec![
                    Constraint::Length(6),
                    Constraint::Min(5),
                    Constraint::Length(3),
                    Constraint::Length(4),
                    Constraint::Length(3),
                ],
            ),
        ];

        for (has_workflow, has_channel, repos, expected_repos_height, expected) in cases {
            let workflows = if has_workflow {
                vec![workflow("build")]
            } else {
                vec![]
            };
            let channels = if has_channel {
                vec![channel("dev")]
            } else {
                vec![]
            };
            let mut app = App::new(config_with(workflows, channels));
            app.repos = repos;
            let actual = right_column_constraints(&app);
            assert_eq!(actual.len(), expected.len());
            assert_eq!(actual, expected);

            let repos_constraint = actual
                .iter()
                .find(|constraint| **constraint == Constraint::Length(expected_repos_height));
            if expected_repos_height == 0 {
                assert!(repos_constraint.is_none());
            } else {
                assert!(repos_constraint.is_some());
            }
        }
    }
}

fn render_channels_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Channels;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" channels ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    let rows = channel_rows(app);
    let mut selected_row: Option<usize> = None;
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(row_idx, row)| match row {
            ChannelRow::Header { channel_idx } => {
                let i = *channel_idx;
                let c = &app.channels[i];
                let active = matches!(app.active_view, ActiveView::Channel(j) if j == i);
                // Persistent highlight on the currently-open channel so
                // clicking it gives visible feedback once focus leaves
                // the panel (mirrors the agent list's always-on selected
                // highlight). The same highlight tracks the keyboard
                // cursor when the panel is focused.
                let mut style = if active || (focused && i == app.selected_channel) {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else if c.is_ephemeral {
                    // Ephemeral (meeting) channels render in yellow so
                    // the operator can tell at a glance which rooms are
                    // permanent (WCL-declared) vs dynamically created
                    // and not persisted.
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                // Italics in every state so ephemeral-ness remains
                // visible even under the cyan selection highlight.
                if c.is_ephemeral {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if !app.channels.is_empty() && i == app.selected_channel.min(app.channels.len() - 1)
                {
                    selected_row = Some(row_idx);
                }
                ListItem::new(Line::from(Span::styled(format!("#{}", c.id), style)))
            }
            ChannelRow::Member {
                member_id,
                agent_chat_idx,
            } => {
                let dim = agent_chat_idx
                    .and_then(|idx| app.chats.get(idx))
                    .map(|c| c.hibernated)
                    .unwrap_or(true);
                let name_style = if dim {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default()
                };
                let spans = vec![
                    Span::raw("  "),
                    Span::styled("↳ ", Style::default().fg(Color::DarkGray)),
                    Span::styled(member_id.clone(), name_style),
                ];
                ListItem::new(Line::from(spans))
            }
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    if let Some(r) = selected_row {
        state.select(Some(r));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

/// Color used for the `── status ──` header strip on the tasks screen.
/// `Planning` is magenta (action requested from the user); `Approved`
/// is green (just-validated work, queued); everything else stays cyan.
fn status_header_color(status: crate::tasks::TaskStatus) -> Color {
    use crate::tasks::TaskStatus::*;
    match status {
        Planning => Color::Magenta,
        Approved => Color::Green,
        Review => Color::Yellow,
        _ => Color::Cyan,
    }
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
                        .fg(status_header_color(*status))
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

fn render_task_detail_screen(
    f: &mut Frame,
    area: Rect,
    app: &mut App,
    task_id: uuid::Uuid,
) -> Rect {
    let mut screen = app
        .task_detail
        .take()
        .filter(|screen| screen.task_id == task_id)
        .unwrap_or_else(|| TaskDetailScreen::new(task_id));
    let inner = screen.render(
        f,
        area,
        app.focus == Focus::TaskDetail,
        &app.tasks,
        &mut app.layout,
    );
    app.task_detail = Some(screen);
    inner
}

fn render_cores_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Cores;

    // Empty-config short-circuit: single block, no detail panel.
    if app.cores_screen.rows.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" cores ")
            .border_style(focus_border_style(focused));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let p = Paragraph::new("No ai_core blocks configured.")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        app.layout.cores_list_inner = Some(inner);
        return inner;
    }

    app.cores_screen.reclamp();

    // Split main area: list on top, detail panel on bottom. Detail
    // panel height scales with number of agents shown (capped).
    let selected_row = app.cores_screen.rows.get(app.cores_screen.cursor);
    let agents = selected_row
        .map(|row| agents_using_core(&app.config, &row.id))
        .unwrap_or_default();
    let agent_rows = agents.len().min(6) as u16;
    // borders (2) + "Used by:" header (1) + rows (0..=6)
    let detail_height = (3 + agent_rows.max(1)).clamp(5, 10);

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(detail_height)])
        .split(area);
    let list_area = split[0];
    let detail_area = split[1];

    let list_inner = render_cores_list(f, list_area, app, focused);
    render_cores_detail(f, detail_area, app, selected_row, &agents);

    app.layout.cores_list_inner = Some(list_inner);
    list_inner
}

fn render_cores_list(f: &mut Frame, area: Rect, app: &App, focused: bool) -> Rect {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" cores ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pending = app.cores_screen.pending > 0;

    let header = Line::from(vec![
        Span::styled(
            format!("{:<18}", "ID"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<14}", "TYPE"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<22}", "CREDENTIAL"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<12}", "PROVIDER"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "USAGE",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let mut items: Vec<ListItem> = Vec::with_capacity(app.cores_screen.rows.len() + 1);
    items.push(ListItem::new(header));

    for (i, row) in app.cores_screen.rows.iter().enumerate() {
        let selected = i == app.cores_screen.cursor;
        items.push(render_core_row(
            row,
            app.cores_screen.usage.get(&row.id),
            pending,
            selected,
        ));
    }

    let list = List::new(items);
    let mut state = ListState::default();
    // Cursor maps to header (0) + one line per row.
    let cursor_line = 1 + app.cores_screen.cursor;
    state.select(Some(cursor_line));
    f.render_stateful_widget(list, inner, &mut state);

    inner
}

fn render_cores_detail(
    f: &mut Frame,
    area: Rect,
    app: &App,
    selected: Option<&CoreRow>,
    agents: &[&str],
) {
    let title = match selected {
        Some(row) if is_team_default(&app.config, &row.id) => {
            format!(" {} (team default) ", row.id)
        }
        Some(row) => format!(" {} ", row.id),
        None => " core ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    if agents.is_empty() {
        lines.push(Line::from(Span::styled(
            "No agents use this core.",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Used by:",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        let visible_rows = (inner.height.saturating_sub(1)) as usize;
        let shown = agents.len().min(visible_rows);
        for agent in &agents[..shown] {
            lines.push(Line::from(Span::raw(format!("  • {}", agent))));
        }
        if shown < agents.len() {
            lines.push(Line::from(Span::styled(
                format!("  … {} more", agents.len() - shown),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);
}

fn render_inbox_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    use super::inbox_view::{ForwardStage, InboxHotspot, InboxMode};
    let focused = app.focus == Focus::Inbox;

    // Clamp before rendering so a stale cursor from a deletion /
    // concurrent mutation doesn't hang off the end.
    app.inbox_screen.reclamp(app.inbox.len());

    // Empty-inbox short-circuit.
    if app.inbox.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" inbox ")
            .border_style(focus_border_style(focused));
        let inner = block.inner(area);
        f.render_widget(block, area);
        let p = Paragraph::new(
            "Inbox is empty. Agents can reach you by addressing messages to `user`.",
        )
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });
        f.render_widget(p, inner);
        return inner;
    }

    // Layout: toolbar (1 row) + list + detail panel (or overlay).
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Percentage(50),
        ])
        .split(area);
    let toolbar_area = split[0];
    let list_area = split[1];
    let detail_area = split[2];

    render_inbox_toolbar(f, toolbar_area, app);

    // List pane
    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" inbox ({}) ", app.inbox.len()))
        .border_style(focus_border_style(focused));
    let list_inner = list_block.inner(list_area);
    f.render_widget(list_block, list_area);
    app.layout.inbox_list_inner = Some(list_inner);

    // Render each row manually so we can register one hotspot per row.
    for (i, msg) in app.inbox.iter().enumerate() {
        let row_y = list_inner.y + i as u16;
        if row_y >= list_inner.y + list_inner.height {
            break;
        }
        let row_rect = Rect {
            x: list_inner.x,
            y: row_y,
            width: list_inner.width,
            height: 1,
        };
        let selected = i == app.inbox_screen.cursor;
        let style = if selected && focused {
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else if selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let preview: String = msg
            .body
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        let line = Line::from(vec![
            Span::styled(format!("{:<16} ", msg.from), style),
            Span::styled(preview, style),
        ]);
        f.render_widget(Paragraph::new(line).style(style), row_rect);
        app.layout
            .inbox_hotspots
            .push((row_rect, InboxHotspot::SelectRow(i)));
    }

    // Detail pane / overlay
    match &app.inbox_screen.mode {
        InboxMode::List => {
            let detail_focused =
                focused && app.inbox_screen.pane == super::inbox_view::InboxPane::Detail;
            let detail_block = Block::default()
                .borders(Borders::ALL)
                .title(" detail ")
                .border_style(if detail_focused {
                    focus_border_style(true)
                } else {
                    Style::default().fg(Color::DarkGray)
                });
            let inner = detail_block.inner(detail_area);
            app.layout.inbox_detail_inner = Some(inner);
            f.render_widget(detail_block, detail_area);
            if let Some(msg) = app.inbox.get(app.inbox_screen.cursor) {
                let mut lines: Vec<Line> = Vec::new();
                let age_secs = msg.received_at.elapsed().as_secs();
                let age = if age_secs < 60 {
                    format!("{age_secs}s ago")
                } else if age_secs < 3600 {
                    format!("{}m ago", age_secs / 60)
                } else {
                    format!("{}h ago", age_secs / 3600)
                };
                lines.push(Line::from(vec![
                    Span::styled("From: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(msg.from.clone()),
                    Span::styled(format!("   ({age})"), Style::default().fg(Color::DarkGray)),
                ]));
                lines.push(Line::from(""));
                for body_line in msg.body.lines() {
                    lines.push(Line::from(body_line.to_string()));
                }
                // Clamp the scroll so the user can't lose the body
                // off the top by overshooting at the bottom. The body
                // wraps, so this is an under-estimate of the rendered
                // row count — but it's better than letting the offset
                // grow without bound.
                let total_rows = lines.len() as u16;
                let max_scroll = total_rows.saturating_sub(inner.height.max(1));
                if app.inbox_screen.detail_scroll > max_scroll {
                    app.inbox_screen.detail_scroll = max_scroll;
                }
                let p = Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .scroll((app.inbox_screen.detail_scroll, 0));
                f.render_widget(p, inner);
            }
        }
        InboxMode::Reply { to, body } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .title(format!(" reply → {to} "))
                .border_style(Style::default().fg(Color::Yellow));
            let inner = block.inner(detail_area);
            f.render_widget(block, detail_area);
            // Reserve the bottom line of the inner area for the
            // [ send ] [ cancel ] button row.
            let body_height = inner.height.saturating_sub(1);
            let body_area = Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: body_height,
            };
            let button_area = Rect {
                x: inner.x,
                y: inner.y + body_height,
                width: inner.width,
                height: 1,
            };
            let p = Paragraph::new(body.clone()).wrap(Wrap { trim: false });
            f.render_widget(p, body_area);
            render_inbox_modal_buttons(f, button_area, app);
        }
        InboxMode::Forward(fwd) => {
            let title = match fwd.stage {
                ForwardStage::PickRecipient => " forward → pick recipient ".to_string(),
                ForwardStage::EditBody => format!(
                    " forward → {} ",
                    fwd.candidates
                        .get(fwd.selected)
                        .cloned()
                        .unwrap_or_default()
                ),
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Yellow));
            let inner = block.inner(detail_area);
            f.render_widget(block, detail_area);

            // Always reserve the bottom line for the button row.
            let body_height = inner.height.saturating_sub(1);
            let body_area = Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: body_height,
            };
            let button_area = Rect {
                x: inner.x,
                y: inner.y + body_height,
                width: inner.width,
                height: 1,
            };

            match fwd.stage {
                ForwardStage::PickRecipient => {
                    for (i, id) in fwd.candidates.iter().enumerate() {
                        let row_y = body_area.y + i as u16;
                        if row_y >= body_area.y + body_area.height {
                            break;
                        }
                        let row_rect = Rect {
                            x: body_area.x,
                            y: row_y,
                            width: body_area.width,
                            height: 1,
                        };
                        let style = if i == fwd.selected {
                            Style::default()
                                .bg(Color::Cyan)
                                .fg(Color::Black)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };
                        f.render_widget(
                            Paragraph::new(Line::from(Span::styled(id.clone(), style)))
                                .style(style),
                            row_rect,
                        );
                        app.layout
                            .inbox_hotspots
                            .push((row_rect, InboxHotspot::ForwardRecipient(i)));
                    }
                }
                ForwardStage::EditBody => {
                    // Top half: editable note. Bottom half: read-only
                    // quoted original, dimmed so it visually reads as
                    // context the user can't edit. A single-line
                    // separator labels each pane.
                    let total = body_area.height;
                    // Reserve at least 3 rows for the note (label + 2
                    // text rows) and at least 2 rows for the quoted
                    // pane (label + 1 text row). On extreme squeezes
                    // we collapse the quoted pane and keep the note.
                    let note_rows = total.saturating_sub(4).max(3).min(total);
                    let quoted_rows = total.saturating_sub(note_rows);

                    let note_label_area = Rect {
                        x: body_area.x,
                        y: body_area.y,
                        width: body_area.width,
                        height: 1.min(note_rows),
                    };
                    let note_text_area = Rect {
                        x: body_area.x,
                        y: body_area.y + note_label_area.height,
                        width: body_area.width,
                        height: note_rows.saturating_sub(note_label_area.height),
                    };
                    f.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            "Note (sent before the forwarded message):",
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        ))),
                        note_label_area,
                    );
                    let note_display = if fwd.note.is_empty() {
                        Paragraph::new(Span::styled(
                            "(type a note, or send blank to forward as-is)",
                            Style::default().fg(Color::DarkGray),
                        ))
                    } else {
                        Paragraph::new(fwd.note.clone()).wrap(Wrap { trim: false })
                    };
                    f.render_widget(note_display, note_text_area);

                    if quoted_rows > 0 {
                        let quoted_label_area = Rect {
                            x: body_area.x,
                            y: body_area.y + note_rows,
                            width: body_area.width,
                            height: 1.min(quoted_rows),
                        };
                        let quoted_text_area = Rect {
                            x: body_area.x,
                            y: body_area.y + note_rows + quoted_label_area.height,
                            width: body_area.width,
                            height: quoted_rows.saturating_sub(quoted_label_area.height),
                        };
                        f.render_widget(
                            Paragraph::new(Line::from(Span::styled(
                                "Forwarded (read-only):",
                                Style::default()
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::BOLD),
                            ))),
                            quoted_label_area,
                        );
                        let p = Paragraph::new(fwd.quoted.clone())
                            .style(Style::default().fg(Color::DarkGray))
                            .wrap(Wrap { trim: false });
                        f.render_widget(p, quoted_text_area);
                    }
                }
            }

            // Pick-recipient stage has no Send button (use Enter / click a row
            // to advance), but Cancel is always available.
            render_inbox_modal_buttons(f, button_area, app);
        }
    }

    // Status toast (rendered on the bottom line of the list pane).
    if let Some(msg) = &app.inbox_screen.status_msg {
        let area_status = Rect {
            x: list_inner.x,
            y: list_inner.y + list_inner.height.saturating_sub(1),
            width: list_inner.width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Yellow),
            )),
            area_status,
        );
    }

    list_inner
}

/// Top toolbar above the inbox list: clickable Reply / Forward / Delete
/// text buttons. Each button registers an `InboxHotspot` so mouse
/// clicks land on the same action as the keyboard shortcut.
fn render_inbox_toolbar(f: &mut Frame, area: Rect, app: &mut App) {
    use super::inbox_view::InboxHotspot;
    // Gray out buttons that do nothing (e.g. no selection). Keeps the
    // layout constant so button rects don't shift between frames.
    let has_selection = !app.inbox.is_empty();
    let enabled = |on: bool| {
        if on {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        }
    };

    let labels: [(&str, InboxHotspot, bool); 3] = [
        (" reply ", InboxHotspot::ReplyButton, has_selection),
        (" forward ", InboxHotspot::ForwardButton, has_selection),
        (" delete ", InboxHotspot::DeleteButton, has_selection),
    ];

    let mut x = area.x;
    for (label, hotspot, on) in labels {
        let width = label.chars().count() as u16;
        if x + width > area.x + area.width {
            break;
        }
        let rect = Rect {
            x,
            y: area.y,
            width,
            height: 1,
        };
        let span = Span::styled(label.to_string(), enabled(on));
        f.render_widget(Paragraph::new(Line::from(span)), rect);
        if on {
            app.layout.inbox_hotspots.push((rect, hotspot));
        }
        x += width + 1; // one space between buttons
    }
}

/// Bottom-row [ send ] [ cancel ] button bar for the reply/forward
/// overlays. `send` is omitted when it would be a no-op (forward
/// picker stage).
fn render_inbox_modal_buttons(f: &mut Frame, area: Rect, app: &mut App) {
    use super::inbox_view::{ForwardStage, InboxHotspot, InboxMode};
    let show_send = match &app.inbox_screen.mode {
        InboxMode::Reply { .. } => true,
        InboxMode::Forward(fwd) => matches!(fwd.stage, ForwardStage::EditBody),
        InboxMode::List => false,
    };

    let primary = Style::default()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let secondary = Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    let mut x = area.x;
    let mut place =
        |label: &str, style: Style, hotspot: InboxHotspot, app: &mut App, x: &mut u16| {
            let width = label.chars().count() as u16;
            if *x + width > area.x + area.width {
                return;
            }
            let rect = Rect {
                x: *x,
                y: area.y,
                width,
                height: 1,
            };
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(label.to_string(), style))),
                rect,
            );
            app.layout.inbox_hotspots.push((rect, hotspot));
            *x += width + 1;
        };

    if show_send {
        place(" send ", primary, InboxHotspot::SendButton, app, &mut x);
    }
    place(
        " cancel ",
        secondary,
        InboxHotspot::CancelButton,
        app,
        &mut x,
    );
}

fn render_core_row(
    row: &CoreRow,
    snapshot: Option<&crate::usage::UsageSnapshot>,
    pending: bool,
    selected: bool,
) -> ListItem<'static> {
    let provider_label = match row.usage_provider {
        Some(crate::config::UsageProvider::Zai) => "zai",
        Some(crate::config::UsageProvider::ChatgptOauth) => "chatgpt",
        Some(crate::config::UsageProvider::Openrouter) => "openrouter",
        Some(crate::config::UsageProvider::GithubCopilot) => "copilot",
        // claudeCode cores feed their reset data from `rate_limit_event`
        // stream lines rather than an HTTP fetcher, so they have no
        // provider but are still annotated distinctly.
        None if row.kind_label == "claudeCode" => "stream",
        None => "—",
    };
    let usage_cell = format_usage_cell(row.usage_provider, snapshot, pending);
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let credential_cell = row.credential.as_deref().unwrap_or("—");
    let line = format!(
        "{:<18}{:<14}{:<22}{:<12}{}",
        truncate(&row.id, 17),
        row.kind_label,
        truncate(credential_cell, 21),
        provider_label,
        usage_cell,
    );
    ListItem::new(Line::from(Span::styled(line, style)))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
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
    let assignee_raw = t.assignee.as_deref().unwrap_or("-");
    let assignee_decorated = if assignee_raw == "-" {
        "-".to_string()
    } else {
        super::tasks_view::decorate_assignee_label(assignee_raw, &app.config)
    };
    let assignee_label = format!(" {} ", assignee_decorated);
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
                } else if raw_value.is_empty() && *field == FormField::PlannedAssignee {
                    "(no target)".to_string()
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
        // Approve / Reject get distinct colors so the destructive
        // and approving paths read at a glance; Ok / Cancel keep
        // the original cyan.
        let accent = match field {
            FormField::Approve => Color::Green,
            FormField::Reject => Color::Red,
            _ => Color::Cyan,
        };
        let style = if is_focused {
            Style::default()
                .bg(accent)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(accent).add_modifier(Modifier::BOLD)
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

pub(super) fn render_text_popover(
    f: &mut Frame,
    area: Rect,
    title: String,
    body_text: String,
    scroll: u16,
) -> (u16, u16) {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical[1]);
    let rect = horizontal[1];
    if rect.width < 4 || rect.height < 4 {
        return (0, 0);
    }
    render_text_popover_at(f, rect, title, body_text, scroll)
}

fn render_text_popover_at(
    f: &mut Frame,
    rect: Rect,
    title: String,
    body_text: String,
    scroll: u16,
) -> (u16, u16) {
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .title_bottom(" j/k scroll · Esc/q close ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let paragraph = Paragraph::new(body_text).wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(inner.width)
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16;
    let scroll = scroll.min(max_scroll);
    f.render_widget(paragraph.scroll((scroll, 0)), inner);
    (inner.height, max_scroll)
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
            } else if s.is_empty() && picker_field == FormField::PlannedAssignee {
                "(no target)".len()
            } else if matches!(
                picker_field,
                FormField::Assignee | FormField::PlannedAssignee
            ) {
                super::tasks_view::decorate_assignee_label(s, &app.config).len()
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
        FormField::PlannedAssignee => " pick handoff target ",
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
            } else if o.is_empty() && picker_field == FormField::PlannedAssignee {
                "(no target)".to_string()
            } else if o.is_empty() {
                "-".to_string()
            } else if matches!(
                picker_field,
                FormField::Assignee | FormField::PlannedAssignee
            ) {
                super::tasks_view::decorate_assignee_label(o, &app.config)
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

// ========= Memory view =========

fn render_memory_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Memory;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" memory ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let stage_is_banks = matches!(app.memory_screen.stage, MemoryStage::Banks { .. });
    if stage_is_banks {
        render_memory_banks(f, inner, app, focused);
    } else {
        render_memory_bank(f, inner, app, focused);
    }
    inner
}

fn render_memory_banks(f: &mut Frame, area: Rect, app: &mut App, focused: bool) {
    if app.config.memory_banks.is_empty() {
        let p = Paragraph::new("No memory banks declared in the team config.")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    }
    let selected = match app.memory_screen.stage {
        MemoryStage::Banks { selected } => selected,
        _ => 0,
    };

    let mut bank_rects: Vec<Rect> = Vec::with_capacity(app.config.memory_banks.len());
    let items: Vec<ListItem> = app
        .config
        .memory_banks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let is_sel = i == selected && focused;
            let style = if is_sel {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let spans = Line::from(vec![
                Span::styled(format!("  {}  ", b.id), style),
                Span::styled(
                    format!("· {} ", b.description),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            // Row rect is recorded lazily below — `List` handles draw.
            let _ = (&mut bank_rects, area);
            ListItem::new(spans)
        })
        .collect();
    for i in 0..app.config.memory_banks.len() {
        let y = area.y.saturating_add(i as u16);
        if y >= area.y + area.height {
            break;
        }
        bank_rects.push(Rect::new(area.x, y, area.width, 1));
    }
    app.layout.memory_bank_rows = bank_rects;

    let list = List::new(items);
    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, area, &mut state);

    if let Some(msg) = &app.memory_screen.status_msg {
        let bot = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
        f.render_widget(
            Paragraph::new(msg.clone()).style(Style::default().fg(Color::Yellow)),
            bot,
        );
    }
}

fn render_memory_bank(f: &mut Frame, area: Rect, app: &mut App, focused: bool) {
    // Layout: title row (bank id) + search bar + results list + status.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // bank header
            Constraint::Length(3), // search box
            Constraint::Min(3),    // results
            Constraint::Length(1), // status
        ])
        .split(area);

    let (bank_id, query, query_cursor, results, selected, bank_focus, pending) =
        match app.memory_screen.bank() {
            Some(b) => (
                b.bank_id.clone(),
                b.search.query.clone(),
                b.search.cursor,
                b.results.clone(),
                b.selected_result,
                b.focus,
                app.memory_screen.pending.clone(),
            ),
            None => return,
        };

    // Bank header.
    let header = Line::from(vec![
        Span::styled(" bank: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            bank_id,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} result(s)", results.len()),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(header), layout[0]);

    // Search box.
    let search_focused = focused && bank_focus == BankFocus::Search;
    let search_block = Block::default()
        .borders(Borders::ALL)
        .title(" search ")
        .border_style(focus_border_style(search_focused));
    let search_inner = search_block.inner(layout[1]);
    f.render_widget(search_block, layout[1]);
    let before = &query[..query_cursor.min(query.len())];
    let after = &query[query_cursor.min(query.len())..];
    let spans = if search_focused {
        vec![
            Span::raw(before.to_string()),
            Span::styled("▏", Style::default().fg(Color::Cyan)),
            Span::raw(after.to_string()),
        ]
    } else if query.is_empty() {
        vec![Span::styled(
            "(semantic query, leave blank to list newest)",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        vec![Span::raw(query.clone())]
    };
    f.render_widget(Paragraph::new(Line::from(spans)), search_inner);
    app.layout.memory_search_rect = Some(layout[1]);

    // Results list.
    let results_focused = focused && bank_focus == BankFocus::Results;
    let results_block = Block::default()
        .borders(Borders::ALL)
        .title(" results ")
        .border_style(focus_border_style(results_focused));
    let results_inner = results_block.inner(layout[2]);
    f.render_widget(results_block, layout[2]);

    if results.is_empty() {
        let msg = if query.trim().is_empty() {
            "No entries in this bank yet. Press `a` to add one."
        } else {
            "No matches. Try a different query or press `a` to add."
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            results_inner,
        );
    } else {
        let mut result_rects: Vec<Rect> = Vec::with_capacity(results.len());
        let items: Vec<ListItem> = results
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let is_sel = i == selected && results_focused;
                let base = if is_sel {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let short = h.id.to_string();
                let short = &short[..8.min(short.len())];
                let title = if h.title.is_empty() {
                    h.body.lines().next().unwrap_or("").to_string()
                } else {
                    h.title.clone()
                };
                let dist = if h.distance.is_nan() {
                    String::new()
                } else {
                    format!("  d={:.3}", h.distance)
                };
                let tags = if h.tags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", h.tags.join(","))
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {short} "), base),
                    Span::styled(title, base),
                    Span::styled(tags, Style::default().fg(Color::DarkGray)),
                    Span::styled(dist, Style::default().fg(Color::DarkGray)),
                ]))
            })
            .collect();
        for i in 0..results.len() {
            let y = results_inner.y.saturating_add(i as u16);
            if y >= results_inner.y + results_inner.height {
                break;
            }
            result_rects.push(Rect::new(results_inner.x, y, results_inner.width, 1));
        }
        app.layout.memory_result_rows = result_rects;

        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(selected));
        f.render_stateful_widget(list, results_inner, &mut state);
    }

    // Status / pending.
    let status = layout[3];
    let line = if let Some(label) = pending {
        Line::from(Span::styled(
            format!(" … {label}"),
            Style::default().fg(Color::Yellow),
        ))
    } else if let Some(msg) = &app.memory_screen.status_msg {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from(Span::styled(
            " / search · a add · e edit · d del · l links · ⏎ detail · esc banks",
            Style::default().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(line), status);
}

fn render_memory_modals(f: &mut Frame, area: Rect, app: &mut App) {
    // Confirm delete prompt (piggy-backs on the main area's bottom row).
    let confirm = app.memory_screen.bank().and_then(|b| b.confirm_delete);
    if let Some(id) = confirm {
        let rect = Rect::new(area.x, area.y + area.height - 1, area.width, 1);
        let short = id.to_string();
        let short = &short[..8.min(short.len())];
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" Delete {short}? "),
                    Style::default().fg(Color::Black).bg(Color::Yellow),
                ),
                Span::raw(" y/N "),
            ])),
            rect,
        );
        return;
    }

    let form_open = app
        .memory_screen
        .bank()
        .map(|b| b.form.is_some())
        .unwrap_or(false);
    if form_open {
        render_memory_form(f, area, app);
        return;
    }

    let links_open = app
        .memory_screen
        .bank()
        .map(|b| b.links_overlay.is_some())
        .unwrap_or(false);
    if links_open {
        render_memory_links_overlay(f, area, app);
        return;
    }

    let detail_open = app
        .memory_screen
        .bank()
        .map(|b| b.detail.is_some())
        .unwrap_or(false);
    if detail_open {
        render_memory_detail(f, area, app);
    }
}

fn render_memory_form(f: &mut Frame, area: Rect, app: &mut App) {
    let w = area.width.saturating_sub(4).min(100);
    let h = area.height.saturating_sub(2).min(24);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" note ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    app.layout.memory_modal_outer = Some(rect);

    let form = match app.memory_screen.bank().and_then(|b| b.form.as_ref()) {
        Some(f) => f.clone(),
        None => return,
    };

    // Tags row grows to fit 1 line; body takes everything left.
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(6),    // Body (expands)
            Constraint::Length(3), // Tags
            Constraint::Length(1), // Error
            Constraint::Length(1), // Buttons
        ])
        .split(inner);

    render_memory_form_line(f, sections[0], NoteField::Title, &form, app);
    render_memory_form_body(f, sections[1], &form, app);
    render_memory_form_line(f, sections[2], NoteField::Tags, &form, app);

    if let Some(err) = &form.error {
        f.render_widget(
            Paragraph::new(format!(" ! {err}")).style(Style::default().fg(Color::Red)),
            sections[3],
        );
    }

    // Buttons row.
    let btn_row = sections[4];
    let btn_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(12),
            Constraint::Length(12),
        ])
        .split(btn_row);
    let ok_style = if form.focused_field == NoteField::Ok {
        Style::default()
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let cancel_style = if form.focused_field == NoteField::Cancel {
        Style::default()
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(" [ OK ] ", ok_style))),
        btn_layout[1],
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(" [ Cancel ] ", cancel_style))),
        btn_layout[2],
    );
    app.layout
        .memory_form_fields
        .push((NoteField::Ok, btn_layout[1]));
    app.layout
        .memory_form_fields
        .push((NoteField::Cancel, btn_layout[2]));
}

fn render_memory_form_line(
    f: &mut Frame,
    area: Rect,
    field: NoteField,
    form: &NoteForm,
    app: &mut App,
) {
    let focused = form.focused_field == field;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", field.label()))
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.layout.memory_form_fields.push((field, area));

    let (text, cursor) = match field {
        NoteField::Title => (form.title.clone(), form.title_cursor),
        NoteField::Tags => (form.tags.clone(), form.tags_cursor),
        _ => (String::new(), 0),
    };
    let before = &text[..cursor.min(text.len())];
    let after = &text[cursor.min(text.len())..];
    let spans = if focused {
        vec![
            Span::raw(before.to_string()),
            Span::styled("▏", Style::default().fg(Color::Cyan)),
            Span::raw(after.to_string()),
        ]
    } else {
        vec![Span::raw(text)]
    };
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_memory_form_body(f: &mut Frame, area: Rect, form: &NoteForm, app: &mut App) {
    let focused = form.focused_field == NoteField::Body;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Body ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.layout.memory_form_fields.push((NoteField::Body, area));

    let wrap_width = inner.width;
    // Update the form's cached wrap width for key-handler consistency.
    if let Some(f_mut) = app.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
        f_mut.body_last_wrap_width = wrap_width;
        f_mut.body_last_visible_lines = inner.height;
        f_mut.body_ensure_visible(inner.height);
    }

    // Render wrapped body with a simple cursor marker at the focused
    // position. Using ratatui's Paragraph with Wrap::trim false handles
    // word-wrap; the cursor glyph is just a visual hint.
    let paragraph = Paragraph::new(form.body.clone()).wrap(Wrap { trim: false });
    f.render_widget(paragraph, inner);

    if focused && inner.height > 0 {
        // Best-effort cursor: byte cursor → line + col assuming no wrap.
        let before = &form.body[..form.body_cursor.min(form.body.len())];
        let line_idx = before.chars().filter(|c| *c == '\n').count() as u16;
        let col = before
            .rsplit('\n')
            .next()
            .map(|l| l.chars().count())
            .unwrap_or(0) as u16;
        let scroll = form.body_scroll;
        if line_idx >= scroll && line_idx < scroll + inner.height {
            let y = inner.y + (line_idx - scroll);
            let x = inner.x + col.min(inner.width.saturating_sub(1));
            f.set_cursor_position((x, y));
        }
    }
}

fn render_memory_detail(f: &mut Frame, area: Rect, app: &mut App) {
    let w = area.width.saturating_sub(4).min(100);
    let h = area.height.saturating_sub(2).min(30);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" note detail ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    app.layout.memory_modal_outer = Some(rect);

    let detail: NoteDetail = match app
        .memory_screen
        .bank()
        .and_then(|b| b.detail.as_ref())
        .cloned()
    {
        Some(d) => d,
        None => return,
    };

    if detail.entry.is_none() {
        f.render_widget(
            Paragraph::new("loading…").style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }
    let hit = detail.entry.as_ref().unwrap();

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title line
            Constraint::Length(1), // meta line
            Constraint::Min(3),    // body
            Constraint::Length(1), // blank
            Constraint::Length(1), // outgoing header
            Constraint::Min(1),    // outgoing list
            Constraint::Length(1), // incoming header
            Constraint::Min(1),    // incoming list
            Constraint::Length(1), // footer hints
        ])
        .split(inner);

    let title_line = if hit.title.is_empty() {
        Line::from(Span::styled(
            " (no title) ",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            format!(" {} ", hit.title),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
    };
    f.render_widget(Paragraph::new(title_line), sections[0]);

    let meta = Line::from(vec![
        Span::styled(
            format!(" {} ", hit.id),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            hit.created_at.format(" · %Y-%m-%d %H:%M ").to_string(),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            if hit.tags.is_empty() {
                String::new()
            } else {
                format!(" · [{}] ", hit.tags.join(","))
            },
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(meta), sections[1]);

    let body = Paragraph::new(hit.body.clone())
        .wrap(Wrap { trim: false })
        .scroll((detail.scroll, 0));
    f.render_widget(body, sections[2]);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " outgoing links ",
            Style::default().fg(Color::DarkGray),
        ))),
        sections[4],
    );
    render_memory_link_list(f, sections[5], &detail.out_links);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " incoming links ",
            Style::default().fg(Color::DarkGray),
        ))),
        sections[6],
    );
    render_memory_link_list(f, sections[7], &detail.in_links);

    f.render_widget(
        Paragraph::new(" e edit · d del · l links · j/k scroll · esc close ")
            .style(Style::default().fg(Color::DarkGray)),
        sections[8],
    );
}

fn render_memory_link_list(f: &mut Frame, area: Rect, links: &[crate::memory::Link]) {
    if links.is_empty() {
        f.render_widget(
            Paragraph::new("  (none)").style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = links
        .iter()
        .map(|l| {
            let short = l.peer_id.to_string();
            let short = &short[..8.min(short.len())];
            let kind = if l.kind.is_empty() {
                String::new()
            } else {
                format!("[{}] ", l.kind)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {short}  "), Style::default().fg(Color::DarkGray)),
                Span::raw(kind),
                Span::raw(l.peer_title.clone()),
            ]))
        })
        .collect();
    let list = List::new(items);
    f.render_widget(list, area);
}

fn render_memory_links_overlay(f: &mut Frame, area: Rect, app: &mut App) {
    let w = area.width.saturating_sub(4).min(100);
    let h = area.height.saturating_sub(2).min(26);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" links ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    app.layout.memory_modal_outer = Some(rect);

    let overlay: LinksOverlay = match app
        .memory_screen
        .bank()
        .and_then(|b| b.links_overlay.as_ref())
        .cloned()
    {
        Some(o) => o,
        None => return,
    };

    if overlay.picker.is_some() {
        render_memory_link_picker(f, inner, &overlay, app);
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // source id header
            Constraint::Min(3),    // existing links list
            Constraint::Length(1), // footer
        ])
        .split(inner);

    let src = overlay.source_id.to_string();
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" source: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                src,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        sections[0],
    );

    if overlay.existing.is_empty() {
        f.render_widget(
            Paragraph::new("  No outgoing links. Press `a` / `+` to add one.")
                .style(Style::default().fg(Color::DarkGray)),
            sections[1],
        );
    } else {
        let mut link_rects: Vec<Rect> = Vec::with_capacity(overlay.existing.len());
        let items: Vec<ListItem> = overlay
            .existing
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let is_sel = i == overlay.cursor;
                let base = if is_sel {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let short = l.peer_id.to_string();
                let short = &short[..8.min(short.len())];
                let kind = if l.kind.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", l.kind)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {short}"), base),
                    Span::styled(kind, base),
                    Span::styled(format!("  {}", l.peer_title), base),
                ]))
            })
            .collect();
        for i in 0..overlay.existing.len() {
            let y = sections[1].y.saturating_add(i as u16);
            if y >= sections[1].y + sections[1].height {
                break;
            }
            link_rects.push(Rect::new(sections[1].x, y, sections[1].width, 1));
        }
        app.layout.memory_link_rows = link_rects;
        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(overlay.cursor));
        f.render_stateful_widget(list, sections[1], &mut state);
    }

    f.render_widget(
        Paragraph::new(" a/+ add · d/- unlink · j/k nav · esc close ")
            .style(Style::default().fg(Color::DarkGray)),
        sections[2],
    );
}

fn render_memory_link_picker(f: &mut Frame, area: Rect, overlay: &LinksOverlay, _app: &mut App) {
    let picker = match overlay.picker.as_ref() {
        Some(p) => p,
        None => return,
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // query box
            Constraint::Min(3),    // results
            Constraint::Length(3), // kind box
            Constraint::Length(1), // footer
        ])
        .split(area);

    // Query box.
    let q_focused = picker.focus == LinkPickerFocus::Query;
    let q_block = Block::default()
        .borders(Borders::ALL)
        .title(" search target ")
        .border_style(focus_border_style(q_focused));
    let q_inner = q_block.inner(sections[0]);
    f.render_widget(q_block, sections[0]);
    let before = &picker.query[..picker.query_cursor.min(picker.query.len())];
    let after = &picker.query[picker.query_cursor.min(picker.query.len())..];
    let spans = if q_focused {
        vec![
            Span::raw(before.to_string()),
            Span::styled("▏", Style::default().fg(Color::Cyan)),
            Span::raw(after.to_string()),
        ]
    } else if picker.query.is_empty() {
        vec![Span::styled(
            "(type to search; leave blank for latest)",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        vec![Span::raw(picker.query.clone())]
    };
    f.render_widget(Paragraph::new(Line::from(spans)), q_inner);

    // Results list.
    let r_focused = picker.focus == LinkPickerFocus::Results;
    let r_block = Block::default()
        .borders(Borders::ALL)
        .title(" results ")
        .border_style(focus_border_style(r_focused));
    let r_inner = r_block.inner(sections[1]);
    f.render_widget(r_block, sections[1]);
    if picker.results.is_empty() {
        f.render_widget(
            Paragraph::new("  No matches. Press ⏎ on the search to run.")
                .style(Style::default().fg(Color::DarkGray)),
            r_inner,
        );
    } else {
        let items: Vec<ListItem> = picker
            .results
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let is_sel = i == picker.selected && r_focused;
                let base = if is_sel {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let short = h.id.to_string();
                let short = &short[..8.min(short.len())];
                let title = if h.title.is_empty() {
                    h.body.lines().next().unwrap_or("").to_string()
                } else {
                    h.title.clone()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("  {short}  "), base),
                    Span::styled(title, base),
                ]))
            })
            .collect();
        let list = List::new(items);
        let mut state = ListState::default();
        state.select(Some(picker.selected));
        f.render_stateful_widget(list, r_inner, &mut state);
    }

    // Kind box.
    let k_focused = picker.focus == LinkPickerFocus::Kind;
    let k_block = Block::default()
        .borders(Borders::ALL)
        .title(" kind (optional) ")
        .border_style(focus_border_style(k_focused));
    let k_inner = k_block.inner(sections[2]);
    f.render_widget(k_block, sections[2]);
    let before = &picker.kind[..picker.kind_cursor.min(picker.kind.len())];
    let after = &picker.kind[picker.kind_cursor.min(picker.kind.len())..];
    let spans = if k_focused {
        vec![
            Span::raw(before.to_string()),
            Span::styled("▏", Style::default().fg(Color::Cyan)),
            Span::raw(after.to_string()),
        ]
    } else if picker.kind.is_empty() {
        vec![Span::styled(
            "e.g. see_also, supersedes, contradicts",
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        vec![Span::raw(picker.kind.clone())]
    };
    f.render_widget(Paragraph::new(Line::from(spans)), k_inner);

    f.render_widget(
        Paragraph::new(" tab next · ⏎ commit · esc cancel ")
            .style(Style::default().fg(Color::DarkGray)),
        sections[3],
    );
}

// ---------------------------------------------------------------------
// Workflows sidebar: rows, list renderer, and placeholder main-area
// views. Form + progress view rendering is fleshed out in task #3.
// ---------------------------------------------------------------------

use crate::workflow::InstanceStatus as WfInstanceStatus;

/// A single renderable row in the workflows panel. Instances appear
/// only beneath the header of the currently-selected workflow — same
/// pattern as channel members under the active channel.
pub(super) enum WorkflowRow {
    Header {
        workflow_idx: usize,
    },
    Instance {
        instance_id: u64,
        status: WfInstanceStatus,
    },
}

pub(super) fn workflow_rows(app: &App) -> Vec<WorkflowRow> {
    let mut rows: Vec<WorkflowRow> = Vec::with_capacity(app.config.workflows.len());
    // A workflow always expands whenever it has at least one
    // running / succeeded / failed instance — the operator should
    // be able to click those rows from any view. Workflows with no
    // live instances stay collapsed to keep the panel compact.
    for (wi, _wf) in app.config.workflows.iter().enumerate() {
        rows.push(WorkflowRow::Header { workflow_idx: wi });
        for inst in app
            .workflow_instances
            .iter()
            .filter(|i| i.workflow_idx == wi)
        {
            rows.push(WorkflowRow::Instance {
                instance_id: inst.id,
                status: inst.status,
            });
        }
    }
    rows
}

fn render_workflows_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Workflows;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" workflows ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    let rows = workflow_rows(app);
    // Which instance (if any) is the current main-area view — drives
    // both the persistent cursor highlight and the row style.
    let active_instance = match app.active_view {
        ActiveView::WorkflowInstance { instance_id, .. } => Some(instance_id),
        _ => None,
    };
    // Whether a workflow form is the active view — keeps the header
    // highlighted while the form is open, but not once we've moved
    // on to an instance view.
    let active_form_workflow = match app.active_view {
        ActiveView::WorkflowForm { workflow_idx } => Some(workflow_idx),
        _ => None,
    };
    let mut selected_row: Option<usize> = None;
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(row_idx, row)| match row {
            WorkflowRow::Header { workflow_idx } => {
                let i = *workflow_idx;
                let wf = &app.config.workflows[i];
                let highlight_header = active_form_workflow == Some(i)
                    || (active_instance.is_none() && focused && i == app.selected_workflow);
                let style = if highlight_header {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                // Only anchor the ListState cursor on the header
                // when there's no active instance — once we're
                // viewing an instance, the cursor lives on its row
                // instead (set below).
                if active_instance.is_none()
                    && !app.config.workflows.is_empty()
                    && i == app.selected_workflow.min(app.config.workflows.len() - 1)
                {
                    selected_row = Some(row_idx);
                }
                ListItem::new(Line::from(Span::styled(format!("▶ {}", wf.id), style)))
            }
            WorkflowRow::Instance {
                instance_id,
                status,
            } => {
                let is_active = active_instance == Some(*instance_id);
                if is_active {
                    selected_row = Some(row_idx);
                }
                let dot = match status {
                    WfInstanceStatus::Running => "🟡",
                    WfInstanceStatus::Failed => "🔴",
                    WfInstanceStatus::Succeeded => "🟢",
                };
                // Active instance: cyan-on-black bold so the cursor
                // is unmistakable. Failed but inactive: red fg.
                // Otherwise: default.
                let id_style = if is_active {
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else if matches!(status, WfInstanceStatus::Failed) {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled("↳ ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{dot} ")),
                    Span::styled(format!("#{instance_id}"), id_style),
                ]))
            }
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    if let Some(r) = selected_row {
        state.select(Some(r));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

/// Full property-entry form. Renders one row per visible property
/// (hidden ones are filtered at form construction), with Text/Number
/// edit cursors, Select arrow cycling, and MultiSelect checkbox rows.
/// Also shows a "Steps" preview so the operator can see what the
/// workflow will do before hitting Run. Two buttons at the bottom:
/// Run submits, Cancel returns to Chat.
fn render_workflow_form(f: &mut Frame, area: Rect, app: &mut App, workflow_idx: usize) -> Rect {
    use super::workflow_view::{FieldEditor, FormButton, FormFocus};

    // Clear previous frame's hit-test rects so we only ever hit-test
    // against geometry from the current draw.
    app.layout.workflow_form_buttons.clear();
    app.layout.workflow_form_fields.clear();
    app.layout.workflow_form_multi_rects.clear();
    app.layout.workflow_form_picker_rows.clear();
    app.layout.workflow_form_picker_outer = None;

    let focused = app.focus == Focus::Workflow;
    let Some(wf) = app.config.workflows.get(workflow_idx).cloned() else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" workflow: (unknown) ")
            .border_style(focus_border_style(focused));
        let inner = block.inner(area);
        f.render_widget(
            Paragraph::new("no such workflow")
                .style(Style::default().fg(Color::Red))
                .block(block),
            area,
        );
        return inner;
    };

    let title = format!(" workflow: {} ", wf.id);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(form) = &app.workflow_form else {
        f.render_widget(
            Paragraph::new("(no form state — press /run to start with defaults)")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return inner;
    };

    // Reserve 4 rows at the bottom of the inner rect for:
    //   row 0: (blank separator)
    //   row 1: [ Run ]  [ Cancel ]
    //   row 2: error message (if any) or blank
    //   row 3: hint line
    // Place the buttons at a fixed y so they're always visible, even
    // when the content above overflows.
    const FOOTER_ROWS: u16 = 4;
    let content_height = inner.height.saturating_sub(FOOTER_ROWS);
    let footer_y = inner.y + content_height;

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        wf.description.clone(),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::raw("")));

    if form.fields.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no user-visible properties — this workflow runs with declared defaults only)",
            Style::default().fg(Color::DarkGray),
        )));
        lines.push(Line::from(Span::raw("")));
    }

    let content_bottom = inner.y + content_height;
    for (i, field) in form.fields.iter().enumerate() {
        let is_focused = matches!(form.focus, FormFocus::Field(fi) if fi == i);
        let label_style = if is_focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let name = field.name();
        // Record the y at which this field's label lands so the mouse
        // handler can hit-test it. Checkboxes get their own rects
        // captured alongside each option row below.
        let field_start_y = inner.y + lines.len() as u16;
        lines.push(Line::from(vec![Span::styled(
            format!("{} {name}:", if is_focused { "▶" } else { " " }),
            label_style,
        )]));
        match field {
            FieldEditor::Text { value, cursor, .. } | FieldEditor::Number { value, cursor, .. } => {
                let rendered = render_text_input(value, *cursor, is_focused);
                let mut spans: Vec<Span> = vec![Span::raw("    ")];
                spans.extend(rendered);
                lines.push(Line::from(spans));
            }
            FieldEditor::Select {
                selected, options, ..
            } => {
                let picked = options.get(*selected).cloned().unwrap_or_default();
                let hint = if is_focused {
                    " (click / ⏎ to pick · ←/→ cycle)"
                } else {
                    ""
                };
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(format!("◉ {picked} ▾"), Style::default().fg(Color::Yellow)),
                    Span::styled(hint, Style::default().fg(Color::DarkGray)),
                ]));
            }
            FieldEditor::MultiSelect {
                chosen, options, ..
            } => {
                let cursor_idx = form.multi_cursor(i).min(options.len().saturating_sub(1));
                for (oi, opt) in options.iter().enumerate() {
                    let row_y = inner.y + lines.len() as u16;
                    let mark = if chosen.get(oi).copied().unwrap_or(false) {
                        "☑"
                    } else {
                        "☐"
                    };
                    let style = if is_focused && oi == cursor_idx {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(format!("{mark} {opt}"), style),
                    ]));
                    if row_y < content_bottom {
                        app.layout.workflow_form_multi_rects.push((
                            (i, oi),
                            Rect {
                                x: inner.x,
                                y: row_y,
                                width: inner.width,
                                height: 1,
                            },
                        ));
                    }
                }
                if is_focused {
                    lines.push(Line::from(Span::styled(
                        "    (↑/↓ move · space or click to toggle)",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }
        // Record the whole-field rect (label + value/checkbox rows,
        // excluding the trailing blank separator) clamped to the
        // visible content area.
        let field_end_y = inner.y + lines.len() as u16;
        if field_start_y < content_bottom {
            let clipped_end = field_end_y.min(content_bottom);
            app.layout.workflow_form_fields.push((
                i,
                Rect {
                    x: inner.x,
                    y: field_start_y,
                    width: inner.width,
                    height: clipped_end.saturating_sub(field_start_y),
                },
            ));
        }
        lines.push(Line::from(Span::raw("")));
    }

    // Steps preview — shows the operator what the workflow will
    // actually do once Run is pressed. Concise one-liner per step.
    lines.push(Line::from(Span::styled(
        "Steps",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    if wf.steps.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no steps declared — Run will be a no-op)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (i, step) in wf.steps.iter().enumerate() {
            let (kind, detail) = step_preview(&step.kind);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}. ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    step.id.clone(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" [{kind}]"), Style::default().fg(Color::Magenta)),
                Span::styled(format!(" {detail}"), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
    lines.push(Line::from(Span::raw("")));

    // Render the scrollable content into the top portion of the
    // inner rect. The footer (buttons + error + hint) occupies the
    // last FOOTER_ROWS rows and is always visible.
    let content_rect = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: content_height,
    };
    f.render_widget(Paragraph::new(lines), content_rect);

    // Footer: button row at footer_y + 1 (leave one blank separator
    // at footer_y so the button doesn't collide with content visually).
    let button_y = footer_y + 1;

    let run_label = " [ Run ] ";
    let run_width = run_label.chars().count() as u16;

    let run_style = if matches!(form.focus, FormFocus::Button(FormButton::Run)) {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };

    let run_x = inner.x + 1;
    let run_rect = Rect {
        x: run_x,
        y: button_y,
        width: run_width.min(inner.width.saturating_sub(1)),
        height: 1,
    };

    f.render_widget(Paragraph::new(Span::styled(run_label, run_style)), run_rect);

    app.layout
        .workflow_form_buttons
        .push((FormButton::Run, run_rect));

    // Error line (row below the buttons).
    let error_y = button_y + 1;
    let error_rect = Rect {
        x: inner.x,
        y: error_y,
        width: inner.width,
        height: 1,
    };
    let error_line = match &form.error {
        Some(err) => Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default().fg(Color::Red),
        )),
        None => Line::from(Span::raw("")),
    };
    f.render_widget(Paragraph::new(error_line), error_rect);

    // Hint line (row below the error line).
    let hint_y = error_y + 1;
    if hint_y < inner.y + inner.height {
        let hint_rect = Rect {
            x: inner.x,
            y: hint_y,
            width: inner.width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                " tab next · ⏎ run · click field / checkbox / select · esc cancel ",
                Style::default().fg(Color::DarkGray),
            )),
            hint_rect,
        );
    }

    // Picker overlay, drawn last so it sits on top of the form.
    if app
        .workflow_form
        .as_ref()
        .and_then(|s| s.picker.as_ref())
        .is_some()
    {
        render_workflow_picker(f, inner, app);
    }

    inner
}

/// Single-choice picker overlay for a workflow Select field. Anchors
/// the picker directly below the focused Select row (flipping above if
/// it would overflow the form), so the user's eye doesn't have to jump
/// to the middle of the screen to find it. Records per-row hit-rects
/// and the outer rect so the mouse handler can commit / swallow / close.
fn render_workflow_picker(f: &mut Frame, area: Rect, app: &mut App) {
    use super::workflow_view::FieldEditor;

    let Some(form) = app.workflow_form.as_ref() else {
        return;
    };
    let Some(picker) = form.picker.as_ref() else {
        return;
    };
    let (options, name) = match form.fields.get(picker.field_idx) {
        Some(FieldEditor::Select { options, name, .. }) => (options.clone(), name.clone()),
        _ => return,
    };
    let picker_selected = picker.selected;

    // Anchor rect: the Select field's label+value rows captured this
    // frame. Fall back to centring if we somehow don't have one (e.g.
    // the field scrolled out of view).
    let anchor = app
        .layout
        .workflow_form_fields
        .iter()
        .find(|(idx, _)| *idx == picker.field_idx)
        .map(|(_, r)| *r);

    // Picker size: wide enough for the longest option + title/padding;
    // tall enough for every option plus the two border rows.
    let longest = options.iter().map(|s| s.len()).max().unwrap_or(0);
    let title_width = (name.len() as u16).saturating_add(8);
    let w = ((longest as u16 + 6).max(title_width)).clamp(16, area.width);
    let h = (options.len() as u16 + 2).clamp(4, area.height);

    // Position. Prefer directly under the field's value row, indented
    // to line up with the `◉ value ▾` span. If that would push the
    // picker past the bottom of the form, flip above the field.
    let rect = match anchor {
        Some(a) => {
            let value_indent: u16 = 4; // matches the render_workflow_form indent
            let desired_x = a.x.saturating_add(value_indent);
            let max_x = area.x + area.width.saturating_sub(w);
            let x = desired_x.min(max_x).max(area.x);

            let below_y = a.y.saturating_add(a.height);
            let area_bottom = area.y + area.height;
            let y = if below_y + h <= area_bottom {
                below_y
            } else if a.y >= area.y + h {
                a.y - h
            } else {
                // No room either way — clamp to the top so at least the
                // first options are visible.
                area.y
            };
            Rect {
                x,
                y,
                width: w,
                height: h,
            }
        }
        None => centered_rect(area, w, h),
    };
    f.render_widget(Clear, rect);

    let title = format!(" pick {name} ");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let items: Vec<ListItem> = options
        .iter()
        .map(|o| ListItem::new(Line::from(Span::raw(o.clone()))))
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

    let mut rows: Vec<Rect> = Vec::with_capacity(options.len());
    for i in 0..options.len() as u16 {
        if i >= inner.height {
            break;
        }
        rows.push(Rect::new(inner.x, inner.y + i, inner.width, 1));
    }
    app.layout.workflow_form_picker_rows = rows;
    app.layout.workflow_form_picker_outer = Some(rect);
}

/// One-line summary for a `StepKind`, used by the form's preview.
fn step_preview(kind: &crate::config::StepKind) -> (&'static str, String) {
    use crate::config::StepKind;
    match kind {
        StepKind::Command { command, .. } => ("command", truncate_one_line(command, 60)),
        StepKind::Worker { worker, prompt } => (
            "worker",
            format!("{worker}: {}", truncate_one_line(prompt, 50)),
        ),
        StepKind::Conditional {
            worker,
            prompt,
            target,
        } => (
            "conditional",
            format!(
                "{worker}: {} (on fail → {target})",
                truncate_one_line(prompt, 36)
            ),
        ),
        StepKind::Check {
            variable,
            op,
            value,
            target,
        } => (
            "check",
            format!(
                "{variable} {} {} (on fail → {target})",
                op.name(),
                value.as_deref().unwrap_or("")
            ),
        ),
    }
}

/// Collapse newlines and truncate with an ellipsis so a long command
/// or prompt stays on one sidebar line.
fn truncate_one_line(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let trimmed = flat.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let head: String = trimmed.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Render a text-input buffer with an optional cursor drawn as a thin
/// vertical bar. Returns a span list the caller can splice into a Line.
fn render_text_input(value: &str, cursor: usize, focused: bool) -> Vec<Span<'static>> {
    if !focused {
        return vec![Span::styled(
            value.to_string(),
            Style::default().fg(Color::White),
        )];
    }
    let before: String = value.chars().take(cursor).collect();
    let after: String = value.chars().skip(cursor).collect();
    vec![
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled("▏", Style::default().fg(Color::Cyan)),
        Span::styled(after, Style::default().fg(Color::White)),
    ]
}

/// Live progress view for one workflow instance. Lays out the
/// variables panel (every property's current value), the step list
/// (with status glyphs + retry counts), and the tail of live output.
fn render_workflow_instance(
    f: &mut Frame,
    area: Rect,
    app: &App,
    workflow_idx: usize,
    instance_id: u64,
) -> Rect {
    use crate::workflow::{InstanceStatus, StepStatus};

    let focused = app.focus == Focus::Workflow;
    let wf = app.config.workflows.get(workflow_idx);
    let inst = app.workflow_instances.iter().find(|i| i.id == instance_id);

    let (status_label, _status_style) = match inst.map(|i| i.status) {
        Some(InstanceStatus::Running) => (
            "running",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Some(InstanceStatus::Failed) => (
            "failed",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Some(InstanceStatus::Succeeded) | None => ("done", Style::default().fg(Color::Green)),
    };
    let title = match wf {
        Some(w) => format!(" {} · #{} · {} ", w.id, instance_id, status_label),
        None => format!(" #{instance_id} · {status_label} "),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(inst) = inst else {
        f.render_widget(
            Paragraph::new("instance not found (may have completed successfully)")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return inner;
    };

    let mut lines: Vec<Line> = Vec::new();

    // Variables panel — include hidden so workflow_set mutations are
    // visible to the operator.
    lines.push(Line::from(Span::styled(
        "Variables",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    if inst.vars.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (no variables seeded yet)",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        // Stable order: iterate the declared property list when we can.
        if let Some(w) = wf {
            for p in &w.properties {
                if let Some(v) = inst.vars.get(&p.id) {
                    let hidden_tag = if matches!(p.kind, crate::config::PropertyKind::Hidden { .. })
                    {
                        Span::styled(" [hidden]", Style::default().fg(Color::DarkGray))
                    } else {
                        Span::raw("")
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("  {} ", p.id), Style::default().fg(Color::White)),
                        Span::styled("= ", Style::default().fg(Color::DarkGray)),
                        Span::styled(v.as_display(), Style::default().fg(Color::Yellow)),
                        hidden_tag,
                    ]));
                }
            }
        } else {
            for (k, v) in &inst.vars {
                lines.push(Line::from(Span::raw(format!("  {k} = {}", v.as_display()))));
            }
        }
    }
    lines.push(Line::from(Span::raw("")));

    // Steps panel.
    lines.push(Line::from(Span::styled(
        "Steps",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )));
    for (idx, s) in inst.steps.iter().enumerate() {
        let glyph = match s.status {
            StepStatus::Pending => "•",
            StepStatus::Running => "▶",
            StepStatus::Succeeded => "✓",
            StepStatus::Failed => "✗",
            StepStatus::Retrying => "↺",
        };
        let color = match s.status {
            StepStatus::Pending => Color::DarkGray,
            StepStatus::Running => Color::Yellow,
            StepStatus::Succeeded => Color::Green,
            StepStatus::Failed => Color::Red,
            StepStatus::Retrying => Color::Magenta,
        };
        let current_marker =
            if idx == inst.current_step && matches!(inst.status, InstanceStatus::Running) {
                " ← current"
            } else {
                ""
            };
        let retry = if s.retry_count > 0 {
            format!(" · retry #{}", s.retry_count)
        } else {
            String::new()
        };
        let reason =
            if let (Some(r), StepStatus::Failed | StepStatus::Retrying) = (&s.reason, s.status) {
                format!(" ({})", r)
            } else {
                String::new()
            };
        lines.push(Line::from(vec![
            Span::raw(" "),
            Span::styled(format!("[{glyph}] "), Style::default().fg(color)),
            Span::styled(
                format!("{} ({})", s.id, s.kind_label),
                Style::default().fg(color),
            ),
            Span::styled(retry, Style::default().fg(Color::Magenta)),
            Span::styled(reason, Style::default().fg(Color::DarkGray)),
            Span::styled(current_marker.to_string(), Style::default().fg(Color::Cyan)),
        ]));
    }
    lines.push(Line::from(Span::raw("")));

    // Output tail.
    if !inst.tail_output.is_empty() {
        lines.push(Line::from(Span::styled(
            "Output (tail)",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        // Show the last ~20 lines so the panel fits without scrolling.
        let take = inst.tail_output.len().saturating_sub(20);
        for line in inst.tail_output.iter().skip(take) {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::White),
            )));
        }
    }

    // Failure reason on the final line (if failed).
    if matches!(inst.status, InstanceStatus::Failed) {
        lines.push(Line::from(Span::raw("")));
        lines.push(Line::from(Span::styled(
            format!(
                "⚠ failed: {}",
                inst.failure_reason.as_deref().unwrap_or("(no reason)")
            ),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(Span::styled(
            " press `d` to dismiss · esc back ",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " esc back ",
            Style::default().fg(Color::DarkGray),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
    inner
}

fn render_cron_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    use super::cron_view::CronHotspot;
    let focused = app.focus == Focus::Cron;
    let jobs = super::cron_view::listed_jobs(&app.cron);
    app.cron_screen.reclamp(jobs.len());

    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);
    let toolbar_area = split[0];
    let list_area = split[1];
    let status_area = split[2];

    // Toolbar with a clickable "+ new" chip on the right and a
    // keybinds hint on the left.
    let new_label = "[ + new ]";
    let new_width = new_label.chars().count() as u16;
    let (hint_area, new_area) = if toolbar_area.width > new_width + 1 {
        let new_rect = Rect {
            x: toolbar_area.x + toolbar_area.width - new_width,
            y: toolbar_area.y,
            width: new_width,
            height: 1,
        };
        let hint_rect = Rect {
            x: toolbar_area.x,
            y: toolbar_area.y,
            width: toolbar_area.width - new_width - 1,
            height: 1,
        };
        (hint_rect, Some(new_rect))
    } else {
        (toolbar_area, None)
    };
    let hint = Paragraph::new(Line::from(vec![
        Span::styled(
            " cron  ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "a add · e edit · d delete · x toggle · esc back",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
    f.render_widget(hint, hint_area);
    if let Some(rect) = new_area {
        f.render_widget(
            Paragraph::new(Span::styled(
                new_label,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            rect,
        );
        app.layout
            .cron_hotspots
            .push((rect, CronHotspot::NewButton));
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" cron jobs ({}) ", jobs.len()))
        .border_style(focus_border_style(focused));
    let list_inner = list_block.inner(list_area);
    f.render_widget(list_block, list_area);

    if jobs.is_empty() {
        let p = Paragraph::new("No cron jobs yet. Press `a` or click `+ new` to schedule one.")
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true });
        f.render_widget(p, list_inner);
    } else {
        // Each row is split into: dot (2 cols) | body (fills) | edit (6) | del (5).
        let dot_w: u16 = 2;
        let edit_label = " [edit] ";
        let del_label = " [del] ";
        let edit_w: u16 = edit_label.chars().count() as u16;
        let del_w: u16 = del_label.chars().count() as u16;

        for (i, j) in jobs.iter().enumerate() {
            let row_y = list_inner.y + i as u16;
            if row_y >= list_inner.y + list_inner.height {
                break;
            }
            let row_rect = Rect {
                x: list_inner.x,
                y: row_y,
                width: list_inner.width,
                height: 1,
            };
            let selected = i == app.cron_screen.cursor;
            let base_style = if selected && focused {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            // Dot column
            let dot_rect = Rect {
                x: row_rect.x,
                y: row_rect.y,
                width: dot_w.min(row_rect.width),
                height: 1,
            };
            let dot = if j.enabled { "●" } else { "○" };
            let dot_style = if j.enabled {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            // Compose the body so hover-highlight still fills its width
            // when the row is selected — render a solid styled
            // paragraph across the whole row first, then overlay the
            // dot and the two trailing buttons.
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " ".repeat(row_rect.width as usize),
                    base_style,
                ))),
                row_rect,
            );
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(dot, dot_style))),
                dot_rect,
            );
            app.layout
                .cron_hotspots
                .push((dot_rect, CronHotspot::ToggleEnabled(i)));

            // Reserve the trailing two buttons first so the body never
            // overwrites them when the job's text is long.
            let right_avail = edit_w + del_w + 1;
            let (body_rect, edit_rect, del_rect) = if row_rect.width > dot_w + right_avail {
                let body = Rect {
                    x: row_rect.x + dot_w,
                    y: row_rect.y,
                    width: row_rect.width - dot_w - right_avail,
                    height: 1,
                };
                let edit = Rect {
                    x: row_rect.x + row_rect.width - edit_w - del_w - 1,
                    y: row_rect.y,
                    width: edit_w,
                    height: 1,
                };
                let del = Rect {
                    x: row_rect.x + row_rect.width - del_w,
                    y: row_rect.y,
                    width: del_w,
                    height: 1,
                };
                (body, Some(edit), Some(del))
            } else {
                // Too narrow — drop the inline buttons gracefully.
                let body = Rect {
                    x: row_rect.x + dot_w,
                    y: row_rect.y,
                    width: row_rect.width.saturating_sub(dot_w),
                    height: 1,
                };
                (body, None, None)
            };

            // Trim the formatted text to fit; cron rows can be long.
            let text = super::cron_view::fmt_cron_row(j);
            let body_str: String = text.chars().take(body_rect.width as usize).collect();
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(body_str, base_style))),
                body_rect,
            );
            app.layout
                .cron_hotspots
                .push((body_rect, CronHotspot::SelectRow(i)));

            if let Some(rect) = edit_rect {
                f.render_widget(
                    Paragraph::new(Span::styled(
                        edit_label,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                    )),
                    rect,
                );
                app.layout
                    .cron_hotspots
                    .push((rect, CronHotspot::EditRow(i)));
            }
            if let Some(rect) = del_rect {
                f.render_widget(
                    Paragraph::new(Span::styled(
                        del_label,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    )),
                    rect,
                );
                app.layout
                    .cron_hotspots
                    .push((rect, CronHotspot::DeleteRow(i)));
            }
        }
    }

    // Status / toast line beneath the list.
    let status_line = app.cron_screen.status_msg.clone().unwrap_or_default();
    f.render_widget(
        Paragraph::new(Span::styled(
            status_line,
            Style::default().fg(Color::Yellow),
        )),
        status_area,
    );

    list_inner
}

fn render_cron_modals(f: &mut Frame, area: Rect, app: &mut App) {
    use super::cron_view::{CronFormField, CronHotspot, CronMode};

    match app.cron_screen.mode.clone() {
        CronMode::List => {}
        CronMode::ConfirmDelete { display, .. } => {
            let width = 60.min(area.width.saturating_sub(4));
            let height = 6.min(area.height.saturating_sub(2));
            let rect = centered_rect(area, width.max(24), height.max(4));
            f.render_widget(Clear, rect);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" delete cron job? ")
                .border_style(Style::default().fg(Color::Red));
            let inner = block.inner(rect);
            f.render_widget(block, rect);
            // Layout: line 0 caption · line 1 blank · line 2 buttons
            //         line 3 hint.
            let caption_rect = Rect {
                x: inner.x,
                y: inner.y,
                width: inner.width,
                height: 1,
            };
            f.render_widget(Paragraph::new(Span::raw(display)), caption_rect);

            let yes_label = " [ yes · y ] ";
            let no_label = " [ no · n ] ";
            let yes_w = yes_label.chars().count() as u16;
            let no_w = no_label.chars().count() as u16;
            if inner.height >= 3 && inner.width > yes_w + no_w {
                let buttons_y = inner.y + 2;
                let yes_rect = Rect {
                    x: inner.x,
                    y: buttons_y,
                    width: yes_w,
                    height: 1,
                };
                let no_rect = Rect {
                    x: inner.x + yes_w + 1,
                    y: buttons_y,
                    width: no_w,
                    height: 1,
                };
                f.render_widget(
                    Paragraph::new(Span::styled(
                        yes_label,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    )),
                    yes_rect,
                );
                f.render_widget(
                    Paragraph::new(Span::styled(
                        no_label,
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Gray)
                            .add_modifier(Modifier::BOLD),
                    )),
                    no_rect,
                );
                app.layout
                    .cron_hotspots
                    .push((yes_rect, CronHotspot::ConfirmDeleteYes));
                app.layout
                    .cron_hotspots
                    .push((no_rect, CronHotspot::ConfirmDeleteNo));
            }
            if inner.height >= 4 {
                let hint_rect = Rect {
                    x: inner.x,
                    y: inner.y + 3,
                    width: inner.width,
                    height: 1,
                };
                f.render_widget(
                    Paragraph::new(Span::styled(
                        "y confirm · n / esc cancel",
                        Style::default().fg(Color::DarkGray),
                    )),
                    hint_rect,
                );
            }
        }
        CronMode::Form(form) => {
            let fields = CronFormField::all();
            // One line per field + 2 for title separator + 2 for error
            // banner + 2 for hint.
            let width = 72.min(area.width.saturating_sub(4));
            let height = (fields.len() as u16 + 6).min(area.height.saturating_sub(2));
            let rect = centered_rect(area, width.max(40), height.max(10));
            f.render_widget(Clear, rect);
            let title = match form.kind {
                super::cron_view::CronFormKind::Add => " new cron job ",
                super::cron_view::CronFormKind::Edit(_) => " edit cron job ",
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Yellow));
            let inner = block.inner(rect);
            f.render_widget(block, rect);

            // Render each field on its own row so we can register a
            // matching rect per-row as a clickable hotspot.
            for (i, field) in fields.iter().enumerate() {
                let row_y = inner.y + i as u16;
                if row_y >= inner.y + inner.height {
                    break;
                }
                let row_rect = Rect {
                    x: inner.x,
                    y: row_y,
                    width: inner.width,
                    height: 1,
                };
                let label = format!("{:<20}", field.label());
                let value = match field {
                    CronFormField::Target => form.target.clone(),
                    CronFormField::PayloadKind => form.payload_kind.label().to_string(),
                    CronFormField::PayloadBody => {
                        form.payload_body.lines().next().unwrap_or("").to_string()
                    }
                    CronFormField::ScheduleKind => form.schedule_kind.label().to_string(),
                    CronFormField::ScheduleText => form.schedule_text.clone(),
                    CronFormField::Timezone => form.timezone.as_str().to_string(),
                    CronFormField::CatchupOnStartup => {
                        if form.catchup_on_startup { "yes" } else { "no" }.to_string()
                    }
                    CronFormField::Enabled => if form.enabled { "yes" } else { "no" }.to_string(),
                    CronFormField::SaveButton | CronFormField::CancelButton => String::new(),
                };
                let is_focused = *field == form.focused;
                let marker = if is_focused { "▶ " } else { "  " };
                let is_button = matches!(
                    field,
                    CronFormField::SaveButton | CronFormField::CancelButton
                );
                let style = if is_button {
                    let (fg, bg) = match field {
                        CronFormField::SaveButton => (Color::Black, Color::Green),
                        _ => (Color::Black, Color::Gray),
                    };
                    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
                } else if is_focused {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let line_text = if is_button {
                    format!("{marker}{}", field.label())
                } else {
                    format!("{marker}{label} {value}")
                };
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(line_text, style))),
                    row_rect,
                );
                let hotspot = match field {
                    CronFormField::SaveButton => CronHotspot::FormSave,
                    CronFormField::CancelButton => CronHotspot::FormCancel,
                    other => CronHotspot::FormField(*other),
                };
                app.layout.cron_hotspots.push((row_rect, hotspot));
            }

            // Hint + error lines fill whatever vertical space is left.
            let hint = match form.focused {
                CronFormField::Target => "configured agent id (e.g. `reviewer`)",
                CronFormField::PayloadKind => "click / space toggles message / command",
                CronFormField::PayloadBody => "free-form; slash commands must start with `/`",
                CronFormField::ScheduleKind => "click / space toggles recurring / one-off",
                CronFormField::ScheduleText => form.schedule_kind.hint(),
                CronFormField::Timezone => "click / space toggles local / utc",
                CronFormField::CatchupOnStartup => "one-off only; click / space toggles",
                CronFormField::Enabled => "click / space toggles; disabled jobs never fire",
                CronFormField::SaveButton | CronFormField::CancelButton => {
                    "click or enter to activate · tab cycles fields · esc cancels"
                }
            };
            let hint_y = inner.y + fields.len() as u16 + 1;
            if hint_y < inner.y + inner.height {
                let rr = Rect {
                    x: inner.x,
                    y: hint_y,
                    width: inner.width,
                    height: 1,
                };
                f.render_widget(
                    Paragraph::new(Span::styled(
                        format!(" hint: {hint}"),
                        Style::default().fg(Color::DarkGray),
                    )),
                    rr,
                );
            }
            if let Some(err) = &form.error {
                let err_y = inner.y + fields.len() as u16 + 2;
                if err_y < inner.y + inner.height {
                    let rr = Rect {
                        x: inner.x,
                        y: err_y,
                        width: inner.width,
                        height: 1,
                    };
                    f.render_widget(
                        Paragraph::new(Span::styled(
                            format!(" error: {err}"),
                            Style::default().fg(Color::Red),
                        )),
                        rr,
                    );
                }
            }
        }
    }
}
