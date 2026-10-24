//! Library overview with real totals and direct collection entry points.
use super::*;

impl Gallery {
    fn browse_category(&mut self, category: Category, decoration: &str, cx: &mut Context<Self>) {
        self.category = category;
        self.decoration = decoration.into();
        self.collection_key.clear();
        self.navigate(Page::Collection, cx);
    }

    pub(super) fn home(&self, window: &Window, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let connected = self.account.is_connected();
        let available =
            (f32::from(window.viewport_size().width) - self.sidebar_width() - 32.).max(300.);
        let spacious = ((f32::from(window.viewport_size().height) - 650.) / 250.).clamp(0., 1.);
        let gap = 8. + spacious * 4.;
        let portrait_height = 140. + spacious * 90.;
        let category_icon = 36. + spacious * 12.;
        let preview_count = ((available + 12.) / 165.).floor().clamp(3., 7.) as usize;
        let preview_width = (available - (preview_count - 1) as f32 * 12.) / preview_count as f32;
        let totals = [
            (
                Category::Characters,
                text::CHARACTERS,
                IconName::Users,
                self.library.characters.len(),
                self.library
                    .characters
                    .iter()
                    .filter(|c| self.account.snapshot.character(&c.id).owned)
                    .count(),
            ),
            (
                Category::Skins,
                text::SKINS,
                IconName::Image,
                self.library.skin_count(),
                self.library
                    .characters
                    .iter()
                    .flat_map(|c| &c.skins)
                    .filter(|s| self.account.snapshot.owns_skin(s))
                    .count(),
            ),
            (
                Category::Decorations,
                text::DECORATIONS,
                IconName::Layers,
                self.library.decorations.len(),
                self.library
                    .decorations
                    .iter()
                    .filter(|d| self.account.snapshot.owns_decoration(d))
                    .count(),
            ),
        ];
        let mut body =
            v_flex()
                .gap(px(gap))
                .child(
                    h_flex().h(px(28.)).items_center().child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("概览"),
                    ),
                )
                .child(h_flex().gap_3().children(totals.into_iter().map(
                    |(category, label, icon, total, owned)| {
                        v_flex()
                            .id(format!("home-stat-{}", category as u8))
                            .flex_1()
                            .min_w_0()
                            .px_4()
                            .py(px(8. + spacious * 8.))
                            .gap_2()
                            .rounded_xl()
                            .bg(p.panel)
                            .border_1()
                            .border_color(p.border)
                            .cursor_pointer()
                            .hover(move |style| style.border_color(p.accent).bg(p.accent_bg))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.browse_category(category, "", cx)
                            }))
                            .child(
                                h_flex()
                                    .justify_between()
                                    .items_center()
                                    .text_color(p.muted)
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child(Icon::new(icon).size_4())
                                            .child(label),
                                    )
                                    .child(Icon::new(IconName::ArrowUpRight).size_4()),
                            )
                            .child(div().text_2xl().font_weight(FontWeight::BOLD).child(
                                if connected {
                                    format!("已拥有 {owned}")
                                } else {
                                    format!("共 {total}")
                                },
                            ))
                            .when(connected, |card| {
                                card.child(
                                    v_flex().gap_2().child(
                                        div()
                                            .text_xs()
                                            .text_color(p.muted)
                                            .child(format!("共 {total}")),
                                    ),
                                )
                            })
                    },
                )))
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("角色"))
                        .child(
                            Button::new("home-characters")
                                .outline()
                                .small()
                                .label("查看全部")
                                .icon(IconName::ArrowRight)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.browse_category(Category::Characters, "", cx)
                                })),
                        ),
                )
                .child(
                    h_flex().gap_3().flex_shrink_0().items_start().children(
                        self.library
                            .characters
                            .iter()
                            .take(preview_count)
                            .enumerate()
                            .map(|(index, character)| {
                                v_flex()
                                    .id(("home-character", index))
                                    .w(px(preview_width))
                                    .rounded_lg()
                                    .overflow_hidden()
                                    .bg(p.panel)
                                    .border_1()
                                    .border_color(p.border)
                                    .cursor_pointer()
                                    .hover(move |style| {
                                        style.border_color(p.accent).bg(p.accent_bg)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.open_character(index, cx)
                                    }))
                                    .child(super::cards::picture_with_corners(
                                        character.thumbnail(),
                                        portrait_height,
                                        p,
                                        Corners {
                                            top_left: px(7.),
                                            top_right: px(7.),
                                            ..Default::default()
                                        },
                                    ))
                                    .child(
                                        div()
                                            .px_3()
                                            .py_2()
                                            .text_sm()
                                            .truncate()
                                            .child(character.name.clone()),
                                    )
                            }),
                    ),
                )
                .child(div().font_weight(FontWeight::SEMIBOLD).child("分类"));
        let categories = [
            "background",
            "frame",
            "table",
            "tile-back",
            "tile-face",
            "hand",
            "win-effect",
            "riichi-effect",
            "call-effect",
            "stick",
            "bgm",
            "illustrations",
        ];
        for row in categories.chunks(4) {
            body = body.child(h_flex().gap_3().children(row.iter().map(|&key| {
                let entries = self
                    .library
                    .decorations
                    .iter()
                    .filter(|d| d.matches_category(key))
                    .collect::<Vec<_>>();
                let (category, label, image, total, owned) = if key == "illustrations" {
                    (
                        Category::Illustrations,
                        text::ILLUSTRATIONS,
                        self.library
                            .illustrations
                            .first()
                            .and_then(|item| item.thumbnail.clone()),
                        self.library.illustrations.len(),
                        self.library
                            .illustrations
                            .iter()
                            .filter(|item| self.account.snapshot.owns_illustration(item))
                            .count(),
                    )
                } else {
                    (
                        Category::Decorations,
                        decoration_name(key),
                        entries.first().and_then(|item| item.image.clone()),
                        entries.len(),
                        entries
                            .iter()
                            .filter(|item| self.account.snapshot.owns_decoration(item))
                            .count(),
                    )
                };
                h_flex()
                    .id(format!("home-deco-{key}"))
                    .w(px((available - 36.) / 4.))
                    .min_w_0()
                    .p(px(8. + spacious * 4.))
                    .gap_3()
                    .items_center()
                    .rounded_lg()
                    .bg(p.panel)
                    .border_1()
                    .border_color(p.border)
                    .cursor_pointer()
                    .hover(move |style| style.border_color(p.accent).bg(p.accent_bg))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.browse_category(category, key, cx)),
                    )
                    .child(
                        div()
                            .w(px(category_icon))
                            .flex_shrink_0()
                            .rounded_md()
                            .overflow_hidden()
                            .child(super::cards::picture_with_corners(
                                image,
                                category_icon,
                                p,
                                Corners::all(px(6.)),
                            )),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_1()
                            .child(div().truncate().text_sm().child(label))
                            .child(div().text_xs().text_color(p.muted).child(if connected {
                                format!("已拥有 {owned} / 共 {total}")
                            } else {
                                format!("共 {total} 项")
                            })),
                    )
            })));
        }
        body.h_full().into_any_element()
    }
}
