use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::{collections::VecDeque, io, time::Duration};
use tokio_util::sync::CancellationToken;

use crate::{
    cli::Page,
    config::RuntimeConfig,
    domain::{ContainerSnapshot, HardwareSnapshot, ModuleState, ProcessEntry, SystemSnapshot},
    i18n::Language,
    providers::{
        platform::{self, ServiceActionKind},
        system::SystemAction,
    },
    supervisor::{RuntimeEvent, Supervisor},
    terminal::TerminalSession,
    ui,
};

#[cfg(feature = "containers")]
use crate::providers::containers::ContainerAction;

#[derive(Debug, Clone)]
pub enum PendingAction {
    Process {
        pid: u32,
        start_time: u64,
        name: String,
        force: bool,
    },
    #[cfg(feature = "containers")]
    Container {
        engine: String,
        id: String,
        name: String,
        kind: ContainerActionKind,
    },
    Service {
        manager: String,
        id: String,
        name: String,
        kind: ServiceActionKind,
    },
}

#[cfg(feature = "containers")]
#[derive(Debug, Clone, Copy)]
pub enum ContainerActionKind {
    Stop,
    Restart,
}

#[derive(Debug, Clone, Copy)]
pub enum ProcessSort {
    Cpu,
    Memory,
    Pid,
    Name,
}

pub(crate) const HISTORY_SAMPLES: usize = 100;

#[cfg(feature = "containers")]
pub struct ContainerLogView {
    pub title: String,
    pub lines: Vec<String>,
    pub offset: usize,
}

pub struct AppState {
    pub config: RuntimeConfig,
    pub language: Language,
    pub page: Page,
    pub pages: Vec<Page>,
    pub system: ModuleState<SystemSnapshot>,
    pub containers: ModuleState<ContainerSnapshot>,
    pub platform: ModuleState<HardwareSnapshot>,
    pub cpu_history: VecDeque<f64>,
    pub memory_history: VecDeque<f64>,
    pub process_selection: usize,
    pub storage_selection: usize,
    pub container_selection: usize,
    pub network_selection: usize,
    pub service_selection: usize,
    pub search: String,
    pub search_mode: bool,
    pub process_sort: ProcessSort,
    pub process_details: Option<ProcessEntry>,
    pub show_help: bool,
    pub pending: Option<PendingAction>,
    pub status: Option<(bool, String)>,
    #[cfg(feature = "containers")]
    pub container_logs: Option<ContainerLogView>,
    pub should_quit: bool,
}

impl AppState {
    pub fn new(config: RuntimeConfig) -> Self {
        let pages = Page::ALL
            .into_iter()
            .filter(|page| {
                !page.linux_only() || cfg!(target_os = "linux") || config.show_unsupported_modules
            })
            .collect::<Vec<_>>();
        let page = if pages.contains(&config.default_page) {
            config.default_page
        } else {
            Page::Overview
        };
        Self {
            language: config.language,
            page,
            pages,
            config,
            system: ModuleState::Loading,
            containers: ModuleState::Loading,
            platform: ModuleState::Loading,
            cpu_history: VecDeque::with_capacity(HISTORY_SAMPLES),
            memory_history: VecDeque::with_capacity(HISTORY_SAMPLES),
            process_selection: 0,
            storage_selection: 0,
            container_selection: 0,
            network_selection: 0,
            service_selection: 0,
            search: String::new(),
            search_mode: false,
            process_sort: ProcessSort::Cpu,
            process_details: None,
            show_help: false,
            pending: None,
            status: None,
            #[cfg(feature = "containers")]
            container_logs: None,
            should_quit: false,
        }
    }

    pub fn filtered_processes(&self) -> Vec<&ProcessEntry> {
        let query = self.search.to_lowercase();
        let mut processes: Vec<&ProcessEntry> = self
            .system
            .data()
            .map(|snapshot| {
                snapshot
                    .processes
                    .iter()
                    .filter(|process| {
                        query.is_empty()
                            || process.name.to_lowercase().contains(&query)
                            || process.pid.to_string().contains(&query)
                    })
                    .collect()
            })
            .unwrap_or_default();
        match self.process_sort {
            ProcessSort::Cpu => processes.sort_by(|a, b| {
                b.cpu_percent
                    .total_cmp(&a.cpu_percent)
                    .then_with(|| a.pid.cmp(&b.pid))
            }),
            ProcessSort::Memory => processes.sort_by(|a, b| {
                b.memory_bytes
                    .cmp(&a.memory_bytes)
                    .then_with(|| a.pid.cmp(&b.pid))
            }),
            ProcessSort::Pid => processes.sort_by_key(|process| process.pid),
            ProcessSort::Name => processes.sort_by(|a, b| {
                a.name
                    .to_ascii_lowercase()
                    .cmp(&b.name.to_ascii_lowercase())
                    .then_with(|| a.pid.cmp(&b.pid))
            }),
        }
        processes
    }

