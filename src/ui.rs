use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

use crate::app::{visible_load_items, App, Confirm, LoadUi, Mode, Tenant};

use crate::proc::Stack;
use crate::theme::Theme;
use crate::util::{human_mib, is_unreachable, short_error, truncate};

pub fn draw(frame: &mut Frame, app: &mut App) {
    app.navigation.begin(frame.area());
    let theme = &app.theme;
    frame.render_widget(Block::default().style(theme.base()), frame.area());

    let area = frame.area();
    if area.width < 30 || area.height < 14 {
        frame.render_widget(
            Paragraph::new(
                "glim · needs 30×14
Resize terminal; q quits",
            )
            .style(theme.base()),
            area,
        );
        return;
    }
    let chrome = chrome(app);
    let area = chrome.shell(frame, area);
    let title_h = crate::aurora::title_height(area.height);
    let footer_h = if area.height >= 26 { 4 } else { 3 };
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(title_h + 3),
        Constraint::Min(1),
        Constraint::Length(footer_h),
    ])
    .areas(area);
    let [identity, nav] =
        Layout::vertical([Constraint::Length(title_h), Constraint::Length(3)]).areas(header);
    draw_logo(frame, identity, app);
    app.navigation_bar(frame, nav);
    let body = crate::aurora::body(body);
    if body.height < 14 {
        if app.inspect_focus {
            app.navigation.details(body, 1);
            draw_mid(frame, body, app);
        } else {
            app.navigation.pane(body, 0);
            draw_table(frame, body, app);
            app.navigation.rows(
                app.table_area,
                0,
                app.table_row_offset,
                app.table_row_height,
                app.tenants.len().saturating_sub(app.table_row_offset),
            );
        }
    } else {
        let summary_h = if app.inspect_focus {
            if body.height >= 24 {
                13
            } else {
                body.height / 2
            }
        } else if body.height >= 24 {
            13
        } else if body.height >= 20 {
            11
        } else if body.height >= 14 {
            8
        } else {
            body.height.saturating_sub(4).min(6)
        };
        let desired_table_h = if body.height >= 20 {
            (app.tenants.len() as u16 * 2 + 7).max(9)
        } else {
            (app.tenants.len() as u16 + 5).max(5)
        };
        let [summary, models, _space] = Layout::vertical([
            Constraint::Length(summary_h),
            Constraint::Length(
                desired_table_h
                    .min(body.height.saturating_sub(summary_h))
                    .max(1),
            ),
            Constraint::Min(0),
        ])
        .areas(body);
        app.navigation.pane(summary, 1);
        draw_mid(frame, summary, app);
        app.navigation.pane(models, 0);
        draw_table(frame, models, app);
        app.navigation.rows(
            app.table_area,
            0,
            app.table_row_offset,
            app.table_row_height,
            app.tenants.len().saturating_sub(app.table_row_offset),
        );
    }
    let footer = chrome.footer(frame, footer);
    draw_footer(frame, footer, app);

    match &app.mode {
        Mode::Help => draw_help(frame, app),
        Mode::Load(ui) => draw_load(frame, app, ui),
        Mode::Confirm(c) => draw_confirm(frame, app, c),
        Mode::Normal => {}
    }
    app.commands.draw(
        frame,
        &app.command_actions(),
        app.theme.bg,
        app.theme.fg,
        app.theme.accent,
        app.theme.bg_raised,
    );
    app.draw_navigation(frame);
}

fn chrome(app: &App) -> crate::aurora::Chrome {
    let t = &app.theme;
    crate::aurora::Chrome {
        base: t.base(),
        muted: Style::default().fg(t.muted),
        accent: Style::default().fg(t.accent),
        data: Style::default().fg(t.cyan),
        border: Style::default().fg(t.border),
        deep: t.status(),
    }
}

fn draw_logo(frame: &mut Frame, area: Rect, app: &App) {
    let state = if app.last_poll.is_some() {
        "● online"
    } else {
        "○ waiting"
    };
    chrome(app).title(
        frame,
        area,
        "glim",
        "GPU memory / local inference",
        state,
        Style::default().fg(if app.last_poll.is_some() {
            app.theme.green
        } else {
            app.theme.muted
        }),
    );
}

fn draw_mid(frame: &mut Frame, area: Rect, app: &App) {
    if app.inspect_focus {
        if area.width >= 90 {
            let [gpu, model] =
                Layout::horizontal([Constraint::Length(40), Constraint::Min(1)]).areas(area);
            draw_gpu_card(frame, gpu, app);
            draw_inspector(frame, model, app);
        } else {
            draw_inspector(frame, area, app);
        }
        return;
    }
    let roomy = area.height >= 10;
    let [gpu, _gap, occupancy, _gap2, stacks, _space] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(u16::from(roomy)),
        Constraint::Length(if area.height >= 6 { 3 } else { 2 }),
        Constraint::Length(u16::from(roomy)),
        Constraint::Length(if roomy {
            6
        } else {
            area.height.saturating_sub(4)
        }),
        Constraint::Min(0),
    ])
    .areas(area);
    let device = if app.snapshot.gpu.name.is_empty() {
        "Waiting for NVIDIA GPU".to_owned()
    } else {
        app.snapshot.gpu.name.clone()
    };
    let details = format!(
        "{} · {} °C · {}% compute",
        human_mib(app.occupancy.total),
        app.snapshot.gpu.temp_c,
        app.snapshot.gpu.util_pct
    );
    let [label, stats] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(36)]).areas(gpu);
    frame.render_widget(
        Paragraph::new(device).style(
            Style::default()
                .fg(app.theme.fg)
                .add_modifier(Modifier::BOLD),
        ),
        label,
    );
    frame.render_widget(
        Paragraph::new(details)
            .style(Style::default().fg(app.theme.muted))
            .alignment(Alignment::Right),
        stats,
    );
    draw_vram(frame, occupancy, app, false);
    draw_stack_cards(frame, stacks, app);
}

