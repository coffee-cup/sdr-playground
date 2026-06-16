//! A flat arc knob, the signature device control: a value arc in the accent color over a track
//! arc, plus a pointer, with a label and value beneath. Display-only — the caller wires
//! interaction (e.g. scroll) on a wrapping element. Drawn on a GPUI canvas like `signal.rs`.

use gpui::{
    canvas, div, point, px, App, Hsla, IntoElement, ParentElement, PathBuilder, Styled, Window,
};
use gpui_component::ActiveTheme;

use crate::ui::{palette, tokens};

const SIZE: f32 = 44.0;
/// The dial's gap is centered at the bottom: the arc sweeps 290° from the lower-left.
const START_DEG: f32 = 215.0;
const SWEEP_DEG: f32 = 290.0;

/// A knob showing `value` (0..1) with `display` text and a `label`. `accent` colors the value arc.
pub fn knob(
    label: &str,
    value: f32,
    display: impl Into<gpui::SharedString>,
    accent: Hsla,
    cx: &App,
) -> impl IntoElement {
    let track = palette(cx).knob_track;
    let pointer = palette(cx).pointer;
    let v = value.clamp(0.0, 1.0);

    let dial = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let ox = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
            let oy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
            let r = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0 - 3.0;

            arc(window, ox, oy, r, START_DEG, START_DEG + SWEEP_DEG, track);
            if v > 0.0 {
                arc(
                    window,
                    ox,
                    oy,
                    r,
                    START_DEG,
                    START_DEG + v * SWEEP_DEG,
                    accent,
                );
            }
            // Pointer at the value end-angle.
            let a = (START_DEG + v * SWEEP_DEG).to_radians();
            let (sx, sy) = (a.sin(), -a.cos());
            let mut b = PathBuilder::stroke(px(2.0));
            b.move_to(point(px(ox + sx * (r - 10.0)), px(oy + sy * (r - 10.0))));
            b.line_to(point(px(ox + sx * (r - 2.0)), px(oy + sy * (r - 2.0))));
            if let Ok(p) = b.build() {
                window.paint_path(p, pointer);
            }
        },
    );

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(5.))
        .child(div().size(px(SIZE)).child(dial.size_full()))
        .child(
            div()
                .text_size(tokens::TEXT_SM)
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(tokens::TEXT_SM)
                .text_color(cx.theme().foreground)
                .child(display.into()),
        )
}

/// Stroke a circular arc (polyline) from `a0` to `a1` degrees, 0° = up, clockwise.
fn arc(window: &mut Window, ox: f32, oy: f32, r: f32, a0: f32, a1: f32, color: Hsla) {
    const STEPS: usize = 48;
    let mut b = PathBuilder::stroke(px(3.0));
    for i in 0..=STEPS {
        let a = (a0 + (a1 - a0) * (i as f32 / STEPS as f32)).to_radians();
        let x = px(ox + a.sin() * r);
        let y = px(oy - a.cos() * r);
        if i == 0 {
            b.move_to(point(x, y));
        } else {
            b.line_to(point(x, y));
        }
    }
    if let Ok(p) = b.build() {
        window.paint_path(p, color);
    }
}