    fn move_page(&mut self, delta: isize) {
        let current = self
            .pages
            .iter()
            .position(|page| page == &self.page)
            .unwrap_or(0) as isize;
        let count = self.pages.len() as isize;
        self.page = self.pages[(current + delta).rem_euclid(count) as usize];
    }

    fn clamp_selections(&mut self) {
        let process_len = self.filtered_processes().len();
        self.process_selection = clamp(self.process_selection, process_len);
        if let Some(system) = self.system.data() {
            self.storage_selection = clamp(self.storage_selection, system.storage.len());
            self.network_selection = clamp(self.network_selection, system.networks.len());
        }
        if let Some(containers) = self.containers.data() {
            self.container_selection = clamp(self.container_selection, containers.containers.len());
        }
        if let Some(platform) = self.platform.data() {
            self.service_selection = clamp(self.service_selection, platform.services.len());
        }
    }

    fn selection_mut(&mut self) -> &mut usize {
        match self.page {
            Page::Processes => &mut self.process_selection,
            Page::Storage => &mut self.storage_selection,
            Page::Containers => &mut self.container_selection,
            Page::Network => &mut self.network_selection,
            Page::Services => &mut self.service_selection,
            _ => &mut self.process_selection,
        }
    }

    fn record_system_history(&mut self, snapshot: &SystemSnapshot) {
        push_history(&mut self.cpu_history, snapshot.cpu.total_percent as f64);
        let memory_percent = if snapshot.memory_total == 0 {
            0.0
        } else {
            snapshot.memory_used as f64 / snapshot.memory_total as f64 * 100.0
        };
        push_history(&mut self.memory_history, memory_percent);
    }
}

fn clamp(selection: usize, len: usize) -> usize {
    if len == 0 { 0 } else { selection.min(len - 1) }
}

fn push_history(history: &mut VecDeque<f64>, value: f64) {
    history.push_back(if value.is_finite() {
        value.clamp(0.0, 100.0)
    } else {
        0.0
    });
    while history.len() > HISTORY_SAMPLES {
        history.pop_front();
    }
}

pub async fn run_tui(config: RuntimeConfig) -> anyhow::Result<()> {
    let mut terminal = TerminalSession::new()?;
    let mut supervisor = Supervisor::start(&config);
    let mut app = AppState::new(config);
    let shutdown = install_signal_handlers();

    while !app.should_quit && !shutdown.is_cancelled() {
        while let Ok(runtime_event) = supervisor.events.try_recv() {
            apply_runtime_event(&mut app, runtime_event);
        }
        app.clamp_selections();
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(&mut app, &supervisor, &mut terminal, key).await
                }
                Event::Resize(_, _) => terminal.clear()?,
                _ => {}
            }
        }
    }
    Ok(())
}

fn install_signal_handlers() -> CancellationToken {
    let shutdown = CancellationToken::new();
    let ctrl_c = shutdown.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            ctrl_c.cancel();
        }
    });

    #[cfg(target_os = "linux")]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let terminate = shutdown.clone();
        tokio::spawn(async move {
            if let Ok(mut signal) = signal(SignalKind::terminate()) {
                signal.recv().await;
                terminate.cancel();
            }
        });
    }
    shutdown
}

fn apply_runtime_event(app: &mut AppState, runtime_event: RuntimeEvent) {
    match runtime_event {
        RuntimeEvent::System(state) => {
            if let Some(snapshot) = state.data() {
                app.record_system_history(snapshot);
            }
            app.system = state;
        }
        RuntimeEvent::Containers(state) => app.containers = state,
        RuntimeEvent::Platform(state) => app.platform = state,
        #[cfg(feature = "containers")]
        RuntimeEvent::ContainerLogs { title, lines } => {
            let offset = lines.len();
            app.container_logs = Some(ContainerLogView {
                title,
                lines,
                offset,
            });
        }
        RuntimeEvent::ActionResult(result) => {
            app.status = Some(match result {
                Ok(message) => (true, message),
                Err(message) => (false, message),
            })
        }
    }
}