fn draw_gpu_card(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let gpu = &app.snapshot.gpu;
    let block = Block::default()
        .title(Span::styled(" GPU ", Style::default().fg(theme.cyan)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    if let Some(err) = &gpu.error {
        frame.render_widget(
            Paragraph::new(short_error(err)).style(Style::default().fg(theme.red)),
            inner,
        );
        return;
    }

    let name = if gpu.name.is_empty() {
        "waiting for nvidia-smi".into()
    } else {
        truncate(&short_gpu(&gpu.name), inner.width as usize)
    };
    let mut lines: Vec<Line> = vec![
        Line::from(Span::styled(
            name,
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        gpu_stat(
            inner.width,
            "Compute",
            &format!("{}% busy", gpu.util_pct),
            theme,
            Style::default().fg(theme.cyan),
        ),
        gpu_stat(
            inner.width,
            "Memory I/O",
            &format!("{}% busy", gpu.mem_util_pct),
            theme,
            Style::default().fg(theme.magenta),
        ),
        gpu_stat(
            inner.width,
            "Temperature",
            &format!("{} °C", gpu.temp_c),
            theme,
            temp_style(theme, gpu.temp_c),
        ),
        gpu_stat(
            inner.width,
            "Power draw",
            &power_label(gpu.power_w, gpu.power_limit_w),
            theme,
            Style::default().fg(theme.yellow),
        ),
        gpu_stat(
            inner.width,
            "Power state",
            &power_state(&gpu.pstate),
            theme,
            Style::default().fg(theme.blue),
        ),
        gpu_stat(
            inner.width,
            "Core clock",
            &mhz(gpu.clock_sm),
            theme,
            Style::default().fg(theme.fg),
        ),
        gpu_stat(
            inner.width,
            "Memory clock",
            &mhz(gpu.clock_mem),
            theme,
            Style::default().fg(theme.fg),
        ),
    ];
    if let Some(fan) = gpu.fan_pct {
        lines.push(gpu_stat(
            inner.width,
            "Fan",
            &format!("{fan}%"),
            theme,
            Style::default().fg(theme.fg_dim),
        ));
    }
    if !gpu.driver.is_empty() {
        lines.push(gpu_stat(
            inner.width,
            "Driver",
            &gpu.driver,
            theme,
            Style::default().fg(theme.muted),
        ));
    }

    let gauge_n = if inner.height >= lines.len() as u16 + 2 {
        2
    } else {
        0
    };
    let stats_h = inner.height.saturating_sub(gauge_n);
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(stats_h), Constraint::Min(0)])
        .split(inner);
    frame.render_widget(Paragraph::new(lines), split[0]);
    if gauge_n == 2 && split[1].height >= 2 {
        let gauges = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(split[1]);
        labeled_gauge(frame, gauges[0], theme, "Compute", gpu.util_pct, theme.cyan);
        labeled_gauge(
            frame,
            gauges[1],
            theme,
            "Mem I/O",
            gpu.mem_util_pct,
            theme.magenta,
        );
    }
}

fn labeled_gauge(frame: &mut Frame, area: Rect, theme: &Theme, label: &str, pct: u8, color: Color) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(14), Constraint::Min(6)])
        .split(area);
    frame.render_widget(
        Paragraph::new(Span::styled(
            format!("{label:<12}"),
            Style::default().fg(theme.muted),
        )),
        cols[0],
    );
    frame.render_widget(
        Gauge::default()
            .percent(pct.min(100) as u16)
            .gauge_style(Style::default().fg(color).bg(theme.bg_raised))
            .label(format!("{pct}%")),
        cols[1],
    );
}

fn gpu_stat<'a>(
    width: u16,
    label: &str,
    value: &str,
    theme: &Theme,
    value_style: Style,
) -> Line<'a> {
    const LABEL_W: usize = 14;
    let value = value.to_string();
    let gap = (width as usize)
        .saturating_sub(LABEL_W + value.len())
        .max(1);
    Line::from(vec![
        Span::styled(
            format!("{label:<LABEL_W$}"),
            Style::default().fg(theme.muted),
        ),
        Span::raw(" ".repeat(gap)),
        Span::styled(value, value_style),
    ])
}

fn power_label(draw: f32, limit: Option<f32>) -> String {
    match limit {
        Some(limit) => format!("{} / {}", watts(draw), watts(limit)),
        None => watts(draw),
    }
}

fn power_state(pstate: &str) -> String {
    match pstate {
        "" => "—".into(),
        "P0" | "P1" => format!("{pstate}  full"),
        "P2" | "P3" | "P4" | "P5" => format!("{pstate}  active"),
        "P8" | "P10" | "P12" => format!("{pstate}  idle"),
        other => other.to_string(),
    }
}

fn mhz(clock: Option<u32>) -> String {
    match clock {
        Some(n) => format!("{n} MHz"),
        None => "—".into(),
    }
}

