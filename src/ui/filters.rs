//! Shared collection/training controls and character filters.
use super::*;

pub(super) fn controls_panel(p: Palette) -> Div {
    v_flex()
        .w_full()
        .p_2()
        .gap_2()
        .bg(p.panel)
        .border_1()
        .border_color(p.border)
        .rounded_lg()
}
pub(super) fn controls_row() -> Div {
    h_flex().h(px(32.)).flex_shrink_0().items_center().gap_3()
}

impl Gallery {
    pub(super) fn character_matches(&self, c: &Character) -> bool {
        (self.character_sex == 0 || c.sex == self.character_sex)
            && self
                .character_limited
                .is_none_or(|limited| c.limited == limited)
            && (!self.character_collaboration || c.collaboration)
            && (!self.account.is_connected()
                || !self.only_favorites
                || self
                    .account
                    .snapshot
                    .characters
                    .get(&c.id)
                    .is_some_and(|s| s.favorite))
    }
    pub(super) fn account_filters(
        &self,
        characters: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        h_flex()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .when(characters, |row| {
                row.child(self.character_filters(p, cx))
                    .when(self.account.is_connected(), |row| {
                        row.child(
                            filter_chip("favorites-only", text::FAVORITE, self.only_favorites, p)
                                .icon(IconName::Star)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.only_favorites = !this.only_favorites;
                                    cx.notify();
                                })),
                        )
                    })
            })
            .when(self.account.is_connected(), |row| {
                row.child(
                    filter_chip("owned-filter", text::OWNED, self.only_owned, p)
                        .icon(IconName::Check)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.only_owned = !this.only_owned;
                            cx.notify();
                        })),
                )
            })
            .when(characters && self.account.is_connected(), |row| {
                use crate::account::CharacterOrder;
                use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
                let selected = self.character_order;
                let entity = cx.entity().downgrade();
                row.child(
                    Button::new("character-order")
                        .outline()
                        .small()
                        .w(px(96.))
                        .label(match selected {
                            CharacterOrder::Game => "寮舍顺序",
                            CharacterOrder::Release => "默认顺序",
                            CharacterOrder::Bond => "好感度",
                        })
                        .icon(IconName::ChevronDown)
                        .dropdown_menu(move |mut menu, _, _| {
                            for (order, label) in [
                                (CharacterOrder::Game, "寮舍顺序"),
                                (CharacterOrder::Release, "默认顺序"),
                                (CharacterOrder::Bond, "好感度"),
                            ] {
                                let entity = entity.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(label)
                                        .checked(order == selected)
                                        .on_click(move |_, _, cx| {
                                            let _ = entity.update(cx, |this, cx| {
                                                this.character_order = order;
                                                this.collection_key.clear();
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            menu
                        }),
                )
            })
            .child(
                filter_chip(
                    "sort-direction",
                    if self.descending { "倒序" } else { "顺序" },
                    self.descending,
                    p,
                )
                .icon(if self.descending {
                    IconName::ArrowDown
                } else {
                    IconName::ArrowUp
                })
                .w(px(72.))
                .tooltip("反转当前排序")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.descending = !this.descending;
                    cx.notify();
                })),
            )
    }
    pub(super) fn filter_controls(
        &self,
        training: bool,
        characters: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        controls_row()
            .w_full()
            .when(!training && self.category == Category::Skins, |row| {
                row.child(
                    ButtonGroup::new("skin-art-mode")
                        .outline()
                        .compact()
                        .small()
                        .children(
                            [
                                (false, "头像", IconName::UserRound),
                                (true, "立绘", IconName::Image),
                            ]
                            .into_iter()
                            .map(|(full, label, icon)| {
                                segment_button(
                                    format!("skin-full-art-{full}"),
                                    label,
                                    self.skin_full_art == full,
                                    p,
                                )
                                .icon(icon)
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.skin_full_art = full;
                                        cx.notify();
                                    },
                                ))
                            }),
                        ),
                )
                .when(cfg!(feature = "spine"), |row| {
                    row.child(
                        filter_chip("animated-skins-only", "仅动态", self.only_animated_skins, p)
                            .icon(IconName::Play)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.only_animated_skins = !this.only_animated_skins;
                                cx.notify();
                            })),
                    )
                })
            })
            .when(training, |row| {
                row.child(
                    ButtonGroup::new("contract-filter")
                        .outline()
                        .compact()
                        .small()
                        .children(
                            [
                                (ContractFilter::All, "全部"),
                                (ContractFilter::Contracted, "已契约"),
                                (ContractFilter::Uncontracted, "未契约"),
                                (ContractFilter::Ready, text::CONTRACT_READY),
                                (ContractFilter::Full, text::FULL_BOND),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(i, (filter, label))| {
                                segment_button(
                                    format!("contract-{i}"),
                                    label,
                                    self.contract_filter == filter,
                                    p,
                                )
                                .disabled(!self.account.is_connected())
                                .when(filter == ContractFilter::Full, |b| {
                                    b.tooltip("累计好感达到 88,000，包含已契约角色")
                                })
                                .when(filter == ContractFilter::Ready, |b| {
                                    b.tooltip("已拥有、未契约、满好感且契约材料齐备")
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.contract_filter = filter;
                                        cx.notify();
                                    },
                                ))
                            }),
                        ),
                )
            })
            .child(div().flex_1())
            .child(self.account_filters(characters, p, cx))
    }
    pub(super) fn character_filters(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_2()
            .items_center()
            .flex_shrink_0()
            .child(
                ButtonGroup::new("sex-filter")
                    .outline()
                    .compact()
                    .small()
                    .children(
                        [(0, text::ALL), (1, text::SAKURA), (2, text::BAMBOO)]
                            .into_iter()
                            .map(|(sex, label)| {
                                Button::new(format!("sex-{sex}"))
                                    .label(label)
                                    .w(px(40.))
                                    .px_1()
                                    .selected(self.character_sex == sex)
                                    .when(self.character_sex == sex, |b| {
                                        b.bg(p.accent_bg).text_color(p.accent)
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.character_sex = sex;
                                        cx.notify();
                                    }))
                            }),
                    ),
            )
            .child(
                ButtonGroup::new("limited-filter")
                    .outline()
                    .compact()
                    .small()
                    .children(
                        [
                            (None, false, text::ALL),
                            (Some(false), false, text::STANDARD),
                            (Some(true), false, text::LIMITED),
                            (Some(true), true, "联动"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (limited, collaboration, label))| {
                            Button::new(format!("limited-{i}"))
                                .label(label)
                                .w(px(40.))
                                .px_1()
                                .selected(
                                    self.character_limited == limited
                                        && self.character_collaboration == collaboration,
                                )
                                .when(
                                    self.character_limited == limited
                                        && self.character_collaboration == collaboration,
                                    |b| b.bg(p.accent_bg).text_color(p.accent),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.character_limited = limited;
                                    this.character_collaboration = collaboration;
                                    cx.notify();
                                }))
                        }),
                    ),
            )
            .into_any_element()
    }
}