async fn handle_key(
    app: &mut AppState,
    supervisor: &Supervisor,
    terminal: &mut TerminalSession,
    key: KeyEvent,
) {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.should_quit = true;
        return;
    }
    #[cfg(feature = "containers")]
    if app.container_logs.is_some() {
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
            app.container_logs = None;
            return;
        }
        let logs = app.container_logs.as_mut().expect("checked above");
        match key.code {
            KeyCode::Up => logs.offset = logs.offset.saturating_sub(1),
            KeyCode::Down => logs.offset = logs.offset.saturating_add(1).min(logs.lines.len()),
            KeyCode::PageUp => logs.offset = logs.offset.saturating_sub(20),
            KeyCode::PageDown => {
                logs.offset = logs.offset.saturating_add(20).min(logs.lines.len());
            }
            KeyCode::Home => logs.offset = 0,
            KeyCode::End => logs.offset = logs.lines.len().saturating_sub(1),
            _ => {}
        }
        return;
    }
    if app.pending.is_some() {
        handle_confirmation(app, supervisor, terminal, key).await;
        return;
    }
    if app.process_details.is_some() {
        if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
            app.process_details = None;
        }
        return;
    }
    if app.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1)) {
            app.show_help = false;
        }
        return;
    }
    if app.search_mode {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => app.search_mode = false,
            KeyCode::Backspace => {
                app.search.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.search.clear()
            }
            KeyCode::Char(ch) => app.search.push(ch),
            _ => {}
        }
        app.process_selection = 0;
        return;
    }

    match key.code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Esc => app.status = None,
        KeyCode::Tab => app.move_page(1),
        KeyCode::BackTab => app.move_page(-1),
        KeyCode::Char('l') => app.language = app.language.next_visible(),
        KeyCode::Char('?') | KeyCode::F(1) => app.show_help = true,
        KeyCode::F(5) => {
            let _ = supervisor.system_actions.try_send(SystemAction::Refresh);
            #[cfg(feature = "containers")]
            let _ = supervisor
                .container_actions
                .try_send(ContainerAction::Refresh);
            app.status = Some((true, "Refresh scheduled".into()));
        }
        KeyCode::Char(ch @ '1'..='7') => {
            let page = Page::ALL[(ch as usize) - ('1' as usize)];
            if app.pages.contains(&page) {
                app.page = page;
            } else {
                app.status = Some((
                    false,
                    "This page is unavailable on the current platform".into(),
                ));
            }
        }
        KeyCode::Up => {
            let selected = app.selection_mut();
            *selected = selected.saturating_sub(1);
        }
        KeyCode::Down => {
            let selected = app.selection_mut();
            *selected = selected.saturating_add(1);
        }
        KeyCode::PageUp => {
            let selected = app.selection_mut();
            *selected = selected.saturating_sub(10);
        }
        KeyCode::PageDown => {
            let selected = app.selection_mut();
            *selected = selected.saturating_add(10);
        }
        KeyCode::Home => *app.selection_mut() = 0,
        KeyCode::Char('/') if app.page == Page::Processes => app.search_mode = true,
        KeyCode::Enter if app.page == Page::Processes => show_process_details(app),
        KeyCode::Char('c') if app.page == Page::Processes => {
            app.process_sort = ProcessSort::Cpu;
            app.process_selection = 0;
        }
        KeyCode::Char('m') if app.page == Page::Processes => {
            app.process_sort = ProcessSort::Memory;
            app.process_selection = 0;
        }
        KeyCode::Char('p') if app.page == Page::Processes => {
            app.process_sort = ProcessSort::Pid;
            app.process_selection = 0;
        }
        KeyCode::Char('n') if app.page == Page::Processes => {
            app.process_sort = ProcessSort::Name;
            app.process_selection = 0;
        }
        KeyCode::Char('k') if app.page == Page::Processes => {
            prepare_process_action(app, key.modifiers.contains(KeyModifiers::SHIFT))
        }
        #[cfg(feature = "containers")]
        KeyCode::Char('t') if app.page == Page::Containers => send_container_start(app, supervisor),
        #[cfg(feature = "containers")]
        KeyCode::Enter if app.page == Page::Containers => send_container_logs(app, supervisor),
        #[cfg(feature = "containers")]
        KeyCode::Char('s') if app.page == Page::Containers => {
            prepare_container_action(app, ContainerActionKind::Stop)
        }
        #[cfg(feature = "containers")]
        KeyCode::Char('r') if app.page == Page::Containers => {
            prepare_container_action(app, ContainerActionKind::Restart)
        }
        KeyCode::Char('t') if app.page == Page::Services => {
            prepare_service_action(app, ServiceActionKind::Start);
        }
        KeyCode::Char('s') if app.page == Page::Services => {
            prepare_service_action(app, ServiceActionKind::Stop);
        }
        KeyCode::Char('r') if app.page == Page::Services => {
            prepare_service_action(app, ServiceActionKind::Restart);
        }
        _ => {}
    }
}

