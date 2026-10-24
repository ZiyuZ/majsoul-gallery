use super::*;

impl Gallery {
    pub(super) fn detail(
        &self,
        index: usize,
        _window: &mut Window,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let lib = self.library.clone();
        let c = &lib.characters[index];
        let state = self.account.snapshot.character(&c.id);
        let skin = c.skins.get(self.skin).or_else(|| c.skins.first());
        #[cfg(feature = "spine")]
        let playing = self.animation_enabled && skin.is_some_and(|s| s.scene.is_some());
        #[cfg(not(feature = "spine"))]
        let animation_controls: Vec<AnyElement> = Vec::new();
        #[cfg(feature = "spine")]
        let animation_controls = {
            let mut controls = Vec::new();
            if playing {
                controls.push(self.animation_menu(cx));
            }
            controls.push(
                Button::new("animation-toggle")
                    .outline()
                    .small()
                    .icon(if playing {
                        IconName::Pause
                    } else {
                        IconName::Play
                    })
                    .label(if playing {
                        "关闭动画"
                    } else {
                        "播放动画"
                    })
                    .selected(playing)
                    .disabled(!skin.is_some_and(|s| s.scene.is_some()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.animation_enabled = !this.animation_enabled;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            controls
        };
        let mut side = v_flex()
            .w_full()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(c.name.clone()),
                    )
                    .child(self.favorite_indicator(&c.id, p)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .when(self.account.is_connected(), |row| {
                        row.child(badge(
                            if state.owned {
                                text::OWNED
                            } else {
                                "待收集"
                            },
                            p.accent,
                            p.accent_bg,
                        ))
                    })
                    .when(c.limited, |d| {
                        d.child(badge(text::LIMITED, p.gold, p.panel))
                    })
                    .when(c.collaboration, |d| d.child(badge("联动", p.gold, p.panel))),
            );
        if let Some(skin) = skin {
            side = side.child(
                div()
                    .text_sm()
                    .text_color(p.muted)
                    .child(skin.display_name(&c.name).to_owned()),
            );
        }
        side =
            side.when(self.account.is_connected(), |side| {
                side.child(
                    v_flex()
                        .p_3()
                        .gap_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(p.border)
                        .bg(p.panel)
                        .child(
                            h_flex()
                                .justify_between()
                                .text_xs()
                                .child("累计好感 / 总好感")
                                .child(div().text_color(p.accent).child(if state.contracted {
                                    text::CONTRACTED
                                } else {
                                    "培养中"
                                })),
                        )
                        .child(super::bond::heart_row("detail-hearts", state.bond, 24., p))
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!(
                                    "{} / {}",
                                    number(BondProgress::new(state.bond).total),
                                    number(MAX_BOND)
                                )),
                        )
                        .child(
                            Progress::new("detail-bond")
                                .value(BondProgress::new(state.bond).percent())
                                .color(p.accent)
                                .small(),
                        )
                        .child(div().text_xs().text_color(p.muted).child(
                            if state.bond >= MAX_BOND {
                                "已达到 5 心".into()
                            } else {
                                let bond = BondProgress::new(state.bond);
                                format!(
                                    "本级 {} / {} · 下一心还需 {}",
                                    number(bond.stage_current),
                                    number(bond.stage_required),
                                    number(bond.remaining())
                                )
                            },
                        )),
                )
            });
        side = side.child(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .justify_between()
                        .text_xs()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("契约材料"))
                        .child(
                            div()
                                .text_color(p.muted)
                                .child(if self.account.is_connected() {
                                    "已有 / 需要"
                                } else {
                                    "需要数量"
                                }),
                        ),
                )
                .child(
                    div()
                        .grid()
                        .grid_cols(3)
                        .w_full()
                        .gap_x_2()
                        .gap_y_3()
                        .children(c.materials.iter().enumerate().filter_map(|(i, g)| {
                            let item = g.display_item()?;
                            let available = self.account.snapshot.available(g);
                            Some(
                                v_flex()
                                    .w_full()
                                    .min_w_0()
                                    .gap_1()
                                    .items_center()
                                    .child(material_icon(
                                        format!("detail-material-{i}"),
                                        item.icon.clone(),
                                        44.,
                                        if self.account.is_connected() {
                                            material_color(
                                                state.contracted,
                                                available,
                                                g.quantity,
                                                p,
                                            )
                                        } else {
                                            p.border
                                        },
                                        if self.account.is_connected() {
                                            format!(
                                                "{} · 已有 {} / 需要 {}",
                                                item.name, available, g.quantity
                                            )
                                        } else {
                                            format!("{} × {}", item.name, g.quantity)
                                        },
                                        p,
                                    ))
                                    .child(div().text_xs().text_center().child(item.name.clone()))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if self.account.is_connected() {
                                                material_color(
                                                    state.contracted,
                                                    available,
                                                    g.quantity,
                                                    p,
                                                )
                                            } else {
                                                p.muted
                                            })
                                            .child(if self.account.is_connected() {
                                                format!("{available}/{}", g.quantity)
                                            } else {
                                                format!("× {}", g.quantity)
                                            }),
                                    ),
                            )
                        })),
                ),
        );
        if !c.gifts.is_empty() {
            side = side.child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("喜欢的礼物"),
                    )
                    .child(div().grid().grid_cols(3).w_full().gap_2().children(
                        c.gifts.iter().enumerate().map(|(i, item)| {
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .gap_1()
                                .items_center()
                                .child(
                                    Button::new(format!("gift-{i}"))
                                        .rounded(ButtonRounded::None)
                                        .ghost()
                                        .p_0()
                                        .w(px(44.))
                                        .h(px(44.))
                                        .child(div().w(px(42.)).child(picture(
                                            item.icon.clone(),
                                            42.,
                                            p,
                                        )))
                                        .tooltip(item.name.clone()),
                                )
                                .child(div().text_xs().text_center().child(item.name.clone()))
                        }),
                    )),
            );
        }
        let art = skin.and_then(|s| s.full_image());
        let preview = self.preview(art, p, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .gap_3()
            .child(
                h_flex()
                    .h(px(40.))
                    .flex_shrink_0()
                    .justify_between()
                    .items_center()
                    .child(
                        back_icon_button(
                            "back",
                            match self.page {
                                Page::Home => "返回主页",
                                Page::Training => "返回培养",
                                Page::Collection => "返回收藏",
                            },
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.selected = None;
                            cx.notify();
                        })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new("export-skin")
                                    .outline()
                                    .small()
                                    .icon(IconName::Download)
                                    .tooltip("导出皮肤")
                                    .disabled(skin.is_none())
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.open_skin_export(cx)),
                                    ),
                            )
                            .child(self.preview_controls(cx))
                            .children(animation_controls),
                    ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .items_stretch()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .gap_2()
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .flex_1()
                                    .min_h_0()
                                    .rounded_lg()
                                    .overflow_hidden()
                                    .border_1()
                                    .border_color(p.border)
                                    .child(preview)
                                    .when_some(self.animation_error.clone(), |d, error| {
                                        d.child(
                                            div()
                                                .absolute()
                                                .top_2()
                                                .left_2()
                                                .right_2()
                                                .p_2()
                                                .bg(p.panel)
                                                .text_xs()
                                                .text_color(p.gold)
                                                .child(error),
                                        )
                                    })
                                    .when(
                                        self.animation_path.is_some()
                                            && self.animation_frame.is_none()
                                            && self.animation_error.is_none(),
                                        |d| {
                                            d.child(div().absolute().top_2().left_2().child(badge(
                                                "正在加载动态皮肤…",
                                                p.muted,
                                                p.panel,
                                            )))
                                        },
                                    ),
                            )
                            .child(
                                div().h(px(54.)).flex_shrink_0().child(
                                    h_flex()
                                        .id("skin-choices")
                                        .gap_2()
                                        .h_full()
                                        .items_center()
                                        .children(c.skins.iter().enumerate().map(|(i, s)| {
                                            v_flex()
                                                .id(("skin-choice", i))
                                                .w(px(48.))
                                                .flex_shrink_0()
                                                .rounded_md()
                                                .overflow_hidden()
                                                .border_2()
                                                .border_color(if self.skin == i {
                                                    p.accent
                                                } else {
                                                    p.border
                                                })
                                                .cursor_pointer()
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.skin = i;
                                                    cx.notify();
                                                }))
                                                .child(picture(s.thumbnail(), 48., p))
                                        }))
                                        .overflow_x_scrollbar(),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .w(px(290.))
                            .h_full()
                            .min_h_0()
                            .flex_shrink_0()
                            .child(side.id("detail-scroll").h_full().overflow_y_scrollbar()),
                    ),
            )
            .into_any_element()
    }
}
