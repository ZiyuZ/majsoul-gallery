//! Account inventory browsing; each business ID retains its own stock and lock state.
use super::form_dialog::FormDialog;
use super::*;
use majsoul_assets::core::items::{GiftRarity, ItemGroup};

pub(super) struct State {
    group: Option<ItemGroup>,
    rarity: Option<GiftRarity>,
    only_owned: bool,
    selected: Option<String>,
    list: ListState,
    key: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            group: None,
            rarity: None,
            only_owned: true,
            selected: None,
            list: ListState::new(0, ListAlignment::Top, px(240.)),
            key: String::new(),
        }
    }
}

impl Gallery {
    pub(super) fn inventory_page(
        &mut self,
        window: &Window,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.account.is_connected() {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_3()
                .child(Icon::new(IconName::Package).size_8().text_color(p.muted))
                .child(div().text_lg().child("登录后查看道具"))
                .child(
                    div()
                        .text_sm()
                        .text_color(p.muted)
                        .child("查看道具、礼物及各自的库存数量。"),
                )
                .child(
                    Button::new("inventory-login")
                        .outline()
                        .icon(IconName::LogIn)
                        .label(text::LOGIN)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.refresh_saved_password(window, cx);
                            this.modal = Some(Modal::Login);
                            cx.notify();
                        })),
                )
                .into_any_element();
        }
        let query = crate::search::normalize(self.search.read(cx).value().as_ref());
        let library = self.library.clone();
        let filtered = library
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                self.inventory
                    .group
                    .is_none_or(|group| item.definition.group == group)
                    && (self.inventory.group != Some(ItemGroup::Gifts)
                        || self
                            .inventory
                            .rarity
                            .is_none_or(|rarity| item.definition.rarity == Some(rarity)))
                    && item.search.matches(&query)
                    && (!self.inventory.only_owned
                        || self
                            .account
                            .snapshot
                            .inventory
                            .get(&item.definition.id)
                            .copied()
                            .unwrap_or(0)
                            > 0)
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let available =
            (f32::from(window.viewport_size().width) - self.sidebar_width() - 32.).max(300.);
        let target = [130., 180., 230.][self.card_size];
        let columns = ((available + 16.) / (target + 16.)).floor().max(1.) as usize;
        let width = (available - (columns - 1) as f32 * 16.) / columns as f32;
        let groups = [
            None,
            Some(ItemGroup::Materials),
            Some(ItemGroup::Gifts),
            Some(ItemGroup::Vouchers),
            Some(ItemGroup::Containers),
            Some(ItemGroup::Activity),
            Some(ItemGroup::Other),
        ];
        let controls = controls_panel(p)
            .child(
                controls_row()
                    .child(
                        div().flex_1().min_w_0().h_full().child(
                            h_flex()
                                .id("inventory-groups")
                                .h_full()
                                .gap_1()
                                .items_center()
                                .children(
                                    groups
                                        .into_iter()
                                        .enumerate()
                                        .filter(|(_, group)| {
                                            group.is_none_or(|group| {
                                                library
                                                    .items
                                                    .iter()
                                                    .any(|item| item.definition.group == group)
                                            })
                                        })
                                        .map(|(i, group)| {
                                            view_tab(
                                                format!("inventory-group-{i}"),
                                                group.map(ItemGroup::label).unwrap_or(text::ALL),
                                                self.inventory.group == group,
                                                p,
                                            )
                                            .on_click(
                                                cx.listener(move |this, _, _, cx| {
                                                    this.inventory.group = group;
                                                    cx.notify();
                                                }),
                                            )
                                        }),
                                )
                                .overflow_x_scrollbar(),
                        ),
                    )
                    .child(self.card_size_controls(p, cx)),
            )
            .child(div().h(px(1.)).w_full().bg(p.border))
            .child(
                controls_row()
                    .when(self.inventory.group == Some(ItemGroup::Gifts), |row| {
                        row.child(
                            choice_group("inventory-rarity").children(
                                [
                                    None,
                                    Some(GiftRarity::Green),
                                    Some(GiftRarity::Blue),
                                    Some(GiftRarity::Purple),
                                    Some(GiftRarity::Special),
                                ]
                                .into_iter()
                                .enumerate()
                                .map(|(i, rarity)| {
                                    segment_button(
                                        format!("inventory-rarity-{i}"),
                                        rarity.map(GiftRarity::label).unwrap_or(text::ALL),
                                        self.inventory.rarity == rarity,
                                        p,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.inventory.rarity = rarity;
                                            cx.notify();
                                        },
                                    ))
                                }),
                            ),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted)
                            .child(format!("共 {} 种", filtered.len())),
                    )
                    .child(
                        filter_chip("inventory-owned", text::OWNED, self.inventory.only_owned, p)
                            .icon(IconName::Check)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.inventory.only_owned = !this.inventory.only_owned;
                                cx.notify();
                            })),
                    ),
            );
        let key = format!(
            "{:?}|{:?}|{}|{}|{}|{}",
            self.inventory.group,
            self.inventory.rarity,
            self.inventory.only_owned,
            query,
            width.to_bits(),
            filtered.len()
        );
        let rows = filtered
            .chunks(columns)
            .map(|row| row.to_vec())
            .collect::<Vec<_>>();
        if self.inventory.key != key {
            self.inventory.list.reset(rows.len());
            self.inventory.list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
            self.inventory.key = key;
        }
        let empty = filtered.is_empty();
        let content = div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(
                list(
                    self.inventory.list.clone(),
                    cx.processor(move |this, row: usize, _, cx| {
                        h_flex()
                            .gap_4()
                            .pb_4()
                            .items_start()
                            .children(rows[row].iter().map(|&index| {
                                this.inventory_card(&library.items[index], width, p, cx)
                            }))
                            .into_any_element()
                    }),
                )
                .size_full(),
            )
            .child(browsing::scrollbar_overlay(
                gpui_kit::component::scroll::Scrollbar::vertical(&self.inventory.list)
                    .id("inventory-scrollbar")
                    .mode(gpui_kit::component::scroll::ScrollbarMode::Scrolling)
                    .viewport_from_layout(),
            ));
        v_flex()
            .size_full()
            .min_h_0()
            .gap_3()
            .child(controls.flex_shrink_0())
            .child(if empty {
                self.empty(
                    "没有符合条件的道具",
                    "试试其他分类、搜索名称，或关闭“已拥有”筛选。",
                    p,
                )
            } else {
                content.into_any_element()
            })
            .into_any_element()
    }

    fn inventory_card(
        &self,
        item: &model::InventoryItem,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let definition = &item.definition;
        let id = definition.id.clone();
        let quantity = self
            .account
            .snapshot
            .inventory
            .get(&id)
            .copied()
            .unwrap_or(0);
        collection_card(format!("inventory-item-{id}"), width, true, quantity > 0, p)
            .p_3()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.inventory.selected = Some(id.clone());
                this.modal = Some(Modal::InventoryItem);
                cx.notify();
            }))
            .child(collection_image(
                item.icon.clone(),
                width - 26.,
                width - 26.,
                p,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(definition.name.clone()),
                    )
                    .when(definition.locked, |row| {
                        row.child(
                            div()
                                .id(format!("inventory-lock-{}", definition.id))
                                .flex_shrink_0()
                                .text_color(p.gold)
                                .tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new("锁定礼物：不可分解")
                                        .build(window, cx)
                                })
                                .child(Icon::new(IconName::Lock).size_4()),
                        )
                    }),
            )
            .child(collection_meta(
                div().truncate().child(
                    definition
                        .rarity
                        .map(GiftRarity::label)
                        .unwrap_or(definition.group.label()),
                ),
                Some(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(if quantity > 0 { p.accent } else { p.muted })
                        .child(format!("× {}", number(quantity))),
                ),
                p,
            ))
            .into_any_element()
    }

    pub(super) fn inventory_item_dialog(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(item) = self
            .library
            .items
            .iter()
            .find(|item| Some(&item.definition.id) == self.inventory.selected.as_ref())
        else {
            return div().into_any_element();
        };
        let definition = &item.definition;
        let quantity = self
            .account
            .snapshot
            .inventory
            .get(&definition.id)
            .copied()
            .unwrap_or(0);
        let mut body = v_flex().gap_4().child(
            h_flex()
                .gap_4()
                .items_center()
                .child(div().size(px(72.)).flex_shrink_0().child(picture(
                    item.icon.clone(),
                    72.,
                    p,
                )))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("× {}", number(quantity))),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(badge(definition.group.label(), p.accent, p.accent_bg))
                                .when_some(definition.rarity, |row, rarity| {
                                    row.child(badge(rarity.label(), p.muted, p.bg))
                                })
                                .when(definition.locked, |row| {
                                    row.child(badge("不可分解", p.gold, p.gold.opacity(0.12)))
                                }),
                        ),
                ),
        );
        if !definition.description.is_empty() {
            body = body.child(div().text_sm().child(definition.description.clone()));
        }
        if definition.gift_bond > 0 {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(p.muted)
                    .child(format!("基础赠礼好感 +{}", number(definition.gift_bond))),
            );
        }
        if !definition.expires.is_empty() {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(p.muted)
                    .child(format!("到期时间：{}", definition.expires)),
            );
        }
        let equivalents = self
            .library
            .items
            .iter()
            .filter(|other| definition.same_gift(&other.definition))
            .collect::<Vec<_>>();
        if equivalents.len() > 1 {
            body = body.child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("同类礼物库存"),
                    )
                    .children(equivalents.iter().map(|other| {
                        let count = self
                            .account
                            .snapshot
                            .inventory
                            .get(&other.definition.id)
                            .copied()
                            .unwrap_or(0);
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .size(px(32.))
                                    .child(picture(other.icon.clone(), 32., p)),
                            )
                            .child(div().flex_1().text_sm().child(if other.definition.locked {
                                "锁定"
                            } else {
                                "普通"
                            }))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(p.accent)
                                    .child(format!("× {}", number(count))),
                            )
                    })),
            );
        }
        let related_ids = equivalents
            .iter()
            .map(|item| item.definition.id.as_str())
            .chain(std::iter::once(definition.id.as_str()))
            .collect::<std::collections::BTreeSet<_>>();
        let related = self
            .library
            .characters
            .iter()
            .enumerate()
            .filter_map(|(i, character)| {
                let favorite = character
                    .gifts
                    .iter()
                    .any(|gift| related_ids.contains(gift.id.as_str()));
                let contract = character.materials.iter().any(|group| {
                    group
                        .items
                        .iter()
                        .any(|item| related_ids.contains(item.id.as_str()))
                });
                (favorite || contract).then_some((i, character, favorite, contract))
            })
            .collect::<Vec<_>>();
        if !related.is_empty() {
            body =
                body.child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("关联角色"),
                        )
                        .children(related.into_iter().map(
                            |(index, character, favorite, contract)| {
                                let owned = self.account.snapshot.character(&character.id).owned;
                                h_flex()
                                    .id(("inventory-related", index))
                                    .gap_2()
                                    .p_2()
                                    .items_center()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(p.accent_bg))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.modal = None;
                                        this.open_character(index, cx);
                                    }))
                                    .child(div().size(px(32.)).flex_shrink_0().child(picture(
                                        character.thumbnail(),
                                        32.,
                                        p,
                                    )))
                                    .child(div().flex_1().text_sm().child(character.name.clone()))
                                    .when(favorite, |row| {
                                        row.child(badge("喜好", p.accent, p.accent_bg))
                                    })
                                    .when(contract, |row| {
                                        row.child(badge("契约", p.gold, p.gold.opacity(0.12)))
                                    })
                                    .child(collection_status(owned, false, p))
                            },
                        )),
                );
        }
        body = body.child(
            div()
                .text_xs()
                .text_color(p.muted)
                .child(format!("道具 ID：{}", definition.id)),
        );
        FormDialog::new(definition.name.clone(), "道具详情", body, div(), p)
            .no_actions()
            .back(self.dialog_back(cx))
            .into_any_element()
    }
}
