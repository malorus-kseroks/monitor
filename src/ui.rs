use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Cell, Chart, Clear, Dataset, Gauge, GraphType, Paragraph, Row, Table,
        TableState, Tabs, Wrap,
    },
};

use crate::{
    app::{AppState, HISTORY_SAMPLES, PendingAction},
    cli::{ColorMode, Page},
    domain::{HardwareMetric, ModuleState},
    i18n::{TextKey, tr},
    sanitize::terminal_text,
};

#[derive(Clone, Copy)]
struct Theme {
    accent: Color,
    ok: Color,
    warning: Color,
    error: Color,
    muted: Color,
    selection: Style,
}

impl Theme {
    fn for_app(app: &AppState) -> Self {
        if app.config.color == ColorMode::Never {
            Self {
                accent: Color::Reset,
                ok: Color::Reset,
                warning: Color::Reset,
                error: Color::Reset,
                muted: Color::Reset,
                selection: Style::default().add_modifier(Modifier::REVERSED),
            }
        } else {
            Self {
                accent: Color::Cyan,
                ok: Color::Green,
                warning: Color::Yellow,
                error: Color::Red,
                muted: Color::DarkGray,
                selection: Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            }
        }
    }
}

pub fn render(frame: &mut Frame<'_>, app: &AppState) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        render_too_small(frame, app, area);
        return;
    }
    let theme = Theme::for_app(app);
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .split(area);
    render_header(frame, app, layout[0], theme);
    match app.page {
        Page::Overview => render_overview(frame, app, layout[1], theme),
        Page::Processes => render_processes(frame, app, layout[1], theme),
        Page::Storage => render_storage(frame, app, layout[1], theme),
        Page::Containers => render_containers(frame, app, layout[1], theme),
        Page::Network => render_network(frame, app, layout[1], theme),
        Page::Services => render_services(frame, app, layout[1], theme),
        Page::Hardware => render_hardware(frame, app, layout[1], theme),
    }
    render_status(frame, app, layout[2], theme);
    render_keys(frame, app, layout[3], theme);
    if app.show_help {
        render_help(frame, app, theme);
    }
    if let Some(pending) = &app.pending {
        render_confirmation(frame, app, pending, theme);
    }
    #[cfg(feature = "containers")]
    if let Some(logs) = &app.container_logs {
        render_container_logs(frame, logs, theme);
    }
    if let Some(process) = &app.process_details {
        render_process_details(frame, process, theme);
    }
}

fn render_header(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    if area.width < 110 {
        let index = app
            .pages
            .iter()
            .position(|page| page == &app.page)
            .unwrap_or(0)
            + 1;
        let text = format!(
            " {} · {index}/{} {} ",
            tr(app.language, TextKey::AppTitle),
            app.pages.len(),
            page_name(app, app.page)
        );
        frame.render_widget(
            Paragraph::new(text)
                .block(Block::default().borders(Borders::ALL))
                .style(
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
            area,
        );
        return;
    }
    let titles = app
        .pages
        .iter()
        .map(|page| Line::from(format!(" {} ", page_name(app, *page))))
        .collect::<Vec<_>>();
    let selected = app
        .pages
        .iter()
        .position(|page| page == &app.page)
        .unwrap_or(0);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(tr(app.language, TextKey::AppTitle)),
            )
            .highlight_style(theme.selection)
            .style(Style::default().fg(theme.accent)),
        area,
    );
}

