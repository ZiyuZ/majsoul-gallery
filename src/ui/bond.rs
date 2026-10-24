use super::*;

pub(super) fn heart_row(id: impl Into<ElementId>, total: u64, size: f32, p: Palette) -> AnyElement {
    let progress = BondProgress::new(total);
    let stage = if progress.hearts == 5 {
        "5 心 · 已满好感".into()
    } else {
        format!(
            "{} 心 · 本级 {} / {} · 距离下一心还需 {}",
            progress.hearts,
            number(progress.stage_current),
            number(progress.stage_required),
            number(progress.remaining())
        )
    };
    let tooltip = format!(
        "{stage}\n累计 {} / {}",
        number(progress.total),
        number(MAX_BOND)
    );
    h_flex()
        .id(id)
        .gap(px(2.))
        .items_center()
        .children((0..5).map(|i| {
            let fill = progress.heart_fill(i);
            div()
                .relative()
                .size(px(size))
                .flex_shrink_0()
                .child(
                    Icon::empty()
                        .data(include_bytes!("../assets/heart-outline.svg"))
                        .size(px(size))
                        .text_color(p.muted.opacity(0.5)),
                )
                .when(fill > 0., |heart| {
                    heart.child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .w(px(size))
                            .h(px(size * fill))
                            .overflow_hidden()
                            .child(
                                div().absolute().bottom_0().left_0().size(px(size)).child(
                                    Icon::empty()
                                        .data(include_bytes!("../assets/heart-filled.svg"))
                                        .size(px(size))
                                        .text_color(rgb(0xe36d86)),
                                ),
                            ),
                    )
                })
        }))
        .tooltip(move |window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .into_any_element()
}
