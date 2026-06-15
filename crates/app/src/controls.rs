//! The settings-panel dropdowns, grouped so `SdrApp` holds one `controls` field instead of a
//! widget entity per setting. `SettingsControls::new` builds each dropdown from the loaded
//! settings and wires its confirm-subscription to the matching `SdrApp` setter, keeping all of
//! that boilerplate in one place.

use gpui::*;
use gpui_component::select::{SearchableVec, SelectEvent, SelectState};

use crate::app::SdrApp;
use crate::colormap::Colormap;
use crate::settings::{
    bandwidth_label, Settings, WindowChoice, AVG_PRESETS, BW_PRESETS, FFT_SIZES, FPS_CHOICES,
};
use crate::ui::DropdownState;

/// The analysis windows offered in the panel.
const WINDOW_CHOICES: [WindowChoice; 2] = [WindowChoice::Hann, WindowChoice::Rectangular];

pub struct SettingsControls {
    pub fft: DropdownState<usize>,
    pub window: DropdownState<WindowChoice>,
    pub colormap: DropdownState<Colormap>,
    pub fps: DropdownState<f32>,
    pub averaging: DropdownState<f32>,
    pub bandwidth: DropdownState<u32>,
}

impl SettingsControls {
    pub fn new(settings: &Settings, window: &mut Window, cx: &mut Context<SdrApp>) -> Self {
        let fft = DropdownState::new(
            FFT_SIZES
                .iter()
                .map(|&n| (n.to_string().into(), n))
                .collect(),
            index_of(&FFT_SIZES, &settings.fft_size),
            window,
            cx,
        );
        let window_dd = DropdownState::new(
            WINDOW_CHOICES
                .iter()
                .map(|&w| (w.label().into(), w))
                .collect(),
            index_of(&WINDOW_CHOICES, &settings.window),
            window,
            cx,
        );
        let colormap = DropdownState::new(
            Colormap::ALL
                .iter()
                .map(|&c| (c.label().into(), c))
                .collect(),
            index_of(&Colormap::ALL, &settings.colormap),
            window,
            cx,
        );
        let fps = DropdownState::new(
            FPS_CHOICES
                .iter()
                .map(|&f| (format!("{f:.0} fps").into(), f))
                .collect(),
            FPS_CHOICES
                .iter()
                .position(|&f| f == settings.fps)
                .unwrap_or(0),
            window,
            cx,
        );
        let averaging = DropdownState::new(
            AVG_PRESETS.iter().map(|&(v, l)| (l.into(), v)).collect(),
            AVG_PRESETS
                .iter()
                .position(|&(v, _)| (v - settings.averaging).abs() < 0.01)
                .unwrap_or(0),
            window,
            cx,
        );
        let bandwidth = DropdownState::new(
            BW_PRESETS
                .iter()
                .map(|&b| (bandwidth_label(b).into(), b))
                .collect(),
            index_of(&BW_PRESETS, &settings.bandwidth),
            window,
            cx,
        );

        // Each dropdown's confirmed choice flows back to the matching setter.
        cx.subscribe(
            &fft.state,
            on_pick(|a| &a.controls().fft, SdrApp::set_fft_size),
        )
        .detach();
        cx.subscribe(
            &window_dd.state,
            on_pick(|a| &a.controls().window, SdrApp::set_window),
        )
        .detach();
        cx.subscribe(
            &colormap.state,
            on_pick(|a| &a.controls().colormap, SdrApp::set_colormap),
        )
        .detach();
        cx.subscribe(&fps.state, on_pick(|a| &a.controls().fps, SdrApp::set_fps))
            .detach();
        cx.subscribe(
            &averaging.state,
            on_pick(|a| &a.controls().averaging, SdrApp::set_averaging),
        )
        .detach();
        cx.subscribe(
            &bandwidth.state,
            on_pick(|a| &a.controls().bandwidth, SdrApp::set_bandwidth),
        )
        .detach();

        Self {
            fft,
            window: window_dd,
            colormap,
            fps,
            averaging,
            bandwidth,
        }
    }
}

/// Index of `v` in `slice`, or 0 if absent (used to pre-select a dropdown).
fn index_of<T: PartialEq>(slice: &[T], v: &T) -> usize {
    slice.iter().position(|x| x == v).unwrap_or(0)
}

/// A subscription handler mapping a dropdown's confirmed label to its value, then applying it via
/// `set`. `pick` selects which dropdown to read the value table from.
#[allow(clippy::type_complexity)] // the gpui subscription handler signature is inherently nested
fn on_pick<T: Clone + 'static>(
    pick: impl Fn(&SdrApp) -> &DropdownState<T> + 'static,
    set: impl Fn(&mut SdrApp, T, &mut Context<SdrApp>) + 'static,
) -> impl FnMut(
    &mut SdrApp,
    Entity<SelectState<SearchableVec<SharedString>>>,
    &SelectEvent<SearchableVec<SharedString>>,
    &mut Context<SdrApp>,
) {
    move |app, _emitter, ev, cx| {
        if let SelectEvent::Confirm(Some(label)) = ev {
            if let Some(value) = pick(app).value_for(label) {
                set(app, value, cx);
            }
        }
    }
}