fn render_overview(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    let Some(system) = app.system.data() else {
        render_state(frame, &app.system, app, area, theme);
        return;
    };
    let overview = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    let graphs = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(overview[0]);

    let cpu_primary = format!(
        "{:5.1}% · {} MHz · {} cores",
        system.cpu.total_percent,
        system.cpu.frequency_mhz,
        system.cpu.per_core_percent.len()
    );
    render_history_chart(
        frame,
        graphs[0],
        tr(app.language, TextKey::Cpu),
        tr(app.language, TextKey::History),
        &app.cpu_history,
        system.cpu.total_percent as f64,
        history_summary(cpu_primary, &app.cpu_history, graphs[0].width >= 60),
        app.config.interval,
        app.config.ascii,
        theme,
    );
    let memory_percent = if system.memory_total == 0 {
        0.0
    } else {
        system.memory_used as f64 / system.memory_total as f64 * 100.0
    };
    let memory_primary = format!(
        "{} / {} · {:.1}%",
        format_bytes(system.memory_used),
        format_bytes(system.memory_total),
        memory_percent
    );
    render_history_chart(
        frame,
        graphs[1],
        tr(app.language, TextKey::Memory),
        tr(app.language, TextKey::History),
        &app.memory_history,
        memory_percent,
        history_summary(memory_primary, &app.memory_history, graphs[1].width >= 60),
        app.config.interval,
        app.config.ascii,
        theme,
    );

    let summary_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(overview[1]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(summary_columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(summary_columns[1]);

    let host = vec![
        Line::from(format!(
            "{}: {}",
            tr(app.language, TextKey::Host),
            terminal_text(&system.host_name, 128)
        )),
        Line::from(format!("OS: {}", terminal_text(&system.os_name, 160))),
        Line::from(format!(
            "{}: {}",
            tr(app.language, TextKey::Kernel),
            terminal_text(&system.kernel_version, 128)
        )),
        Line::from(format!(
            "{}: {}",
            tr(app.language, TextKey::Uptime),
            format_duration(system.uptime_seconds)
        )),
    ];
    frame.render_widget(
        Paragraph::new(host).block(panel(" System ", theme)),
        left[0],
    );
    render_memory_gauge(
        frame,
        left[1],
        tr(app.language, TextKey::Swap),
        system.swap_used,
        system.swap_total,
        theme,
    );
    let storage_used = system
        .storage
        .iter()
        .map(|disk| disk.total_bytes.saturating_sub(disk.available_bytes))
        .sum::<u64>();
    let storage_total = system
        .storage
        .iter()
        .map(|disk| disk.total_bytes)
        .sum::<u64>();
    let network_rx = system
        .networks
        .iter()
        .map(|network| network.received_per_second)
        .sum::<f64>();
    let network_tx = system
        .networks
        .iter()
        .map(|network| network.transmitted_per_second)
        .sum::<f64>();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "{} / {}",
                format_bytes(storage_used),
                format_bytes(storage_total)
            )),
            Line::from(format!("{} filesystems", system.storage.len())),
        ])
        .block(panel(
            &format!(" {} ", tr(app.language, TextKey::Storage)),
            theme,
        )),
        right[0],
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "RX {} · TX {}",
                format_rate(network_rx),
                format_rate(network_tx)
            )),
            Line::from(format!("{} interfaces", system.networks.len())),
        ])
        .block(panel(
            &format!(" {} ", tr(app.language, TextKey::Network)),
            theme,
        )),
        right[1],
    );
}

#[allow(clippy::too_many_arguments)]
fn render_history_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    history_label: &str,
    history: &std::collections::VecDeque<f64>,
    current: f64,
    summary: String,
    interval: std::time::Duration,
    ascii: bool,
    theme: Theme,
) {
    let block = panel(&format!(" {title} · {history_label} "), theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);
    frame.render_widget(
        Paragraph::new(summary)
            .alignment(Alignment::Center)
            .style(Style::default().fg(usage_color(current, theme))),
        parts[0],
    );
    if parts[1].is_empty() {
        return;
    }
    if ascii {
        frame.render_widget(
            Paragraph::new(ascii_history(history, parts[1].width, parts[1].height))
                .style(Style::default().fg(usage_color(current, theme))),
            parts[1],
        );
        return;
    }

    let x_max = HISTORY_SAMPLES.saturating_sub(1).max(1) as f64;
    let x_offset = HISTORY_SAMPLES.saturating_sub(history.len());
    let data = history
        .iter()
        .enumerate()
        .map(|(index, value)| ((x_offset + index) as f64, *value))
        .collect::<Vec<_>>();
    let marker = chart_marker();
    let guide_data = [(0.0, 50.0), (x_max, 50.0)];
    let guide = Dataset::default()
        .marker(marker)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme.muted))
        .data(&guide_data);
    let dataset = Dataset::default()
        .marker(marker)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(usage_color(current, theme)))
        .data(&data);
    let window = interval.saturating_mul(HISTORY_SAMPLES.saturating_sub(1) as u32);
    let chart = Chart::new(vec![guide, dataset])
        .x_axis(
            Axis::default()
                .bounds([0.0, x_max])
                .labels([
                    Line::from(format!("-{}", format_chart_window(window))),
                    Line::from("now"),
                ])
                .style(Style::default().fg(theme.muted)),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, 100.0])
                .labels(["0%", "50%", "100%"])
                .style(Style::default().fg(theme.muted)),
        );
    frame.render_widget(chart, parts[1]);
}

fn chart_marker() -> symbols::Marker {
    if cfg!(target_os = "windows") {
        symbols::Marker::HalfBlock
    } else {
        symbols::Marker::Braille
    }
}