fn prepare_process_action(app: &mut AppState, force: bool) {
    let selected = app
        .filtered_processes()
        .get(app.process_selection)
        .cloned()
        .cloned();
    if let Some(process) = selected {
        app.pending = Some(PendingAction::Process {
            pid: process.pid,
            start_time: process.start_time,
            name: process.name,
            force,
        });
    }
}

fn show_process_details(app: &mut AppState) {
    app.process_details = app
        .filtered_processes()
        .get(app.process_selection)
        .copied()
        .cloned();
}

#[cfg(feature = "containers")]
fn selected_container(app: &AppState) -> Option<crate::domain::ContainerEntry> {
    app.containers
        .data()?
        .containers
        .get(app.container_selection)
        .cloned()
}

#[cfg(feature = "containers")]
fn prepare_container_action(app: &mut AppState, kind: ContainerActionKind) {
    if let Some(container) = selected_container(app) {
        app.pending = Some(PendingAction::Container {
            engine: container.engine,
            id: container.id,
            name: container.name,
            kind,
        });
    }
}

#[cfg(feature = "containers")]
fn send_container_start(app: &mut AppState, supervisor: &Supervisor) {
    if let Some(container) = selected_container(app)
        && supervisor
            .container_actions
            .try_send(ContainerAction::Start {
                engine: container.engine,
                id: container.id,
            })
            .is_err()
    {
        app.status = Some((false, "Container action queue is busy".into()));
    }
}

#[cfg(feature = "containers")]
fn send_container_logs(app: &mut AppState, supervisor: &Supervisor) {
    if let Some(container) = selected_container(app) {
        if supervisor
            .container_actions
            .try_send(ContainerAction::Logs {
                engine: container.engine,
                id: container.id,
                name: container.name,
            })
            .is_err()
        {
            app.status = Some((false, "Container action queue is busy".into()));
        } else {
            app.status = Some((true, "Loading container logs".into()));
        }
    }
}

fn prepare_service_action(app: &mut AppState, kind: ServiceActionKind) {
    let Some(platform) = app.platform.data() else {
        return;
    };
    let Some(service) = platform.services.get(app.service_selection) else {
        return;
    };
    let manager = platform
        .provider_reports
        .iter()
        .find(|report| report.provider.starts_with("services:"))
        .map(|report| report.provider.trim_start_matches("services:").to_owned())
        .unwrap_or_else(|| "unknown".into());
    app.pending = Some(PendingAction::Service {
        manager,
        id: service.id.clone(),
        name: service.name.clone(),
        kind,
    });
}

async fn handle_confirmation(
    app: &mut AppState,
    supervisor: &Supervisor,
    terminal: &mut TerminalSession,
    key: KeyEvent,
) {
    match key.code {
        KeyCode::Char('y') => {
            if let Some(action) = app.pending.take() {
                match action {
                    PendingAction::Process {
                        pid,
                        start_time,
                        force,
                        ..
                    } => {
                        let _ = supervisor.system_actions.try_send(SystemAction::Terminate {
                            pid,
                            start_time,
                            force,
                        });
                    }
                    #[cfg(feature = "containers")]
                    PendingAction::Container {
                        engine, id, kind, ..
                    } => {
                        let command = match kind {
                            ContainerActionKind::Stop => ContainerAction::Stop { engine, id },
                            ContainerActionKind::Restart => ContainerAction::Restart { engine, id },
                        };
                        let _ = supervisor.container_actions.try_send(command);
                    }
                    PendingAction::Service {
                        manager, id, kind, ..
                    } => {
                        let result = terminal.suspend(|| {
                            platform::control_service(&manager, &id, kind).map_err(io::Error::other)
                        });
                        app.status = Some(match result {
                            Ok(message) => (true, message),
                            Err(error) => (false, error.to_string()),
                        });
                    }
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Esc | KeyCode::Char('q') => app.pending = None,
        _ => {}
    }
}

impl From<io::Error> for crate::domain::ProviderError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded_and_sanitizes_percentages() {
        let mut history = VecDeque::new();
        for value in 0..150 {
            push_history(&mut history, value as f64);
        }
        assert_eq!(history.len(), HISTORY_SAMPLES);
        assert_eq!(history.front(), Some(&50.0));
        assert_eq!(history.back(), Some(&100.0));

        push_history(&mut history, f64::NEG_INFINITY);
        assert_eq!(history.back(), Some(&0.0));
    }
}
