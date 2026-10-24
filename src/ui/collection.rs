use super::*;

enum Row {
    Characters(Vec<usize>),
    SkinHeading(usize, usize),
    Skins(usize, Vec<usize>),
    SkinGallery(Vec<(usize, usize)>),
    Decorations(Vec<usize>),
    Titles(Vec<usize>),
}

impl Gallery {
    fn character_card(
        &self,
        index: usize,
        character: &Character,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.account.snapshot.character(&character.id);
        let thumbnail = character.thumbnail();
        let collaboration = character.collaboration;
        collection_card(
            ("character-card", index),
            width,
            self.account.is_connected(),
            state.owned,
            p,
        )
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| this.open_character(index, cx)))
        .child(collection_image(thumbnail, width - 2., width * 0.95, p))
        .child(
            v_flex()
                .p_3()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(character.name.clone()),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .when(character.limited || character.collaboration, |row| {
                                    row.child(
                                        div()
                                            .id(format!("availability-{}", character.id))
                                            .text_color(p.gold)
                                            .tooltip(move |window, cx| {
                                                gpui_kit::component::tooltip::Tooltip::new(
                                                    if collaboration {
                                                        "联动限定"
                                                    } else {
                                                        text::LIMITED
                                                    },
                                                )
                                                .build(window, cx)
                                            })
                                            .child(
                                                Icon::new(if character.collaboration {
                                                    IconName::Handshake
                                                } else {
                                                    IconName::Gem
                                                })
                                                .size_4(),
                                            ),
                                    )
                                })
                                .child(self.favorite_indicator(&character.id, p)),
                        ),
                )
                .child(collection_meta(
                    div().truncate().child(format!(
                        "{} 套皮肤",
                        character
                            .skins
                            .iter()
                            .filter(|skin| !self.account.is_connected()
                                || !self.only_owned
                                || self.account.snapshot.owns_skin(skin))
                            .count()
                    )),
                    self.account
                        .is_connected()
                        .then(|| collection_status(state.owned, state.contracted, p)),
                    p,
                )),
        )
        .into_any_element()
    }
    pub(super) fn collection(
        &mut self,
        window: &mut Window,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = crate::search::normalize(self.search.read(cx).value().as_ref());
        let lib = self.library.clone();
        let available =
            (f32::from(window.viewport_size().width) - self.sidebar_width() - 32.).max(300.);
        let target = [130., 200., 280.][self.card_size];
        let columns = ((available + 16.) / (target + 16.)).floor().max(1.);
        let width = (available - (columns - 1.) * 16.) / columns;
        let columns = columns as usize;
        let tabs = [
            (Category::Characters, text::CHARACTERS),
            (Category::Skins, text::SKINS),
            (Category::Decorations, text::DECORATIONS),
            (Category::Titles, text::TITLES),
        ];
        let mut controls = controls_panel(p)
            .child(
                controls_row()
                    .justify_between()
                    .flex_wrap()
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .children(tabs.into_iter().map(|(category, name)| {
                                view_tab(
                                    format!("category-{name}"),
                                    name,
                                    self.category == category,
                                    p,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.category = category;
                                        cx.notify();
                                    },
                                ))
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .flex_wrap()
                            .when(self.category == Category::Skins, |row| {
                                row.child(
                                    h_flex().gap_1().items_center().children(
                                        [
                                            (true, "按角色", IconName::List),
                                            (false, "画廊", IconName::LayoutGrid),
                                        ]
                                        .into_iter()
                                        .map(
                                            |(grouped, label, icon)| {
                                                view_tab(
                                                    format!("skin-view-{grouped}"),
                                                    label,
                                                    self.skins_grouped == grouped,
                                                    p,
                                                )
                                                .icon(icon)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.skins_grouped = grouped;
                                                    cx.notify();
                                                }))
                                            },
                                        ),
                                    ),
                                )
                            })
                            .child(self.card_size_controls(p, cx)),
                    ),
            )
            .child(div().h(px(1.)).w_full().bg(p.border))
            .when(self.category != Category::Decorations, |panel| {
                panel.child(self.filter_controls(
                    false,
                    matches!(self.category, Category::Characters | Category::Skins),
                    p,
                    cx,
                ))
            });
        if self.category == Category::Decorations {
            let filters = [
                "",
                "frame",
                "table",
                "tile-back",
                "tile-face",
                "hand",
                "win-effect",
                "riichi-effect",
                "call-effect",
                "stick",
                "background",
                "bgm",
            ]
            .map(|key| {
                (
                    key,
                    if key.is_empty() {
                        text::ALL
                    } else {
                        decoration_name(key)
                    },
                )
            });
            controls = controls.child(
                controls_row()
                    .w_full()
                    .child(
                        div().flex_1().min_w_0().h_full().child(
                            h_flex()
                                .id("decoration-types")
                                .gap_2()
                                .h_full()
                                .items_center()
                                .children(filters.into_iter().map(|(key, label)| {
                                    filter_chip(
                                        format!("deco-filter-{key}"),
                                        label,
                                        self.decoration == key,
                                        p,
                                    )
                                    .flex_shrink_0()
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.decoration = key.into();
                                            cx.notify();
                                        },
                                    ))
                                }))
                                .overflow_x_scrollbar(),
                        ),
                    )
                    .child(self.account_filters(false, p, cx)),
            );
        }
        let mut body = v_flex()
            .h_full()
            .min_h_0()
            .gap_3()
            .w_full()
            .child(controls.flex_shrink_0());
        let mut rows = Vec::new();
        let total;
        match self.category {
            Category::Characters => {
                let mut filtered = lib
                    .characters
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        c.search.matches(&query)
                            && self.character_matches(c)
                            && (!self.account.is_connected()
                                || !self.only_owned
                                || self.account.snapshot.character(&c.id).owned)
                    })
                    .collect::<Vec<_>>();
                total = filtered.len();
                self.account.snapshot.sort_characters(
                    &mut filtered,
                    if self.account.is_connected() {
                        self.character_order
                    } else {
                        crate::account::CharacterOrder::Release
                    },
                    self.descending,
                );
                for chunk in filtered.chunks(columns) {
                    rows.push(Row::Characters(chunk.iter().map(|(i, _)| *i).collect()));
                }
            }
            Category::Skins => {
                let mut count = 0;
                let mut flat = Vec::new();
                let mut indices = lib.characters.iter().enumerate().collect::<Vec<_>>();
                self.account.snapshot.sort_characters(
                    &mut indices,
                    if self.account.is_connected() {
                        self.character_order
                    } else {
                        crate::account::CharacterOrder::Release
                    },
                    self.descending,
                );
                for (i, _) in indices {
                    let c = &lib.characters[i];
                    let mut skins = c
                        .skins
                        .iter()
                        .enumerate()
                        .filter(|(_, skin)| {
                            (skin.search.matches(&query) || c.search.matches(&query))
                                && (!self.only_animated_skins || skin.animated)
                                && self.character_matches(c)
                                && (!self.account.is_connected()
                                    || !self.only_owned
                                    || self.account.snapshot.owns_skin(skin))
                        })
                        .collect::<Vec<_>>();
                    if self.descending {
                        skins.reverse();
                    }
                    if skins.is_empty() {
                        continue;
                    }
                    count += skins.len();
                    if self.skins_grouped {
                        rows.push(Row::SkinHeading(i, skins.len()));
                        for chunk in skins.chunks(columns) {
                            rows.push(Row::Skins(i, chunk.iter().map(|(s, _)| *s).collect()));
                        }
                    } else {
                        flat.extend(skins.iter().map(|(s, _)| (i, *s)));
                    }
                }
                for chunk in flat.chunks(columns) {
                    rows.push(Row::SkinGallery(chunk.to_vec()));
                }
                total = count;
            }
            Category::Decorations => {
                let mut filtered = lib
                    .decorations
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| {
                        d.search.matches(&query)
                            && d.matches_category(&self.decoration)
                            && (!self.account.is_connected()
                                || !self.only_owned
                                || self.account.snapshot.owns_decoration(d))
                    })
                    .collect::<Vec<_>>();
                total = filtered.len();
                if self.descending {
                    filtered.reverse();
                }
                for chunk in filtered.chunks(columns) {
                    rows.push(Row::Decorations(chunk.iter().map(|(i, _)| *i).collect()));
                }
            }
            Category::Titles => {
                let mut filtered = lib
                    .titles
                    .iter()
                    .enumerate()
                    .filter(|(_, title)| {
                        title.search.matches(&query)
                            && (!self.account.is_connected()
                                || !self.only_owned
                                || self.account.snapshot.owns_title(title))
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                total = filtered.len();
                if self.descending {
                    filtered.reverse();
                }
                for chunk in filtered.chunks(columns) {
                    rows.push(Row::Titles(chunk.to_vec()));
                }
            }
        }
        let key = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{:?}|{}|{}|{}|{}|{}|{}",
            self.category as u8,
            query,
            self.only_owned,
            self.decoration,
            width.to_bits(),
            self.card_size,
            lib.root.display(),
            total,
            self.character_sex,
            (self.character_limited, self.character_collaboration),
            self.skins_grouped,
            self.only_favorites,
            self.descending,
            self.only_animated_skins,
            self.skin_full_art,
            self.character_order as u8
        );
        if key != self.collection_key {
            self.collection_list.reset(rows.len());
            self.collection_list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
            self.collection_key = key;
        }
        body = body.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    list(
                        self.collection_list.clone(),
                        cx.processor(move |this, index, _, cx| match &rows[index] {
                            Row::Characters(indices) => h_flex()
                                .gap_4()
                                .pb_4()
                                .items_start()
                                .children(indices.iter().map(|&i| {
                                    this.character_card(i, &lib.characters[i], width, p, cx)
                                }))
                                .into_any_element(),
                            Row::SkinHeading(i, count) => h_flex()
                                .gap_3()
                                .items_center()
                                .pt_3()
                                .pb_3()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(lib.characters[*i].name.clone()),
                                )
                                .child(badge(format!("{count} 套"), p.muted, p.panel))
                                .into_any_element(),
                            Row::Skins(i, indices) => h_flex()
                                .gap_4()
                                .pb_4()
                                .items_start()
                                .children(indices.iter().map(|&s| {
                                    this.skin_card(
                                        (*i, s),
                                        &lib.characters[*i],
                                        &lib.characters[*i].skins[s],
                                        width,
                                        p,
                                        cx,
                                    )
                                }))
                                .into_any_element(),
                            Row::SkinGallery(indices) => h_flex()
                                .gap_4()
                                .pb_4()
                                .items_start()
                                .children(indices.iter().map(|&(i, s)| {
                                    this.skin_card(
                                        (i, s),
                                        &lib.characters[i],
                                        &lib.characters[i].skins[s],
                                        width,
                                        p,
                                        cx,
                                    )
                                }))
                                .into_any_element(),
                            Row::Titles(indices) => h_flex()
                                .gap_4()
                                .pb_4()
                                .items_start()
                                .children(
                                    indices
                                        .iter()
                                        .map(|&i| this.title_card(&lib.titles[i], width, p)),
                                )
                                .into_any_element(),
                            Row::Decorations(indices) => h_flex()
                                .gap_4()
                                .pb_4()
                                .items_start()
                                .children(indices.iter().map(|&i| {
                                    this.decoration_card(i, &lib.decorations[i], width, p)
                                }))
                                .into_any_element(),
                        }),
                    )
                    .flex_1()
                    .size_full(),
                )
                .child(browsing::scrollbar_overlay(
                    gpui_kit::component::scroll::Scrollbar::vertical(&self.collection_list)
                        .id("collection-scrollbar")
                        .mode(gpui_kit::component::scroll::ScrollbarMode::Scrolling)
                        .viewport_from_layout(),
                )),
        );
        if total == 0 {
            body = body.child(self.empty("没有找到匹配的收藏", "试试其他名称或筛选条件。", p));
        }
        body.into_any_element()
    }
    fn skin_card(
        &self,
        indices: (usize, usize),
        character: &Character,
        skin: &Skin,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (character_index, skin_index) = indices;
        let owned = self.account.snapshot.owns_skin(skin);
        let art_width = (width - 2.).max(1.);
        let art_height = width * if self.skin_full_art { 1.35 } else { 0.95 };
        let path = if self.skin_full_art {
            skin.full_image()
        } else {
            skin.thumbnail()
        };
        let artwork = collection_image(path, art_width, art_height, p);
        collection_card(
            format!("skin-card-{character_index}-{skin_index}"),
            width,
            self.account.is_connected(),
            owned,
            p,
        )
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selected = Some(character_index);
            this.skin = skin_index;
            cx.notify();
        }))
        .child(artwork)
        .child(
            v_flex()
                .p_3()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(skin.display_name(&character.name).to_owned()),
                )
                .child(collection_meta(
                    h_flex()
                        .min_w_0()
                        .gap_1()
                        .items_center()
                        .when(!self.skins_grouped, |row| {
                            row.child(div().min_w_0().truncate().child(character.name.clone()))
                        })
                        .when(skin.animated, |row| {
                            row.child(badge("动态", p.accent, p.accent_bg).flex_shrink_0())
                        }),
                    self.account
                        .is_connected()
                        .then(|| collection_status(owned, false, p)),
                    p,
                )),
        )
        .into_any_element()
    }
    fn title_card(&self, title: &Title, width: f32, p: Palette) -> AnyElement {
        let owned = self.account.snapshot.owns_title(title);
        collection_card(
            format!("title-{}", title.id),
            width,
            self.account.is_connected(),
            owned,
            p,
        )
        .p_3()
        .gap_3()
        .child(collection_image(title.image.clone(), width - 26., 64., p))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(title.name.clone()),
        )
        .when(self.account.is_connected(), |card| {
            card.child(collection_meta(
                div(),
                Some(collection_status(owned, false, p)),
                p,
            ))
        })
        .into_any_element()
    }
    fn decoration_card(
        &self,
        _index: usize,
        item: &Decoration,
        width: f32,
        p: Palette,
    ) -> AnyElement {
        collection_card(
            format!("decoration-card-{}-{}", item.category, item.id),
            width,
            self.account.is_connected(),
            self.account.snapshot.owns_decoration(item),
            p,
        )
        .p_3()
        .gap_3()
        .child(collection_image(
            item.image.clone(),
            width - 26.,
            width - 26.,
            p,
        ))
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(item.name.clone()),
        )
        .child(collection_meta(
            div().truncate().child(decoration_name(&item.category)),
            self.account
                .is_connected()
                .then(|| collection_status(self.account.snapshot.owns_decoration(item), false, p)),
            p,
        ))
        .into_any_element()
    }
}