fn history_summary(
    primary: String,
    history: &std::collections::VecDeque<f64>,
    show_stats: bool,
) -> String {
    if !show_stats || history.is_empty() {
        return primary;
    }
    let average = history.iter().sum::<f64>() / history.len() as f64;
    let maximum = history.iter().copied().fold(0.0_f64, f64::max);
    format!("{primary} · avg {average:.1}% · max {maximum:.1}%")
}

fn format_chart_window(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs_f64();
    if total_seconds < 60.0 {
        format!("{total_seconds:.0}s")
    } else {
        format!("{:.1}m", total_seconds / 60.0)
    }
}

fn ascii_history(history: &std::collections::VecDeque<f64>, width: u16, height: u16) -> String {
    let rows = usize::from(height);
    let columns = usize::from(width.saturating_sub(4));
    if rows == 0 || columns == 0 {
        return String::new();
    }
    let mut grid = vec![vec![' '; rows]; columns];
    let active_columns = columns
        .saturating_mul(history.len())
        .div_ceil(HISTORY_SAMPLES)
        .min(columns);
    let start_column = columns.saturating_sub(active_columns);
    for (column, cells) in grid.iter_mut().enumerate().skip(start_column) {
        let local_column = column - start_column;
        let index = if active_columns <= 1 {
            history.len().saturating_sub(1)
        } else {
            local_column * (history.len() - 1) / (active_columns - 1)
        };
        let value = history
            .get(index)
            .copied()
            .unwrap_or_default()
            .clamp(0.0, 100.0);
        let row = if rows == 1 {
            0
        } else {
            ((100.0 - value) / 100.0 * (rows - 1) as f64).round() as usize
        };
        cells[row.min(rows - 1)] = '*';
    }
    (0..rows)
        .map(|row| {
            let label = if row == 0 {
                "100|"
            } else if row == rows / 2 {
                " 50|"
            } else if row + 1 == rows {
                "  0|"
            } else {
                "   |"
            };
            let points = grid.iter().map(|column| column[row]).collect::<String>();
            format!("{label}{points}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_processes(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    if app.system.data().is_none() {
        render_state(frame, &app.system, app, area, theme);
        return;
    }
    let processes = app.filtered_processes();
    let compact = area.width < 110;
    let rows = processes
        .iter()
        .map(|process| {
            let mut cells = vec![
                Cell::from(process.pid.to_string()),
                Cell::from(terminal_text(&process.name, if compact { 36 } else { 64 })),
                Cell::from(format!("{:.1}", process.cpu_percent)),
                Cell::from(format_bytes(process.memory_bytes)),
            ];
            if !compact {
                cells.push(Cell::from(terminal_text(&process.status, 24)));
            }
            Row::new(cells)
        })
        .collect::<Vec<_>>();
    let mut header = vec![
        "PID".to_owned(),
        tr(app.language, TextKey::Name).into(),
        "CPU%".into(),
        "RAM".into(),
    ];
    let mut widths = vec![
        Constraint::Length(8),
        Constraint::Min(20),
        Constraint::Length(8),
        Constraint::Length(12),
    ];
    if !compact {
        header.push(tr(app.language, TextKey::Status).into());
        widths.push(Constraint::Length(14));
    }
    let title = if app.search_mode {
        format!(" {}: {}_ ", tr(app.language, TextKey::Search), app.search)
    } else if app.search.is_empty() {
        format!(
            " {} · sort {:?} ",
            tr(app.language, TextKey::Processes),
            app.process_sort
        )
    } else {
        format!(" {}: {} ", tr(app.language, TextKey::Search), app.search)
    };
    let table = Table::new(rows, widths)
        .header(
            Row::new(header).style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(panel(&title, theme))
        .row_highlight_style(theme.selection)
        .highlight_symbol(if app.config.ascii { "> " } else { "▶ " });
    let mut state = TableState::default().with_selected(Some(app.process_selection));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_storage(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    let Some(system) = app.system.data() else {
        render_state(frame, &app.system, app, area, theme);
        return;
    };
    let compact = area.width < 110;
    let rows = system
        .storage
        .iter()
        .map(|disk| {
            let used = disk.total_bytes.saturating_sub(disk.available_bytes);
            if compact {
                Row::new(vec![
                    terminal_text(&disk.mount_point, 40),
                    format_bytes(used),
                    format_bytes(disk.total_bytes),
                    percent(used, disk.total_bytes),
                ])
            } else {
                Row::new(vec![
                    terminal_text(&disk.name, 24),
                    terminal_text(&disk.mount_point, 40),
                    terminal_text(&disk.file_system, 12),
                    format_bytes(used),
                    format_bytes(disk.total_bytes),
                    percent(used, disk.total_bytes),
                    disk.read_per_second.map_or_else(|| "-".into(), format_rate),
                    disk.write_per_second
                        .map_or_else(|| "-".into(), format_rate),
                ])
            }
        })
        .collect::<Vec<_>>();
    let (header, widths): (Vec<&str>, Vec<Constraint>) = if compact {
        (
            vec![
                tr(app.language, TextKey::Mount),
                tr(app.language, TextKey::Usage),
                "Total",
                "%",
            ],
            vec![
                Constraint::Min(24),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(8),
            ],
        )
    } else {
        (
            vec![
                tr(app.language, TextKey::Name),
                tr(app.language, TextKey::Mount),
                tr(app.language, TextKey::FileSystem),
                tr(app.language, TextKey::Usage),
                "Total",
                "%",
                "Read/s",
                "Write/s",
            ],
            vec![
                Constraint::Length(16),
                Constraint::Min(18),
                Constraint::Length(10),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Length(14),
                Constraint::Length(14),
            ],
        )
    };
    let table = Table::new(rows, widths)
        .header(
            Row::new(header).style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(panel(
            &format!(
                " {} · SMART: {} ",
                tr(app.language, TextKey::Storage),
                provider_available(app, "smartctl")
            ),
            theme,
        ))
        .row_highlight_style(theme.selection)
        .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(Some(app.storage_selection));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_containers(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    let Some(snapshot) = app.containers.data() else {
        render_state(frame, &app.containers, app, area, theme);
        return;
    };
    let compact = area.width < 110;
    let rows = snapshot
        .containers
        .iter()
        .map(|container| {
            let mut cells = vec![
                terminal_text(&container.name, 40),
                terminal_text(&container.state, 16),
                terminal_text(&container.image, 44),
            ];
            if !compact {
                cells.insert(0, terminal_text(&container.engine, 18));
                cells.push(
                    container
                        .cpu_percent
                        .map_or_else(|| "-".into(), |value| format!("{value:.1}%")),
                );
                cells.push(container.memory_used.map_or_else(
                    || "-".into(),
                    |used| {
                        container.memory_limit.map_or_else(
                            || format_bytes(used),
                            |limit| format!("{} / {}", format_bytes(used), format_bytes(limit)),
                        )
                    },
                ));
                cells.push(terminal_text(&container.status, 48));
            }
            Row::new(cells)
        })
        .collect::<Vec<_>>();
    let (header, widths): (Vec<&str>, Vec<Constraint>) = if compact {
        (
            vec![
                tr(app.language, TextKey::Name),
                tr(app.language, TextKey::Status),
                tr(app.language, TextKey::Image),
            ],
            vec![
                Constraint::Min(20),
                Constraint::Length(14),
                Constraint::Min(24),
            ],
        )
    } else {
        (
            vec![
                tr(app.language, TextKey::Engine),
                tr(app.language, TextKey::Name),
                tr(app.language, TextKey::Status),
                tr(app.language, TextKey::Image),
                "CPU",
                "Memory",
                "Details",
            ],
            vec![
                Constraint::Length(16),
                Constraint::Length(22),
                Constraint::Length(12),
                Constraint::Length(24),
                Constraint::Length(9),
                Constraint::Length(20),
                Constraint::Min(24),
            ],
        )
    };
    let table = Table::new(rows, widths)
        .header(
            Row::new(header).style(
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(panel(
            &format!(
                " {} · {} engines ",
                tr(app.language, TextKey::Containers),
                snapshot.engines.len()
            ),
            theme,
        ))
        .row_highlight_style(theme.selection)
        .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(Some(app.container_selection));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_network(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    let Some(system) = app.system.data() else {
        render_state(frame, &app.system, app, area, theme);
        return;
    };
    let rows = system
        .networks
        .iter()
        .map(|network| {
            Row::new(vec![
                terminal_text(&network.name, 24),
                terminal_text(&network.addresses.join(", "), 64),
                format_rate(network.received_per_second),
                format_rate(network.transmitted_per_second),
                format_bytes(network.total_received),
                format_bytes(network.total_transmitted),
            ])
        })
        .collect::<Vec<_>>();
    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Min(24),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Length(14),
        ],
    )
    .header(
        Row::new([
            tr(app.language, TextKey::Name),
            "IP",
            tr(app.language, TextKey::Received),
            tr(app.language, TextKey::Transmitted),
            "RX total",
            "TX total",
        ])
        .style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(panel(
        &format!(
            " {} · Wi-Fi: {} ",
            tr(app.language, TextKey::Network),
            provider_available(app, "network-manager")
        ),
        theme,
    ))
    .row_highlight_style(theme.selection)
    .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(Some(app.network_selection));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_services(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    if !cfg!(target_os = "linux") {
        render_linux_only(frame, app, area, theme);
        return;
    }
    let Some(platform) = app.platform.data() else {
        render_state(frame, &app.platform, app, area, theme);
        return;
    };
    let rows = platform
        .services
        .iter()
        .map(|service| {
            Row::new(vec![
                terminal_text(&service.name, 60),
                terminal_text(&service.state, 24),
                terminal_text(&service.description, 100),
            ])
        })
        .collect::<Vec<_>>();
    let manager = platform
        .provider_reports
        .iter()
        .find(|report| report.provider.starts_with("services:"))
        .map_or("unknown", |report| {
            report.provider.trim_start_matches("services:")
        });
    let table = Table::new(
        rows,
        [
            Constraint::Length(30),
            Constraint::Length(16),
            Constraint::Min(32),
        ],
    )
    .header(
        Row::new([
            tr(app.language, TextKey::Name),
            tr(app.language, TextKey::Status),
            "Description",
        ])
        .style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(panel(
        &format!(
            " {} · {}: {} ",
            tr(app.language, TextKey::Services),
            tr(app.language, TextKey::ServiceManager),
            manager
        ),
        theme,
    ))
    .row_highlight_style(theme.selection)
    .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(Some(app.service_selection));
    frame.render_stateful_widget(table, area, &mut state);
}

fn render_hardware(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    if !cfg!(target_os = "linux") {
        render_linux_only(frame, app, area, theme);
        return;
    }
    let Some(platform) = app.platform.data() else {
        render_state(frame, &app.platform, app, area, theme);
        return;
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(columns[1]);
    frame.render_widget(
        Paragraph::new(groups_to_lines(&platform.gpus))
            .block(panel(" GPU ", theme))
            .wrap(Wrap { trim: true }),
        left[0],
    );
    frame.render_widget(
        Paragraph::new(metrics_to_lines(&platform.sensors))
            .block(panel(" Sensors ", theme))
            .wrap(Wrap { trim: true }),
        left[1],
    );
    let mut batteries = groups_to_lines(&platform.batteries);
    batteries.extend(groups_to_lines(&platform.displays));
    frame.render_widget(
        Paragraph::new(batteries)
            .block(panel(" Battery & Display ", theme))
            .wrap(Wrap { trim: true }),
        right[0],
    );
    let capabilities = platform
        .provider_reports
        .iter()
        .map(|report| {
            Line::from(vec![
                Span::styled(
                    if report.state == crate::domain::ProbeState::Ready {
                        "> "
                    } else {
                        "! "
                    },
                    Style::default().fg(if report.state == crate::domain::ProbeState::Ready {
                        theme.ok
                    } else {
                        theme.warning
                    }),
                ),
                Span::raw(format!("{}: {:?}", report.provider, report.state)),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(capabilities)
            .block(panel(" Capabilities ", theme))
            .wrap(Wrap { trim: true }),
        right[1],
    );
}

fn render_state<T>(
    frame: &mut Frame<'_>,
    state: &ModuleState<T>,
    app: &AppState,
    area: Rect,
    theme: Theme,
) {
    let (title, body, color) = match state {
        ModuleState::Loading => (
            tr(app.language, TextKey::Loading),
            "Please wait".to_owned(),
            theme.accent,
        ),
        ModuleState::Empty { message } => (
            tr(app.language, TextKey::Empty),
            terminal_text(message, 1024),
            theme.muted,
        ),
        ModuleState::Degraded { reason, .. } => {
            ("Degraded", terminal_text(reason, 1024), theme.warning)
        }
        ModuleState::Unavailable {
            reason,
            remediation,
        } => (
            tr(app.language, TextKey::Unavailable),
            format!(
                "{}{}",
                terminal_text(reason, 768),
                remediation.as_ref().map_or(String::new(), |text| format!(
                    "\n\n{}",
                    terminal_text(text, 768)
                ))
            ),
            theme.warning,
        ),
        ModuleState::PermissionDenied {
            reason,
            remediation,
        } => (
            tr(app.language, TextKey::PermissionDenied),
            format!(
                "{}{}",
                terminal_text(reason, 768),
                remediation.as_ref().map_or(String::new(), |text| format!(
                    "\n\n{}",
                    terminal_text(text, 768)
                ))
            ),
            theme.error,
        ),
        ModuleState::Error { summary, details } => (
            tr(app.language, TextKey::Error),
            format!(
                "{}{}",
                terminal_text(summary, 768),
                details.as_ref().map_or(String::new(), |text| format!(
                    "\n\n{}",
                    terminal_text(text, 768)
                ))
            ),
            theme.error,
        ),
        ModuleState::Stale { reason, .. } => (
            tr(app.language, TextKey::Stale),
            terminal_text(reason, 1024),
            theme.warning,
        ),
        ModuleState::Ready { .. } => (
            tr(app.language, TextKey::NoData),
            String::new(),
            theme.muted,
        ),
    };
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(color))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {title} ")),
            ),
        area,
    );
}

fn render_linux_only(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    frame.render_widget(Paragraph::new("This capability is available only in Linux builds.\nSet show_unsupported_modules=false to hide this page.").alignment(Alignment::Center).style(Style::default().fg(theme.warning)).block(panel(&format!(" {} ", tr(app.language, TextKey::Unavailable)), theme)), area);
}

fn render_status(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    let (ok, text) = app.status.clone().unwrap_or((
        true,
        format!("{} - {:?}", tr(app.language, TextKey::Refresh), app.page),
    ));
    let marker = if ok { ">" } else { "!" };
    frame.render_widget(
        Paragraph::new(format!(
            " {marker} {} ",
            terminal_text(&text, area.width.saturating_sub(4) as usize)
        ))
        .style(Style::default().fg(if ok { theme.ok } else { theme.error }))
        .block(Block::default().borders(Borders::TOP)),
        area,
    );
}

fn render_keys(frame: &mut Frame<'_>, app: &AppState, area: Rect, theme: Theme) {
    let contextual = match app.page {
        Page::Processes => " Enter Details · / Search · c/m/p/n Sort · k Terminate ",
        Page::Containers => " Enter Logs · t Start · s Stop · r Restart ",
        Page::Services => " t Start · s Stop · r Restart ",
        _ => " F5 Refresh ",
    };
    frame.render_widget(
        Paragraph::new(format!(
            " q {} · Tab Page · l Lang · ? {} ·{}",
            tr(app.language, TextKey::Quit),
            tr(app.language, TextKey::Help),
            contextual
        ))
        .style(Style::default().fg(theme.muted)),
        area,
    );
}

fn render_help(frame: &mut Frame<'_>, app: &AppState, theme: Theme) {
    let area = centered(76, 78, frame.area());
    frame.render_widget(Clear, area);
    let help = "Global\n  q  Quit     Ctrl+C  Quit from any state\n  Tab / Shift+Tab  Next / previous page\n  1..7  Direct page     F5  Refresh\n  l  Language     ? / F1  Help\n\nLists\n  Up/Down, PgUp/PgDn, Home\n\nProcesses\n  Enter  Details     /  Search     c/m/p/n  Sort\n  k  Terminate     Shift+K  Force kill\n\nContainers\n  Enter  Logs     t  Start     s  Stop     r  Restart\n\nServices\n  t  Start     s  Stop     r  Restart\n\nDisruptive actions require a separate y confirmation.";
    frame.render_widget(
        Paragraph::new(help).wrap(Wrap { trim: false }).block(panel(
            &format!(" {} ", tr(app.language, TextKey::Help)),
            theme,
        )),
        area,
    );
}

fn render_confirmation(
    frame: &mut Frame<'_>,
    app: &AppState,
    pending: &PendingAction,
    theme: Theme,
) {
    let area = centered(66, 32, frame.area());
    frame.render_widget(Clear, area);
    let description = match pending {
        PendingAction::Process {
            pid, name, force, ..
        } => format!(
            "{} PID {pid} ({})",
            if *force {
                "Force terminate"
            } else {
                "Terminate"
            },
            terminal_text(name, 128)
        ),
        #[cfg(feature = "containers")]
        PendingAction::Container { name, kind, .. } => {
            format!("{:?} container {}", kind, terminal_text(name, 128))
        }
        PendingAction::Service { name, kind, .. } => {
            format!("{:?} service {}", kind, terminal_text(name, 128))
        }
    };
    let text = format!(
        "{}\n\n{}\n\n[n/Esc] {}",
        description,
        tr(app.language, TextKey::PressY),
        tr(app.language, TextKey::Cancel)
    );
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(theme.warning))
            .block(panel(
                &format!(" {} ", tr(app.language, TextKey::Confirm)),
                theme,
            )),
        area,
    );
}

#[cfg(feature = "containers")]
fn render_container_logs(frame: &mut Frame<'_>, logs: &crate::app::ContainerLogView, theme: Theme) {
    let area = centered(92, 84, frame.area());
    frame.render_widget(Clear, area);
    let visible_height = area.height.saturating_sub(2) as usize;
    let max_start = logs.lines.len().saturating_sub(visible_height);
    let start = logs.offset.min(max_start);
    let text = if logs.lines.is_empty() {
        "No log output".into()
    } else {
        logs.lines
            .iter()
            .skip(start)
            .take(visible_height)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    };
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(panel(
            &format!(
                " Logs: {} · {}/{} · Enter/Esc close ",
                terminal_text(&logs.title, 128),
                start.saturating_add(1).min(logs.lines.len()),
                logs.lines.len()
            ),
            theme,
        )),
        area,
    );
}

fn render_process_details(
    frame: &mut Frame<'_>,
    process: &crate::domain::ProcessEntry,
    theme: Theme,
) {
    let area = centered(82, 58, frame.area());
    frame.render_widget(Clear, area);
    let text = format!(
        "PID: {}\nName: {}\nState: {}\nCPU: {:.2}%\nMemory: {}\nStarted: {}\n\nCommand:\n{}\n\nEnter/Esc: close",
        process.pid,
        terminal_text(&process.name, 256),
        terminal_text(&process.status, 128),
        process.cpu_percent,
        format_bytes(process.memory_bytes),
        process.start_time,
        terminal_text(&process.command, 4096),
    );
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(panel(" Process details ", theme)),
        area,
    );
}

fn render_too_small(frame: &mut Frame<'_>, app: &AppState, area: Rect) {
    frame.render_widget(
        Paragraph::new(format!(
            "{}\n{}x{}",
            tr(app.language, TextKey::TooSmall),
            area.width,
            area.height
        ))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_memory_gauge(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    used: u64,
    total: u64,
    theme: Theme,
) {
    let ratio = if total == 0 {
        0.0
    } else {
        used as f64 / total as f64
    };
    frame.render_widget(
        Gauge::default()
            .block(panel(&format!(" {title} "), theme))
            .gauge_style(Style::default().fg(usage_color(ratio * 100.0, theme)))
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!(
                "{} / {} · {:.1}%",
                format_bytes(used),
                format_bytes(total),
                ratio * 100.0
            )),
        area,
    );
}

fn page_name(app: &AppState, page: Page) -> &'static str {
    tr(
        app.language,
        match page {
            Page::Overview => TextKey::Overview,
            Page::Processes => TextKey::Processes,
            Page::Storage => TextKey::Storage,
            Page::Containers => TextKey::Containers,
            Page::Network => TextKey::Network,
            Page::Services => TextKey::Services,
            Page::Hardware => TextKey::Hardware,
        },
    )
}
fn panel(title: &str, theme: Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(title.to_owned())
        .border_style(Style::default().fg(theme.accent))
}
fn usage_color(percent: f64, theme: Theme) -> Color {
    if percent < 60.0 {
        theme.ok
    } else if percent < 85.0 {
        theme.warning
    } else {
        theme.error
    }
}
fn format_bytes(value: u64) -> String {
    const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    let mut size = value as f64;
    let mut index = 0;
    while size >= 1024.0 && index < UNITS.len() - 1 {
        size /= 1024.0;
        index += 1;
    }
    format!("{size:.1} {}", UNITS[index])
}
fn format_rate(value: f64) -> String {
    format!("{}/s", format_bytes(value.max(0.0) as u64))
}
fn format_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else {
        format!("{hours}h {minutes}m")
    }
}
fn percent(used: u64, total: u64) -> String {
    if total == 0 {
        "—".into()
    } else {
        format!("{:.1}%", used as f64 / total as f64 * 100.0)
    }
}
fn provider_available(app: &AppState, prefix: &str) -> &'static str {
    app.platform
        .data()
        .and_then(|data| {
            data.provider_reports
                .iter()
                .find(|report| report.provider.starts_with(prefix))
        })
        .map_or("probing", |report| {
            if report.state == crate::domain::ProbeState::Ready {
                "ready"
            } else {
                "unavailable"
            }
        })
}
fn groups_to_lines(groups: &[crate::domain::HardwareMetricGroup]) -> Vec<Line<'static>> {
    if groups.is_empty() {
        return vec![Line::from("No data")];
    }
    groups
        .iter()
        .flat_map(|group| {
            std::iter::once(Line::from(Span::styled(
                terminal_text(&group.label, 128),
                Style::default().add_modifier(Modifier::BOLD),
            )))
            .chain(metrics_to_lines(&group.metrics))
        })
        .collect()
}
fn metrics_to_lines(metrics: &[HardwareMetric]) -> Vec<Line<'static>> {
    if metrics.is_empty() {
        return vec![Line::from("No data")];
    }
    metrics
        .iter()
        .map(|metric| {
            Line::from(format!(
                "  {}: {}",
                terminal_text(&metric.label, 128),
                terminal_text(&metric.value, 256)
            ))
        })
        .collect()
}
fn centered(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cli::{ColorMode, Page},
        config::RuntimeConfig,
        domain::{
            ContainerSnapshot, CpuSnapshot, HardwareSnapshot, ModuleState, Snapshot, SystemSnapshot,
        },
        i18n::Language,
    };
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::Duration;

    #[test]
    fn binary_units_are_correct() {
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn ascii_history_uses_only_ascii_and_requested_size() {
        let history = (0..100).map(|value| value as f64).collect();
        let graph = ascii_history(&history, 24, 8);
        assert!(graph.is_ascii());
        assert_eq!(graph.lines().count(), 8);
        assert!(graph.lines().all(|line| line.len() <= 24));
        assert!(graph.contains('*'));

        let empty = ascii_history(&std::collections::VecDeque::new(), 24, 8);
        assert!(!empty.contains('*'));
    }

    #[test]
    fn chart_uses_a_windows_console_safe_marker() {
        if cfg!(target_os = "windows") {
            assert_eq!(chart_marker(), symbols::Marker::HalfBlock);
        } else {
            assert_eq!(chart_marker(), symbols::Marker::Braille);
        }
    }

    #[test]
    fn chart_window_is_human_readable() {
        assert_eq!(format_chart_window(Duration::from_millis(49_500)), "50s");
        assert_eq!(format_chart_window(Duration::from_secs(90)), "1.5m");
    }

    #[test]
    fn all_pages_languages_and_breakpoints_render() {
        let languages = [
            Language::English,
            Language::Ukrainian,
            Language::German,
            Language::French,
            Language::Spanish,
            Language::Russian,
        ];
        let sizes = [(79, 23), (80, 24), (110, 30), (131, 44), (160, 50)];

        for language in languages {
            for page in Page::ALL {
                for (width, height) in sizes {
                    let mut app = fixture(language, page);
                    let backend = TestBackend::new(width, height);
                    let mut terminal = Terminal::new(backend).expect("test terminal");
                    terminal
                        .draw(|frame| render(frame, &app))
                        .expect("render succeeds");
                    let populated = terminal
                        .backend()
                        .buffer()
                        .content
                        .iter()
                        .any(|cell| cell.symbol() != " ");
                    assert!(
                        populated,
                        "empty render for {language:?}/{page:?}/{width}x{height}"
                    );

                    app.config.ascii = true;
                    app.config.color = ColorMode::Never;
                    terminal
                        .draw(|frame| render(frame, &app))
                        .expect("ASCII no-color render succeeds");
                }
            }
        }
    }

    fn fixture(language: Language, page: Page) -> AppState {
        let config = RuntimeConfig {
            language,
            interval: Duration::from_millis(500),
            default_page: page,
            color: ColorMode::Auto,
            ascii: false,
            show_unsupported_modules: true,
            container_endpoints: vec![],
            service_provider: "auto".into(),
            config_source: None,
        };
        let mut app = AppState::new(config);
        app.page = page;
        app.cpu_history = (0..100)
            .map(|sample| ((sample as f64 / 8.0).sin() * 35.0 + 45.0).clamp(0.0, 100.0))
            .collect();
        app.memory_history = (0..100).map(|sample| 42.0 + sample as f64 / 10.0).collect();
        app.system = ModuleState::Ready {
            snapshot: Snapshot::now(SystemSnapshot {
                host_name: "test-host".into(),
                os_name: "KernOX".into(),
                kernel_version: "test".into(),
                uptime_seconds: 3_661,
                load_average: Some([0.1, 0.2, 0.3]),
                cpu: CpuSnapshot {
                    total_percent: 12.5,
                    per_core_percent: vec![10.0, 15.0],
                    frequency_mhz: 2_400,
                },
                memory_used: 4 * 1024 * 1024 * 1024,
                memory_total: 8 * 1024 * 1024 * 1024,
                swap_used: 0,
                swap_total: 2 * 1024 * 1024 * 1024,
                processes: vec![],
                storage: vec![],
                networks: vec![],
            }),
        };
        app.containers = ModuleState::Ready {
            snapshot: Snapshot::now(ContainerSnapshot::default()),
        };
        app.platform = ModuleState::Ready {
            snapshot: Snapshot::now(HardwareSnapshot::default()),
        };
        app
    }
}
