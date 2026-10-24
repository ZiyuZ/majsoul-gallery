//! Shared collection card frame, ownership state and bounded artwork layout.
use super::*;

pub(super) fn collection_card(
    id: impl Into<ElementId>,
    width: f32,
    connected: bool,
    owned: bool,
    p: Palette,
) -> Stateful<Div> {
    v_flex()
        .id(id)
        .w(px(width))
        .rounded_xl()
        .overflow_hidden()
        .bg(if connected && !owned { p.bg } else { p.panel })
        .border_1()
        .border_color(p.border)
        .hover(move |style| style.border_color(p.accent))
}

pub(super) fn collection_image(
    path: Option<PathBuf>,
    width: f32,
    height: f32,
    p: Palette,
) -> AnyElement {
    let area = div()
        .w(px(width.max(1.)))
        .h(px(height.max(1.)))
        .flex_shrink_0()
        .bg(p.image)
        .overflow_hidden();
    match path {
        Some(path) => area
            .child(
                img(path)
                    .w(px(width.max(1.)))
                    .h(px(height.max(1.)))
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element(),
        None => area
            .flex()
            .items_center()
            .justify_center()
            .text_color(p.muted)
            .child(Icon::new(IconName::Image).size_8())
            .into_any_element(),
    }
}

pub(super) fn collection_status(owned: bool, contracted: bool, p: Palette) -> Div {
    badge(
        if !owned {
            "未拥有"
        } else if contracted {
            text::CONTRACTED
        } else {
            text::OWNED
        },
        if !owned {
            p.muted
        } else if contracted {
            p.gold
        } else {
            p.accent
        },
        if owned && contracted {
            p.gold.opacity(0.12)
        } else if owned {
            p.accent_bg
        } else {
            p.bg
        },
    )
}
pub(super) fn picture(path: Option<PathBuf>, height: f32, p: Palette) -> AnyElement {
    let body = div()
        .w_full()
        .h(px(height))
        .flex_shrink_0()
        .bg(p.image)
        .overflow_hidden();
    match path {
        Some(path) => body
            .child(img(path).size_full().object_fit(ObjectFit::Contain))
            .into_any_element(),
        None => body
            .flex()
            .items_center()
            .justify_center()
            .text_color(p.muted)
            .child(Icon::new(IconName::Image).size_8())
            .into_any_element(),
    }
}