fn draw_vram(frame: &mut Frame, area: Rect, app: &App, include_cards: bool) {
    let theme = &app.theme;
    let occ = &app.occupancy;
    let [heading, bar, legend] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);
    let used_pct = if occ.total == 0 {
        0
    } else {
        occ.used.saturating_mul(100) / occ.total
    };
    let title_style = if app.over_budget(0) {
        Style::default().fg(theme.red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.fg)
    };
    let [label, value] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(26)]).areas(heading);
    frame.render_widget(
        Paragraph::new("VRAM occupancy").style(Style::default().fg(theme.muted)),
        label,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{} / {}  {}%",
            display_mib(occ.used),
            human_mib(occ.total),
            used_pct
        ))
        .style(title_style)
        .alignment(Alignment::Right),
        value,
    );
    frame.render_widget(Paragraph::new(stacked_bar(app, bar.width as usize)), bar);
    let mut spans = vec![
        Span::styled("■ ", Style::default().fg(theme.cyan)),
        Span::styled(
            format!("Ollama {}  ", human_mib(occ.ollama)),
            Style::default().fg(theme.fg),
        ),
    ];
    if occ.lmstudio > 0 {
        spans.push(Span::styled("■ ", Style::default().fg(theme.magenta)));
        spans.push(Span::styled(
            format!("LM Studio {}  ", human_mib(occ.lmstudio)),
            Style::default().fg(theme.fg),
        ));
    }
    spans.push(Span::styled("■ ", Style::default().fg(theme.accent)));
    spans.push(Span::styled(
        format!("Other {}", human_mib(occ.other)),
        Style::default().fg(theme.fg),
    ));
    let [key, free] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(15)]).areas(legend);
    frame.render_widget(Paragraph::new(Line::from(spans)), key);
    frame.render_widget(
        Paragraph::new(format!("Free {}", human_mib(occ.free)))
            .style(Style::default().fg(theme.muted))
            .alignment(Alignment::Right),
        free,
    );
    let _ = include_cards;
}

fn display_mib(mib: u64) -> String {
    if mib >= 1024 {
        format!("{:.1} GiB", mib as f64 / 1024.0)
    } else {
        human_mib(mib)
    }
}

fn stacked_bar(app: &App, width: usize) -> Line<'static> {
    let occ = &app.occupancy;
    let theme = &app.theme;
    let total = occ.total.max(1);
    let bar_w = width.max(8);
    let mut counts = [
        (occ.ollama, theme.cyan, "ollama"),
        (occ.lmstudio, theme.magenta, "lms"),
        (occ.other, theme.accent, "other"),
        (occ.free, theme.bg_raised, "free"),
    ];
    let sum: u64 = counts.iter().map(|c| c.0).sum();
    if sum < total {
        counts[3].0 += total - sum;
    }
    let mut cells = Vec::new();
    let mut used_cells = 0usize;
    for (i, (mib, color, _)) in counts.iter().enumerate() {
        let n = if i == counts.len() - 1 {
            bar_w.saturating_sub(used_cells)
        } else {
            let n = ((*mib as usize) * bar_w) / total as usize;
            used_cells += n;
            n
        };
        if n > 0 {
            cells.push(Span::styled(" ".repeat(n), Style::default().bg(*color)));
        }
    }
    Line::from(cells)
}

fn draw_stack_cards(frame: &mut Frame, area: Rect, app: &App) {
    let [ollama, _gutter, studio] = Layout::horizontal([
        Constraint::Ratio(1, 2),
        Constraint::Length(3),
        Constraint::Ratio(1, 2),
    ])
    .areas(area);
    let t = &app.theme;
    for (slot, name, stack, online, app_running, memory, key) in [
        (
            ollama,
            "Ollama",
            Stack::Ollama,
            app.snapshot.ollama_online,
            app.snapshot.ollama_online,
            app.occupancy.ollama,
            "o",
        ),
        (
            studio,
            "LM Studio",
            Stack::LmStudio,
            app.snapshot.lms_online,
            app.snapshot.lms_app,
            app.occupancy.lmstudio,
            "m",
        ),
    ] {
        let block = Block::default()
            .borders(if area.height >= 3 {
                Borders::TOP
            } else {
                Borders::NONE
            })
            .border_style(Style::default().fg(t.border))
            .padding(ratatui::widgets::Padding::new(
                0,
                0,
                u16::from(area.height >= 5),
                0,
            ));
        let inner = block.inner(slot);
        frame.render_widget(block, slot);
        let n = app
            .tenants
            .iter()
            .filter(|tenant| tenant.stack == stack)
            .count();
        let compact = area.width < 76;
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    format!("{name}  "),
                    Style::default().fg(t.fg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if app_running {
                        "● running"
                    } else {
                        "○ stopped"
                    },
                    Style::default().fg(if app_running { t.green } else { t.muted }),
                ),
                Span::styled(
                    if stack == Stack::LmStudio && app_running != online {
                        " · API offline"
                    } else {
                        ""
                    },
                    Style::default().fg(t.muted),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    if compact {
                        format!("{n} models")
                    } else if n == 0 {
                        "No models loaded".into()
                    } else {
                        format!("{n} models loaded")
                    },
                    Style::default().fg(t.muted),
                ),
                Span::styled(format!("   ({key}) "), Style::default().fg(t.accent)),
                Span::styled(
                    if online { "stop" } else { "start" },
                    Style::default().fg(t.muted),
                ),
            ]),
        ];
        let share = memory.saturating_mul(100) / app.occupancy.total.max(1);
        let host = if stack == Stack::Ollama {
            app.config
                .host()
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .to_string()
        } else {
            let daemon = app.snapshot.lms_daemon.as_ref();
            let port = if app.snapshot.lms_server_port > 0 {
                app.snapshot.lms_server_port.to_string()
            } else {
                daemon
                    .map(|d| d.port.to_string())
                    .unwrap_or_else(|| "—".into())
            };
            format!("{}:{port}", daemon.map(|d| d.host.as_str()).unwrap_or("—"))
        };
        let endpoint = if stack == Stack::LmStudio {
            format!(
                "{} · API {} · {host}",
                human_mib(memory),
                if online { "online" } else { "offline" }
            )
        } else {
            format!("{} · {share}% · {host}", human_mib(memory))
        };
        if !compact {
            lines.push(Line::from(Span::styled(
                endpoint,
                Style::default().fg(t.muted),
            )));
        }
        if stack == Stack::LmStudio && !compact {
            if let Some(daemon) = &app.snapshot.lms_daemon {
                lines.push(Line::from(Span::styled(
                    format!(
                        "API {} · PID {}",
                        if online { "online" } else { "offline" },
                        daemon.pid
                    ),
                    Style::default().fg(t.muted),
                )));
            }
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}

