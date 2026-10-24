//! Shared buttons, choice groups, badges and material icons.
use super::*;

pub(super) fn badge(text: impl Into<SharedString>, color: Hsla, bg: Hsla) -> Div {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .text_color(color)
        .bg(bg)
        .child(text.into())
}
pub(super) fn material_color(contracted: bool, available: u64, needed: u64, p: Palette) -> Hsla {
    if contracted {
        p.muted
    } else if available >= needed {
        p.accent
    } else {
        p.gold
    }
}
pub(super) fn material_icon(
    id: impl Into<ElementId>,
    path: Option<PathBuf>,
    size: f32,
    color: Hsla,
    tooltip: String,
    p: Palette,
) -> AnyElement {
    div()
        .id(id)
        .group("material-icon")
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .bg(p.image)
        .overflow_hidden()
        .child(picture(path, size, p))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .border_2()
                .border_color(color)
                .bg(rgba(0x00000000))
                .group_hover("material-icon", |s| s.bg(rgba(0x00000038))),
        )
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .into_any_element()
}
pub(super) fn back_icon_button(
    id: impl Into<ElementId>,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .outline()
        .w(px(40.))
        .h(px(40.))
        .p_0()
        .icon(Icon::new(IconName::ArrowLeft).size_6())
        .tooltip(tooltip)
}
pub(super) fn view_tab(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    p: Palette,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .label(label)
        .selected(selected)
        .toggled(selected)
        .text_color(if selected { p.accent } else { p.muted })
        .when(selected, |button| button.bg(p.accent_bg))
}
pub(super) fn segment_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    p: Palette,
) -> Button {
    Button::new(id)
        .label(label)
        .selected(selected)
        .when(selected, |button| {
            button.bg(p.accent_bg).text_color(p.accent)
        })
}
pub(super) fn filter_chip(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    p: Palette,
) -> Button {
    Button::new(id)
        .outline()
        .small()
        .label(label)
        .selected(selected)
        .bg(if selected { p.accent_bg } else { p.panel })
        .text_color(if selected { p.accent } else { p.muted })
        .border_color(if selected { p.accent } else { p.border })
}
pub(super) fn choice_group(id: impl Into<ElementId>) -> ButtonGroup {
    ButtonGroup::new(id).outline().compact().small()
}
