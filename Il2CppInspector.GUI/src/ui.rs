//! Palette and controls shared by the columns: dark translucent surfaces, one accent blue and 28 px controls.

use std::time::Duration;

use gpui::{prelude::*, *};

pub const FONT: &str = "Segoe UI";
pub const CONTROL_HEIGHT: f32 = 28.;

pub const TEXT: u32 = 0xf5f5f7f0;
pub const SECONDARY: u32 = 0xebebeba3;
pub const TERTIARY: u32 = 0xdcdcdc6b;
pub const SEPARATOR: u32 = 0xffffff1a;
pub const FILL: u32 = 0xffffff12;
pub const WHITE: u32 = 0xffffffff;
pub const ON_ACCENT_SECONDARY: u32 = 0xffffffc7;
pub const ACCENT: u32 = 0x0a84ffff;
pub const ACCENT_HOVER: u32 = 0x409cffff;
pub const ACCENT_TEXT: u32 = 0x8dbdffff;
pub const ACCENT_SOFT: u32 = 0x0a84ff29;
pub const SELECTION: u32 = 0x0a84ff59;
pub const DANGER: u32 = 0xff453aff;
pub const DANGER_TEXT: u32 = 0xff8a82ff;
pub const SUCCESS: u32 = 0x30d158ff;
pub const WARNING: u32 = 0xff9f0aff;
pub const FIELD: u32 = 0x00000038;
pub const FIELD_BORDER: u32 = 0xffffff1a;
pub const BACKGROUND_TOP: u32 = 0x1d1d1dff;
pub const BACKGROUND_BOTTOM: u32 = 0x181818ff;
pub const LAUNCHER_TINT: u32 = 0xffffff05;
pub const TABLE: u32 = 0x00000029;
pub const TABLE_HEADER: u32 = 0x1b1b1bff;
pub const STRIPE: u32 = 0xffffff06;
pub const ROW_HOVER: u32 = 0xffffff09;
pub const WINDOW: u32 = 0x222222f5;
pub const WINDOW_BORDER: u32 = 0xffffff24;

// Stage blocks use hue 212, between the accent blue and the green, red and orange reserved for statuses.
const STAGE_HUE: f32 = 212. / 360.;

pub fn focus_ring() -> BoxShadow {
    BoxShadow {
        color: rgba(ACCENT_SOFT).into(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(3.),
    }
}

pub fn window_shadow() -> BoxShadow {
    BoxShadow {
        color: hsla(0., 0., 0., 0.6),
        offset: point(px(0.), px(20.)),
        blur_radius: px(48.),
        spread_radius: px(0.),
    }
}

pub fn stage_border() -> Hsla {
    hsla(STAGE_HUE, 0.85, 0.56, 0.26)
}

pub fn stage_background() -> Hsla {
    hsla(STAGE_HUE, 0.85, 0.56, 0.09)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Primary,
    Danger,
}

/// A push button; a disabled button is dimmed and ignores clicks.
pub fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (background, hover, border, text) = match tone {
        Tone::Normal => (0xffffff1a, 0xffffff26, 0xffffff17, TEXT),
        Tone::Primary => (ACCENT, ACCENT_HOVER, 0x00000000, WHITE),
        Tone::Danger => (0xe5392fff, 0xff5147ff, 0x00000000, WHITE),
    };
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(CONTROL_HEIGHT))
        .px(px(14.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgba(border))
        .bg(rgba(background))
        .text_color(rgba(text))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .child(label.into())
        .map(|button| {
            if enabled {
                button.hover(move |style| style.bg(rgba(hover))).on_click(on_click)
            } else {
                button.opacity(0.4)
            }
        })
}

/// The sunken track holding [`segment`] toggles.
pub fn segments(children: impl IntoIterator<Item = impl IntoElement>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .h(px(CONTROL_HEIGHT))
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(8.))
        .border_1()
        .border_color(rgba(0xffffff17))
        .bg(rgba(0x00000047))
        .children(children)
}