fn draw_inspector(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let block = Block::default()
        .title(Span::styled(" INSPECT ", Style::default().fg(theme.blue)))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if app.inspect_focus {
            theme.accent
        } else {
            theme.border
        }))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(tenant) = app.selected_tenant() else {
        let idle = vec![
            Line::from(Span::styled(
                "GPU idle",
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "o  start Ollama",
                Style::default().fg(theme.fg_dim),
            )),
            Line::from(Span::styled(
                "m  start LM Studio",
                Style::default().fg(theme.fg_dim),
            )),
            Line::from(Span::styled(
                "l  load a model",
                Style::default().fg(theme.fg_dim),
            )),
        ];
        frame.render_widget(Paragraph::new(idle), inner);
        return;
    };

    let gpu = &app.snapshot.gpu;
    let mut lines = vec![
        kv(theme, "GPU", &gpu.name),
        kv(
            theme,
            "Compute",
            &format!("{}% · {} °C", gpu.util_pct, gpu.temp_c),
        ),
        kv(theme, "Power", &power_label(gpu.power_w, gpu.power_limit_w)),
        kv(theme, "Driver", &gpu.driver),
        Line::from(""),
        kv(theme, "model", &tenant.name),
        kv(theme, "stack", tenant.stack.label()),
        kv(
            theme,
            "vram",
            &format!("{}  {}%", human_mib(tenant.vram_mib), tenant.share_pct),
        ),
    ];
    if !tenant.quant.is_empty() {
        lines.push(kv(theme, "quant", &tenant.quant));
    }
    if tenant.size_mib > 0 {
        lines.push(kv(theme, "size", &human_mib(tenant.size_mib)));
    }
    if tenant.ram_mib > 0 {
        lines.push(kv(theme, "ram", &human_mib(tenant.ram_mib)));
    }
    lines.push(kv(
        theme,
        "ctx",
        &tenant
            .context
            .map(|c| c.to_string())
            .unwrap_or_else(|| "—".into()),
    ));
    lines.push(kv(theme, "ttl", &tenant.status));
    lines.push(kv(
        theme,
        "pid",
        &tenant
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".into()),
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        hint_for(tenant),
        Style::default().fg(theme.yellow),
    )));
    let scroll = app
        .detail_scroll
        .min(lines.len().saturating_sub(inner.height as usize) as u16);
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn kv(theme: &Theme, key: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key:<9}",), Style::default().fg(theme.muted)),
        Span::styled(value.to_string(), Style::default().fg(theme.fg)),
    ])
}

fn hint_for(tenant: &Tenant) -> &'static str {
    match tenant.kind {
        crate::app::TenantKind::OllamaModel | crate::app::TenantKind::LmsModel => {
            "u unload   U unload stack   p park other"
        }
        crate::app::TenantKind::Orphan => "x kill this orphan (confirm)",
        crate::app::TenantKind::Process => "other GPU process — not a loaded model",
    }
}

