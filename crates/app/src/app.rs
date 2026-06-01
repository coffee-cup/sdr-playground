use std::sync::Arc;
use std::time::Duration;

use gpui::*;
use gpui_component::ActiveTheme;
use sdr_engine::{Engine, EngineConfig, RtlConfig, RtlSdrSource};

use crate::components::{title_bar, transport_bar};
use crate::nav::{self, Workspace};
use crate::radio::RadioState;
use crate::signal::Waterfall;
use crate::workspaces;

/// How often the display polls the engine taps and repaints (~30 Hz, the waterfall row rate).
const TICK_INTERVAL: Duration = Duration::from_millis(33);

/// Root view. Owns the active workspace, the radio connection, and the live display state, and
/// lays out the persistent frame: the integrated title bar, then a body of [nav rail | active
/// workspace], then the transport bar pinned along the bottom.
///
/// The frame arrangement is described in `docs/UI.md`.
pub struct SdrApp {
    active: Workspace,
    radio: RadioState,
    paused: bool,
    waterfall: Waterfall,
    /// Drives the ~30 Hz poll/repaint while a device is running; dropped to stop it.
    tick_task: Option<Task<()>>,
}

impl SdrApp {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            active: Workspace::Listen,
            radio: RadioState::Connecting,
            paused: false,
            waterfall: Waterfall::new(),
            tick_task: None,
        }
    }

    pub fn active(&self) -> Workspace {
        self.active
    }

    pub fn radio(&self) -> &RadioState {
        &self.radio
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn waterfall(&self) -> &Waterfall {
        &self.waterfall
    }

    pub fn activate(&mut self, workspace: Workspace, cx: &mut Context<Self>) {
        if self.active != workspace {
            self.active = workspace;
            cx.notify();
        }
    }

    pub fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        if self.radio.engine().is_some() {
            self.paused = !self.paused;
            cx.notify();
        }
    }

    /// Open the first available RTL-SDR and start the engine. Enumeration and the blocking USB
    /// `open` run off the foreground; the constructed engine is handed back to the view. Safe to
    /// call again to retry — it resets the display and replaces any running engine.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.radio = RadioState::Connecting;
        self.paused = false;
        self.waterfall = Waterfall::new();
        self.tick_task = None;
        cx.notify();

        let open = cx.background_executor().spawn(async move {
            let devices = RtlSdrSource::list().map_err(|e| e.to_string())?;
            let Some(device) = devices.into_iter().next() else {
                return Ok::<Option<Engine>, String>(None);
            };
            let source = RtlSdrSource::open(device.index, RtlConfig::default())
                .map_err(|e| e.to_string())?;
            Ok(Some(Engine::start(
                Box::new(source),
                EngineConfig::default(),
            )))
        });

        cx.spawn(async move |this, cx| {
            let result = open.await;
            this.update(cx, |app, cx| {
                match result {
                    Ok(Some(engine)) => {
                        app.radio = RadioState::Running(Arc::new(engine));
                        app.start_tick_loop(cx);
                    }
                    Ok(None) => app.radio = RadioState::NoDevice,
                    Err(e) => app.radio = RadioState::Failed(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_tick_loop(&mut self, cx: &mut Context<Self>) {
        self.tick_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(TICK_INTERVAL).await;
            let alive = this
                .update_in(cx, |app, window, cx| {
                    let Some(engine) = app.radio.engine().cloned() else {
                        return;
                    };
                    if app.paused {
                        return;
                    }
                    // Fold the newest spectrum into the waterfall; release the old GPU tile.
                    let frame = engine.spectrum();
                    if let Some(old) = app.waterfall.push(&frame) {
                        let _ = window.drop_image(old);
                    }
                    cx.notify();
                })
                .is_ok();
            if !alive {
                break;
            }
        }));
    }
}

impl Render for SdrApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;

        let content = match self.active {
            Workspace::Listen => workspaces::listen::render(self, cx).into_any_element(),
            Workspace::Library => workspaces::library::render(cx).into_any_element(),
            Workspace::Recordings => workspaces::recordings::render(cx).into_any_element(),
            Workspace::Settings => workspaces::settings::render(cx).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .text_size(px(13.))
            .bg(background)
            .text_color(foreground)
            .child(title_bar::render(self, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(nav::render(self.active, cx))
                    .child(div().flex().flex_1().overflow_hidden().child(content)),
            )
            .child(transport_bar::render(self, cx))
    }
}
