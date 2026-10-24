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
    image_frame(
        path,
        div().w(px(width.max(1.))),
        height,
        p,
        Corners::default(),
    )
}

/// Edge-to-edge cover inside a collection card's 1px border and 12px outer radius.
pub(super) fn collection_portrait(path: Option<PathBuf>, width: f32, p: Palette) -> AnyElement {
    collection_cover(path, width, width, p)
}

/// Edge-to-edge artwork inside a collection card's top corners.
pub(super) fn collection_cover(
    path: Option<PathBuf>,
    width: f32,
    height: f32,
    p: Palette,
) -> AnyElement {
    let corners = Corners {
        top_left: px(11.),
        top_right: px(11.),
        bottom_left: px(0.),
        bottom_right: px(0.),
    };
    image_frame(
        path,
        div().w(px(width.max(1.))).rounded_t(px(11.)),
        height,
        p,
        corners,
    )
}

fn image_frame(
    path: Option<PathBuf>,
    area: Div,
    height: f32,
    p: Palette,
    corners: Corners<Pixels>,
) -> AnyElement {
    let area = area
        .relative()
        .h(px(height.max(1.)))
        .flex_shrink_0()
        .bg(p.image)
        .rounded_tl(corners.top_left)
        .rounded_tr(corners.top_right)
        .rounded_bl(corners.bottom_left)
        .rounded_br(corners.bottom_right)
        .overflow_hidden();
    match path {
        Some(path) => area
            .child(super::images::fitted_picture(path, corners))
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

pub(super) fn collection_meta(content: impl IntoElement, status: Option<Div>, p: Palette) -> Div {
    h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(20.))
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(p.muted)
        .child(div().flex_1().min_w_0().child(content))
        .when_some(status, |row, status| row.child(status.flex_shrink_0()))
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
    image_frame(path, div().w_full(), height, p, Corners::default())
}

pub(super) fn picture_with_corners(
    path: Option<PathBuf>,
    height: f32,
    p: Palette,
    corners: Corners<Pixels>,
) -> AnyElement {
    image_frame(path, div().w_full(), height, p, corners)
}