fn draw_table(frame: &mut Frame, area: Rect, app: &mut App) {
    let theme = &app.theme;
    let roomy = area.height >= 9;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(if app.inspect_focus {
            theme.border
        } else {
            theme.accent
        }))
        .padding(ratatui::widgets::Padding::horizontal(1))
        .style(theme.base());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [heading, inner] = Layout::vertical([
        Constraint::Length(if roomy { 2 } else { 1 }),
        Constraint::Min(0),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                if app.inspect_focus {
                    "GPU process details "
                } else {
                    "Loaded models "
                },
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                app.tenants.len().to_string(),
                Style::default().fg(theme.muted),
            ),
        ])),
        heading,
    );

    if app.tenants.is_empty() {
        let empty = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  nothing on the GPU",
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  start a stack, then load a model",
                Style::default().fg(theme.muted),
            )),
            Line::from(""),
            Line::from(vec![
                Span::raw("  "),
                keycap("o", theme),
                Span::styled(" ollama   ", Style::default().fg(theme.fg_dim)),
                keycap("m", theme),
                Span::styled(" lm studio   ", Style::default().fg(theme.fg_dim)),
                keycap("l", theme),
                Span::styled(" load", Style::default().fg(theme.fg_dim)),
            ]),
        ];
        frame.render_widget(Paragraph::new(empty), inner);
        return;
    }

    let [table_area, note_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(if roomy { 2 } else { 0 }),
    ])
    .areas(inner);
    app.table_area = Rect {
        x: table_area.x,
        y: table_area.y.saturating_add(1),
        width: table_area.width,
        height: table_area.height.saturating_sub(1),
    };

    let wide = app.inspect_focus && inner.width >= 92;
    let medium = app.inspect_focus && inner.width >= 72;
    let header = if wide {
        Row::new([
            "STACK", "MODEL", "QUANT", "VRAM", "SHARE", "CTX", "TTL", "PID",
        ])
        .style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )
    } else if medium {
        Row::new(["STACK", "MODEL", "VRAM", "SHARE", "TTL", "PID"]).style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Row::new(["Model", "Stack", "VRAM", "State"]).style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        )
    };

    let blink = app.blink_on();
    let rows = app.tenants.iter().map(|t| {
        let stack_style = stack_style(theme, t.stack, blink);
        let share = format!("{} {:>3}%", mini_bar(t.share_pct, 6), t.share_pct);
        let ctx = t
            .context
            .map(|c| c.to_string())
            .unwrap_or_else(|| "—".into());
        let pid = t.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into());
        let quant = if t.quant.is_empty() {
            "—".into()
        } else {
            t.quant.clone()
        };
        if wide {
            Row::new(vec![
                Cell::from(Span::styled(t.stack.label().to_string(), stack_style)),
                Cell::from(Span::styled(
                    t.name.clone(),
                    Style::default().fg(
                        if app
                            .selected_tenant()
                            .is_some_and(|selected| selected.key == t.key)
                        {
                            theme.accent
                        } else {
                            theme.fg
                        },
                    ),
                )),
                Cell::from(Span::styled(quant, Style::default().fg(theme.fg_dim))),
                Cell::from(Span::styled(
                    human_mib(t.vram_mib),
                    Style::default().fg(theme.accent),
                )),
                Cell::from(Span::styled(
                    share,
                    Style::default().fg(stack_color(theme, t.stack)),
                )),
                Cell::from(ctx),
                Cell::from(Span::styled(
                    t.status.clone(),
                    Style::default().fg(if t.status == "loaded" {
                        theme.green
                    } else {
                        theme.fg
                    }),
                )),
                Cell::from(pid),
            ])
        } else if medium {
            Row::new(vec![
                Cell::from(Span::styled(t.stack.label().to_string(), stack_style)),
                Cell::from(Span::styled(
                    t.name.clone(),
                    Style::default().fg(
                        if app
                            .selected_tenant()
                            .is_some_and(|selected| selected.key == t.key)
                        {
                            theme.accent
                        } else {
                            theme.fg
                        },
                    ),
                )),
                Cell::from(Span::styled(
                    human_mib(t.vram_mib),
                    Style::default().fg(theme.accent),
                )),
                Cell::from(Span::styled(
                    share,
                    Style::default().fg(stack_color(theme, t.stack)),
                )),
                Cell::from(Span::styled(
                    t.status.clone(),
                    Style::default().fg(if t.status == "loaded" {
                        theme.green
                    } else {
                        theme.fg
                    }),
                )),
                Cell::from(pid),
            ])
        } else {
            Row::new(vec![
                Cell::from(Span::styled(
                    t.name.clone(),
                    Style::default().fg(
                        if app
                            .selected_tenant()
                            .is_some_and(|selected| selected.key == t.key)
                        {
                            theme.accent
                        } else {
                            theme.fg
                        },
                    ),
                )),
                Cell::from(Span::styled(t.stack.label().to_string(), stack_style)),
                Cell::from(human_mib(t.vram_mib)),
                Cell::from(Span::styled(
                    t.status.clone(),
                    Style::default().fg(if t.status == "loaded" {
                        theme.green
                    } else {
                        theme.fg
                    }),
                )),
            ])
        }
    });

    let widths: Vec<Constraint> = if wide {
        vec![
            Constraint::Length(9),
            Constraint::Min(16),
            Constraint::Length(8),
            Constraint::Length(7),
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(8),
        ]
    } else if medium {
        vec![
            Constraint::Length(9),
            Constraint::Min(14),
            Constraint::Length(7),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
        ]
    } else {
        vec![
            Constraint::Percentage(42),
            Constraint::Percentage(22),
            Constraint::Percentage(20),
            Constraint::Percentage(16),
        ]
    };

    let row_height = if roomy && !app.inspect_focus { 2 } else { 1 };
    let rows = rows.map(|row| row.height(row_height));
    let table = Table::new(rows, widths)
        .header(header)
        .highlight_symbol("▸ ")
        .row_highlight_style(Style::default().bg(theme.selection))
        .style(theme.base());
    let mut state = ratatui::widgets::TableState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(table, table_area, &mut state);
    app.table_row_height = row_height;
    app.table_row_offset = state.offset();
    let mut note = vec![Span::styled(
        app.tenants
            .get(app.selected)
            .map(|t| format!("Selected {} · ", t.name))
            .unwrap_or_else(|| "Select a model to inspect its details".into()),
        Style::default().fg(theme.muted),
    )];
    if let Some(tenant) = app.selected_tenant() {
        match tenant.kind {
            crate::app::TenantKind::OllamaModel | crate::app::TenantKind::LmsModel => note.extend(
                crate::aurora::shortcut_line(
                    &[("u", "unload"), ("U", "unload stack"), ("p", "park other")],
                    Style::default().fg(theme.accent),
                    Style::default().fg(theme.muted),
                )
                .spans,
            ),
            crate::app::TenantKind::Orphan => note.extend(
                crate::aurora::shortcut_line(
                    &[("x", "kill orphan (confirm)")],
                    Style::default().fg(theme.accent),
                    Style::default().fg(theme.muted),
                )
                .spans,
            ),
            crate::app::TenantKind::Process => note.push(Span::styled(
                hint_for(tenant),
                Style::default().fg(theme.muted),
            )),
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(note)).style(theme.base()),
        note_area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let theme = &app.theme;
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let keys = crate::navigation::shortcuts(
        &app.footer_hints(),
        split[0].width,
        Style::default().fg(theme.accent),
        Style::default().fg(theme.muted),
    );
    frame.render_widget(Paragraph::new(keys).style(theme.status()), split[0]);

    let status = match &app.mode {
        Mode::Confirm(c) => c.prompt.clone(),
        Mode::Load(_) => "type to filter    enter load    esc close".into(),
        Mode::Help => "any key closes help".into(),
        Mode::Normal => {
            let age = poll_age(app);
            if app.status.is_empty() {
                if age == "live" {
                    String::new()
                } else {
                    age
                }
            } else {
                format!("{}   {age}", app.status)
            }
        }
    };
    let warn = if app.over_budget(0) {
        Span::styled(
            "  OVER BUDGET",
            Style::default().fg(theme.red).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };
    let err = app
        .snapshot
        .ollama_error
        .as_ref()
        .or(app.snapshot.lms_error.as_ref())
        .or(app.snapshot.gpu.error.as_ref())
        .filter(|e| !is_unreachable(e));
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(status, Style::default().fg(theme.fg)),
        warn,
    ];
    if let Some(err) = err {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            short_error(err),
            Style::default().fg(theme.yellow),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme.status()),
        split[1],
    );
}