pub fn segment(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h_full()
        .px(px(12.))
        .rounded(px(6.))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .whitespace_nowrap()
        .map(|segment| {
            if active {
                segment.bg(rgba(ACCENT)).text_color(rgba(WHITE))
            } else {
                segment.text_color(rgba(SECONDARY)).hover(|style| style.text_color(rgba(TEXT)))
            }
        })
        .on_click(on_click)
        .child(label.into())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Accent,
    Stage,
    White,
    Warning,
    Danger,
}

impl Fill {
    fn background(self) -> Background {
        match self {
            Self::Accent => linear_gradient(90., linear_color_stop(rgba(ACCENT_HOVER), 0.), linear_color_stop(rgba(ACCENT), 1.)),
            Self::Stage => linear_gradient(
                90.,
                linear_color_stop(hsla(STAGE_HUE, 1., 0.72, 1.), 0.),
                linear_color_stop(hsla(STAGE_HUE, 0.85, 0.56, 1.), 1.),
            ),
            Self::White => rgba(WHITE).into(),
            Self::Warning => rgba(WARNING).into(),
            Self::Danger => rgba(DANGER).into(),
        }
    }
}

/// A 6 px bar; `None` sweeps a segment across the track while the total is unknown.
pub fn progress(id: impl Into<ElementId>, ratio: Option<f32>, fill: Fill, track: u32) -> Div {
    let bar = div().absolute().top_0().bottom_0().rounded(px(3.)).bg(fill.background());
    div()
        .relative()
        .flex_shrink_0()
        .w_full()
        .h(px(6.))
        .rounded(px(3.))
        .overflow_hidden()
        .bg(rgba(track))
        .child(match ratio {
            Some(ratio) => bar.left_0().w(relative(ratio)).into_any_element(),
            None => bar
                .w(relative(0.35))
                .with_animation(
                    id,
                    Animation::new(Duration::from_millis(1200)).repeat().with_easing(ease_in_out),
                    |bar, delta| bar.left(relative(-0.35 + 1.35 * delta)),
                )
                .into_any_element(),
        })
}

pub fn dot(color: u32) -> Div {
    div().flex_shrink_0().size(px(8.)).rounded_full().bg(rgba(color))
}

pub fn message(text: impl Into<SharedString>) -> Div {
    div()
        .py(px(18.))
        .px(px(8.))
        .text_color(rgba(TERTIARY))
        .text_size(px(12.5))
        .text_center()
        .child(text.into())
}

/// A vertically scrolled area with a thumb showing the visible part. With `grow` it fills its parent; otherwise it
/// keeps its content height and only gives up space when the parent runs out.
pub fn scroll_area(id: impl Into<ElementId>, handle: &ScrollHandle, grow: bool, content: impl IntoElement) -> Div {
    div()
        .relative()
        .min_h_0()
        .flex()
        .flex_col()
        .when(grow, |area| area.flex_1())
        .child(
            div()
                .id(id)
                .min_h_0()
                .when(grow, |scroll| scroll.flex_1())
                .overflow_y_scroll()
                .track_scroll(handle)
                .child(content),
        )
        .children(scroll_thumb(handle))
}

// The handle reports the previous layout; scrolling re-renders the view, so the thumb follows within a frame.
fn scroll_thumb(handle: &ScrollHandle) -> Option<Div> {
    let view = handle.bounds().size.height;
    let hidden = handle.max_offset().height;
    if hidden <= px(0.) || view <= px(0.) {
        return None;
    }
    let thumb = (view * (view / (view + hidden))).max(px(24.)).min(view);
    let position = (-handle.offset().y / hidden).clamp(0., 1.);
    Some(
        div()
            .absolute()
            .right(px(2.))
            .top((view - thumb) * position)
            .w(px(6.))
            .h(thumb)
            .rounded(px(3.))
            .bg(rgba(0xffffff33)),
    )
}
