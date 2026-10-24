//! Shared content scrolling and compact result summary.
use super::*;

pub(super) fn scroll_content(
    id: &'static str,
    handle: &ScrollHandle,
    content: impl IntoElement,
) -> Div {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .child(
            div()
                .id(id)
                .size_full()
                .track_scroll(handle)
                .overflow_y_scroll()
                .child(content),
        )
        .child(scrollbar_overlay(
            gpui_kit::component::scroll::Scrollbar::vertical(handle)
                .id(format!("{id}-scrollbar"))
                .mode(gpui_kit::component::scroll::ScrollbarMode::Scrolling)
                .viewport_from_layout(),
        ))
}
pub(super) fn scrollbar_motion() -> gpui_kit::component::scroll::ScrollbarMotion {
    use gpui_kit::component::scroll::{ScrollbarEntrance, ScrollbarMotion};
    use std::time::Duration;
    ScrollbarMotion::default()
        .with_idle(Duration::from_millis(900))
        .with_enter(Duration::from_millis(100))
        .with_exit(Duration::from_millis(220))
        .with_expand(Duration::ZERO)
        .with_entrance(ScrollbarEntrance::Fade)
        .with_thumb_hover_entrance(ScrollbarEntrance::Fade)
}

pub(super) fn scrollbar_overlay(scrollbar: impl IntoElement) -> Div {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right(px(-14.))
        .child(scrollbar)
}