fn draw_help(frame: &mut Frame, app: &App) {
    let area = centered(frame.area(), 72, 18);
    frame.render_widget(Clear, area);
    let theme = &app.theme;
    let body = vec![
        Line::from(Span::styled(
            "GPU landlord",
            Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Ollama and LM Studio on one VRAM bar",
            Style::default().fg(theme.muted),
        )),
        Line::from(""),
        Line::from("j/k  move     u  unload selected     U  unload that stack"),
        Line::from("p    park the other stack's models"),
        Line::from("l    load a model onto the focused stack"),
        Line::from("o    start/stop Ollama     m  start/stop LM Studio"),
        Line::from("x    kill selected orphan  r  refresh  ^K actions"),
        Line::from("F2 Navigate  F6 model list / inspector  PgUp/PgDn details"),
        Line::from(""),
        Line::from(Span::styled(
            "Polling never starts LM Studio. lms runs only after its daemon pid is alive.",
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            "Budget defaults to total VRAM minus 2 GiB.",
            Style::default().fg(theme.muted),
        )),
    ];
    frame.render_widget(
        Paragraph::new(body).style(theme.base()).block(
            Block::default()
                .title(Span::styled(" help ", Style::default().fg(theme.accent)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent)),
        ),
        area,
    );
}

fn draw_load(frame: &mut Frame, app: &App, ui: &LoadUi) {
    let area = centered(frame.area(), 74, 18);
    frame.render_widget(Clear, area);
    let items = visible_load_items(ui);
    let selected = if items.is_empty() {
        0
    } else {
        ui.selected.min(items.len() - 1)
    };
    let accent = match ui.stack {
        Stack::Ollama => app.theme.cyan,
        Stack::LmStudio => app.theme.magenta,
        _ => app.theme.accent,
    };
    app.navigation.clear_hits();
    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    let capacity = inner.height.saturating_sub(3) as usize;
    let start = selected.saturating_add(1).saturating_sub(capacity);
    let list_area = Rect::new(
        inner.x,
        inner.y + 3,
        inner.width,
        inner.height.saturating_sub(3),
    );
    app.navigation
        .hit(list_area, crate::navigation::Intent::Pane(98));
    app.navigation
        .rows(list_area, 98, start, 1, items.len().saturating_sub(start));
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                format!("Load on {}", ui.stack.label()),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                format!("{} shown", items.len()),
                Style::default().fg(app.theme.muted),
            ),
        ]),
        Line::from(vec![
            Span::styled("filter ", Style::default().fg(app.theme.muted)),
            Span::styled(
                if ui.query.is_empty() {
                    "type to search".into()
                } else {
                    ui.query.clone()
                },
                Style::default().fg(app.theme.fg),
            ),
        ]),
        Line::from(""),
    ];
    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "No models",
            Style::default().fg(app.theme.yellow),
        )));
    } else {
        for (i, item) in items.iter().enumerate().skip(start).take(capacity) {
            let marker = if i == selected { "▸ " } else { "  " };
            let style = if i == selected {
                app.theme.selected()
            } else {
                app.theme.base()
            };
            lines.push(Line::from(Span::styled(
                format!("{marker}{}  {}", item.label, item.meta),
                style,
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines).style(app.theme.base()).block(
            Block::default()
                .title(Span::styled(" load ", Style::default().fg(accent)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(accent)),
        ),
        area,
    );
}

fn draw_confirm(frame: &mut Frame, app: &App, confirm: &Confirm) {
    let area = centered(frame.area(), 56, 8);
    frame.render_widget(Clear, area);
    let theme = &app.theme;
    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            confirm.prompt.trim_end_matches(" y/n").to_string(),
            Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            keycap("y", theme),
            Span::styled(" yes    ", Style::default().fg(theme.green)),
            keycap("n", theme),
            Span::styled(" no", Style::default().fg(theme.red)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .style(theme.base())
            .block(
                Block::default()
                    .title(Span::styled(" confirm ", Style::default().fg(theme.accent)))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent)),
            ),
        area,
    );
    app.navigation
        .dialog_buttons(frame, area, app.navigation_skin());
}

fn keycap(key: &str, theme: &Theme) -> Span<'static> {
    Span::styled(
        format!("({key})"),
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD),
    )
}

