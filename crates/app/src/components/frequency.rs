//! The frequency selector: a large fixed-width decimal readout that is also the primary tuning
//! control. Each digit is independently editable. Hovering a digit focuses the row and arms it;
//! scrolling the wheel over a digit steps that decimal place (with carry), and typing a number
//! writes it into the armed place and advances to the next, so entering a frequency reads like
//! typing it. All edits flow through [`SdrApp`] so they retune live and persist.

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::{SdrApp, FREQ_DIGITS};

/// Render the selector for the current center frequency.
pub fn render(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let dim = muted.opacity(0.45);
    let accent = cx.theme().primary;
    let mono = cx.theme().mono_font_family.clone();

    let freq = app.tuned_freq();
    let hovered = app.hovered_digit();
    // Most significant nonzero place, so leading zeros render dimmed.
    let leading = (0..FREQ_DIGITS)
        .rev()
        .find(|p| !(freq / 10u64.pow(*p)).is_multiple_of(10))
        .unwrap_or(0);

    let mut row = div()
        .id("freq-selector")
        .track_focus(app.freq_focus())
        .flex()
        .flex_row()
        .items_center()
        .font_family(mono)
        .text_size(px(34.))
        .line_height(px(40.))
        .on_key_down(cx.listener(|app, ev: &KeyDownEvent, _window, cx| {
            match ev.keystroke.key.as_str() {
                d @ ("0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
                    app.type_digit(d.parse().unwrap(), cx);
                }
                "up" => {
                    if let Some(p) = app.hovered_digit() {
                        app.nudge_digit(p, 1, cx);
                    }
                }
                "down" => {
                    if let Some(p) = app.hovered_digit() {
                        app.nudge_digit(p, -1, cx);
                    }
                }
                "left" => {
                    if let Some(p) = app.hovered_digit() {
                        app.set_hovered_digit(Some((p + 1).min(FREQ_DIGITS - 1)), cx);
                    }
                }
                "right" => {
                    if let Some(p) = app.hovered_digit() {
                        app.set_hovered_digit(Some(p.saturating_sub(1)), cx);
                    }
                }
                _ => {}
            }
        }));

    for place in (0..FREQ_DIGITS).rev() {
        let digit = ((freq / 10u64.pow(place)) % 10) as u32;
        let is_hovered = hovered == Some(place);
        let color = if place > leading { dim } else { foreground };

        row = row.child(
            div()
                .id(("digit", place as usize))
                .px(px(1.))
                .cursor_pointer()
                .when(is_hovered, |d| {
                    d.text_color(accent).border_b_2().border_color(accent)
                })
                .when(!is_hovered, |d| {
                    d.text_color(color)
                        .border_b_2()
                        .border_color(transparent_black())
                })
                .child(digit.to_string())
                .on_hover(cx.listener(move |app, hovered: &bool, window, cx| {
                    if *hovered {
                        app.set_hovered_digit(Some(place), cx);
                        app.freq_focus().focus(window, cx);
                    }
                }))
                .on_scroll_wheel(cx.listener(move |app, ev: &ScrollWheelEvent, _window, cx| {
                    let dy = match ev.delta {
                        ScrollDelta::Pixels(p) => f32::from(p.y),
                        ScrollDelta::Lines(l) => l.y,
                    };
                    if dy != 0.0 {
                        app.nudge_digit(place, if dy > 0.0 { 1 } else { -1 }, cx);
                    }
                })),
        );

        // Thousands separators: between the GHz/MHz/kHz/Hz groups.
        if place == 9 || place == 6 || place == 3 {
            row = row.child(div().text_color(muted).px(px(1.)).child("."));
        }
    }

    row
}