fn stack_color(theme: &Theme, stack: Stack) -> ratatui::style::Color {
    match stack {
        Stack::Ollama => theme.cyan,
        Stack::LmStudio => theme.magenta,
        Stack::Orphan => theme.red,
        Stack::Other => theme.yellow,
    }
}

fn stack_style(theme: &Theme, stack: Stack, blink: bool) -> Style {
    let color = stack_color(theme, stack);
    if stack == Stack::Orphan && !blink {
        Style::default()
            .fg(theme.yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    }
}

fn mini_bar(pct: u8, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = ((pct as usize) * width) / 100;
    let mut out = String::new();
    for i in 0..width {
        out.push(if i < filled { '█' } else { '░' });
    }
    out
}

fn short_gpu(name: &str) -> String {
    name.replace("NVIDIA GeForce ", "")
        .replace("NVIDIA ", "")
        .replace(" Laptop GPU", "")
}

fn watts(w: f32) -> String {
    if w >= 10.0 {
        format!("{w:.0}W")
    } else {
        format!("{w:.1}W")
    }
}

fn temp_style(theme: &Theme, temp: u8) -> Style {
    if temp >= 80 {
        Style::default().fg(theme.red)
    } else if temp >= 70 {
        Style::default().fg(theme.yellow)
    } else {
        Style::default().fg(theme.green)
    }
}

fn poll_age(app: &App) -> String {
    match app.last_poll {
        Some(t) => {
            let s = t.elapsed().as_secs();
            if s < 2 {
                "live".into()
            } else {
                format!("{s}s ago")
            }
        }
        None => "waiting for nvidia-smi".into(),
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod aurora_audit {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn commands() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)
    }
    fn fixture() -> App {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(crate::config::Config::default(), crate::theme::aurora(), tx);
        app.snapshot.gpu.name = "NVIDIA RTX 4080".into();
        app.snapshot.gpu.util_pct = 38;
        app.snapshot.gpu.total_mib = 16384;
        app.snapshot.gpu.used_mib = 11468;
        app.snapshot.gpu.temp_c = 48;
        app.snapshot.ollama_online = true;
        app.last_poll = Some(std::time::Instant::now());
        app.occupancy = crate::app::Occupancy {
            total: 16384,
            used: 11468,
            free: 4916,
            ollama: 9523,
            other: 1945,
            ..Default::default()
        };
        for (name, size) in [("qwen3:8b", 6246), ("gemma3:4b", 3277)] {
            app.tenants.push(crate::app::Tenant {
                key: name.into(),
                stack: Stack::Ollama,
                name: name.into(),
                vram_mib: size,
                share_pct: 38,
                context: Some(8192),
                status: "loaded".into(),
                pid: None,
                kind: crate::app::TenantKind::OllamaModel,
                unload_id: Some(name.into()),
                quant: "Q4_K_M".into(),
                size_mib: size,
                ram_mib: 0,
            });
        }
        app
    }

    #[tokio::test]
    async fn common_navigation_contract_preserves_focus_and_drafts() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = fixture();
        app.focus_pane(0);
        if app.navigation_editing() {
            let _ = app.on_key(key(KeyCode::Esc));
        }
        let screen = app.navigation_screen();
        let pane = app.focused_pane();
        let _ = app.on_key(key(KeyCode::Tab));
        assert_eq!(
            app.navigation_screen(),
            crate::navigation::step(
                screen,
                app.navigation_entries()
                    .iter()
                    .filter(|e| matches!(e.target, crate::navigation::Intent::Screen(_)))
                    .count(),
                1
            )
        );
        if app.navigation_editing() {
            let _ = app.on_key(key(KeyCode::Esc));
        }
        let _ = app.on_key(key(KeyCode::BackTab));
        assert_eq!(app.navigation_screen(), screen);
        let _ = app.on_key(key(KeyCode::F(6)));
        assert_eq!(
            app.focused_pane(),
            crate::navigation::step(pane, app.pane_names().len(), 1)
        );
        let _ = app.on_key(KeyEvent::new(KeyCode::F(6), KeyModifiers::SHIFT));
        assert_eq!(app.focused_pane(), pane);
        let _ = app.on_key(key(KeyCode::F(2)));
        assert!(app.navigation.open);
        let _ = app.on_key(key(KeyCode::Char('1')));
        assert!(!app.navigation.open);
        assert_eq!(app.navigation_screen(), 0);
        app.select_screen(screen);
        let _ = app.on_key(key(KeyCode::F(2)));
        assert!(app.navigation.open);

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        assert!(crate::audit::text(terminal.backend().buffer()).contains("Navigate"));
        let _ = app.on_key(key(KeyCode::Esc));
        assert!(!app.navigation.open);
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut hit = None;
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width.saturating_sub(3) {
                if buffer[(x, y)].symbol() == "("
                    && buffer[(x + 1, y)].symbol() == "F"
                    && buffer[(x + 2, y)].symbol() == "2"
                {
                    hit = Some((x, y));
                    break;
                }
            }
            if hit.is_some() {
                break;
            }
        }
        let (x, y) = hit.expect("visible clickable navigation");
        app.navigation_mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.navigation.open);
        let _ = app.on_key(key(KeyCode::Esc));

        let _ = app.on_key(commands());
        assert!(app.commands.open);
        let _ = app.on_key(key(KeyCode::Esc));
    }
    #[tokio::test]
    async fn all_screens_and_focus_targets_fit_the_navigation_matrix() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = fixture();
        let screens = app
            .navigation_entries()
            .iter()
            .filter(|e| matches!(e.target, crate::navigation::Intent::Screen(_)))
            .count();
        for screen in 0..screens {
            app.select_screen(screen);
            for pane in 0..app.pane_names().len() {
                app.focus_pane(pane);
                for &(w, h) in &crate::audit::SIZES[..4] {
                    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                    terminal.draw(|f| draw(f, &mut app)).unwrap();
                    let buffer = terminal.backend().buffer();
                    assert!(
                        crate::audit::text(buffer).contains("(F2)"),
                        "screen {screen} pane {pane} must expose navigation at {w}x{h}"
                    );
                    crate::audit::verify_footer(buffer);
                    crate::audit::capture(
                        "glim",
                        &format!("navigation-screen{screen}-pane{pane}"),
                        buffer,
                    );
                    let before = app.navigation_screen();
                    app.navigation_mouse(crossterm::event::MouseEvent {
                        kind: crossterm::event::MouseEventKind::Down(
                            crossterm::event::MouseButton::Left,
                        ),
                        column: w + 1,
                        row: h + 1,
                        modifiers: KeyModifiers::NONE,
                    });
                    assert_eq!(app.navigation_screen(), before);
                }
            }
        }
    }
    #[test]
    fn dialogs_own_commands_and_kill_still_confirms() {
        let mut app = fixture();
        app.tenants[0].kind = crate::app::TenantKind::Orphan;
        app.tenants[0].stack = Stack::Orphan;
        app.tenants[0].pid = Some(12345);
        app.on_key(key(KeyCode::Char('x')));
        assert!(matches!(app.mode, Mode::Confirm(_)));
        app.on_key(commands());
        assert!(!app.commands.open);
        app.on_key(key(KeyCode::Esc));
        app.on_key(commands());
        assert!(app.commands.open);
        app.on_key(key(KeyCode::Char('u')));
        assert!(matches!(app.mode, Mode::Normal));
    }
    #[test]
    #[ignore = "interactive mock fixture for visual QA"]
    fn live_preview() {
        let mut app = fixture();
        match std::env::var("AURORA_NAV_PREVIEW").as_deref() {
            Ok("menu") => app.navigation.toggle(),
            Ok("actions") => app.commands.open = true,
            Ok("focused") => app.focus_pane(1),
            _ => {}
        }
        crate::audit::live_preview(
            &mut app,
            |f, a| draw(f, a),
            |a, k| {
                if matches!(
                    k.code,
                    KeyCode::Tab | KeyCode::BackTab | KeyCode::Down | KeyCode::Up | KeyCode::Esc
                ) {
                    a.on_key(k);
                }
            },
        );
    }
    #[test]
    fn responsive_render_matrix() {
        let mut app = fixture();
        // One terminal exercises shrink/grow transitions, rather than independent initial frames.
        let mut resizing = Terminal::new(TestBackend::new(120, 40)).unwrap();
        for &(w, h) in &[
            (120, 40),
            (60, 20),
            (80, 24),
            (30, 10),
            (100, 30),
            (120, 40),
        ] {
            resizing.backend_mut().resize(w, h);
            resizing.draw(|f| draw(f, &mut app)).unwrap();
            assert_eq!(resizing.backend().buffer().area.width, w);
        }
        for &(w, h) in crate::audit::SIZES {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| draw(f, &mut app)).unwrap();
            let buffer = term.backend().buffer();
            if w >= 30 && h >= 14 {
                assert!(buffer
                    .content
                    .iter()
                    .any(|cell| matches!(cell.symbol(), "█" | "▀" | "▄")));
            }
            crate::audit::verify_footer(buffer);
            crate::audit::capture("glim", "main", buffer);
        }
        app.inspect_focus = true;
        for &(w, h) in &crate::audit::SIZES[..4] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| draw(f, &mut app)).unwrap();
            crate::audit::capture("glim", "focused", term.backend().buffer());
        }
        app.on_key(commands());
        for &(w, h) in &crate::audit::SIZES[..4] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| draw(f, &mut app)).unwrap();
            let buffer = term.backend().buffer();
            assert!(crate::audit::text(buffer).contains("Actions"));
            crate::audit::capture("glim", "actions", buffer);
        }
        app.commands.open = false;
        app.tenants.clear();
        app.snapshot = Default::default();
        app.occupancy = Default::default();
        app.last_poll = None;
        app.inspect_focus = false;
        for &(w, h) in &crate::audit::SIZES[..4] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| draw(f, &mut app)).unwrap();
            crate::audit::verify_footer(term.backend().buffer());
            crate::audit::capture("glim", "offline", term.backend().buffer());
        }
    }
}
