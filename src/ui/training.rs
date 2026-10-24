use super::*;

impl Gallery {
    fn training_controls(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        controls_panel(p)
            .child(
                controls_row()
                    .justify_end()
                    .child(
                        ButtonGroup::new("bond-view")
                            .outline()
                            .compact()
                            .small()
                            .children(
                                [
                                    (true, "爱心", IconName::Heart),
                                    (false, "进度", IconName::Gauge),
                                ]
                                .into_iter()
                                .map(|(hearts, label, icon)| {
                                    segment_button(
                                        format!("bond-view-{hearts}"),
                                        label,
                                        self.bond_hearts == hearts,
                                        p,
                                    )
                                    .icon(icon)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.bond_hearts = hearts;
                                            cx.notify();
                                        },
                                    ))
                                }),
                            ),
                    )
                    .child(
                        ButtonGroup::new("training-columns")
                            .outline()
                            .compact()
                            .small()
                            .children(
                                [
                                    (1, "一列", IconName::Rows2),
                                    (2, "两列", IconName::Columns2),
                                    (3, "三列", IconName::Columns3),
                                ]
                                .into_iter()
                                .map(|(columns, label, icon)| {
                                    segment_button(
                                        format!("training-columns-{columns}"),
                                        label,
                                        self.training_columns == columns,
                                        p,
                                    )
                                    .icon(icon)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.training_columns = columns;
                                            cx.notify();
                                        },
                                    ))
                                }),
                            ),
                    ),
            )
            .child(div().h(px(1.)).w_full().bg(p.border))
            .child(self.filter_controls(true, true, p, cx))
    }
    pub(super) fn training(
        &mut self,
        _window: &mut Window,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.account.is_connected() {
            return v_flex()
                .size_full()
                .gap_3()
                .child(self.training_controls(p, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child(Icon::new(IconName::Heart).size_8().text_color(p.muted))
                        .child(div().text_lg().child("登录后查看培养计划"))
                        .child(
                            div()
                                .text_sm()
                                .text_color(p.muted)
                                .child("通过左下角登录账号，查看好感、契约与材料库存。"),
                        ),
                )
                .into_any_element();
        }
        let query = crate::search::normalize(self.search.read(cx).value().as_ref());
        let mut eligible = self
            .library
            .characters
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                let state = self.account.snapshot.character(&c.id);
                (!self.only_owned || state.owned)
                    && self.character_matches(c)
                    && c.search.matches(&query)
                    && match self.contract_filter {
                        ContractFilter::All => true,
                        ContractFilter::Contracted => state.contracted,
                        ContractFilter::Uncontracted => !state.contracted,
                        ContractFilter::Full => state.bond >= MAX_BOND,
                        ContractFilter::Ready => self.account.snapshot.can_contract(c),
                    }
            })
            .collect::<Vec<_>>();
        self.account
            .snapshot
            .sort_characters(&mut eligible, self.character_order, self.descending);
        let columns = self.training_columns;
        let compact = columns == 3;
        let total = eligible.len();
        let body = v_flex()
            .w_full()
            .h_full()
            .min_h_0()
            .gap_3()
            .child(self.training_controls(p, cx).flex_shrink_0());
        let items = eligible;
        let mut grid = v_flex().gap_2();
        for row in items.chunks(columns) {
            grid = grid.child(
                h_flex()
                    .gap_3()
                    .items_stretch()
                    .children(row.iter().map(|&(index, c)| {
                        let state = self.account.snapshot.character(&c.id);
                        let bond = BondProgress::new(state.bond);
                        v_flex()
                            .id(("training", index))
                            .flex_1()
                            .min_w_0()
                            .min_h(px(78.))
                            .p_2()
                            .gap_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(p.border)
                            .bg(if state.owned { p.panel } else { p.bg })
                            .cursor_pointer()
                            .hover(move |s| s.border_color(p.accent))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.open_character(index, cx)),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        div()
                                            .size(px(28.))
                                            .flex_shrink_0()
                                            .rounded_md()
                                            .overflow_hidden()
                                            .child(picture(c.thumbnail(), 28., p)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(c.name.clone()),
                                    )
                                    .when(state.favorite, |row| {
                                        row.child(self.favorite_indicator(&c.id, p))
                                    })
                                    .when(!state.owned || state.contracted, |row| {
                                        row.child(collection_status(
                                            state.owned,
                                            state.contracted,
                                            p,
                                        ))
                                    }),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .justify_between()
                                    .flex_wrap()
                                    .when(self.bond_hearts, |row| {
                                        row.child(
                                            h_flex()
                                                .gap_2()
                                                .items_center()
                                                .flex_shrink_0()
                                                .child(super::bond::heart_row(
                                                    format!("training-hearts-{index}"),
                                                    state.bond,
                                                    if compact { 14. } else { 16. },
                                                    p,
                                                ))
                                                .child(div().text_xs().text_color(p.muted).child(
                                                    format!(
                                                        "{} / {}",
                                                        number(bond.total),
                                                        number(MAX_BOND)
                                                    ),
                                                )),
                                        )
                                    })
                                    .when(!self.bond_hearts, |row| {
                                        row.child(
                                            v_flex()
                                                .flex_1()
                                                .min_w(px(140.))
                                                .max_w(px(260.))
                                                .gap_1()
                                                .child(div().text_xs().text_color(p.muted).child(
                                                    format!(
                                                        "{} / {}",
                                                        number(bond.total),
                                                        number(MAX_BOND)
                                                    ),
                                                ))
                                                .child(
                                                    Progress::new(("bond-row", index))
                                                        .value(bond.percent())
                                                        .color(p.accent)
                                                        .small(),
                                                ),
                                        )
                                    })
                                    .child(
                                        h_flex().gap_1().items_center().flex_shrink_0().children(
                                            c.materials.iter().enumerate().filter_map(|(i, g)| {
                                                let item = g.display_item()?;
                                                let available = self.account.snapshot.available(g);
                                                Some(material_icon(
                                                    format!("training-material-{index}-{i}"),
                                                    item.icon.clone(),
                                                    if compact { 20. } else { 24. },
                                                    material_color(
                                                        state.contracted,
                                                        available,
                                                        g.quantity,
                                                        p,
                                                    ),
                                                    format!(
                                                        "{} · {} · 已有 {} / 需要 {}",
                                                        if state.contracted {
                                                            text::CONTRACTED
                                                        } else if available >= g.quantity {
                                                            "材料足够"
                                                        } else {
                                                            "材料不足"
                                                        },
                                                        item.name,
                                                        available,
                                                        g.quantity
                                                    ),
                                                    p,
                                                ))
                                            }),
                                        ),
                                    ),
                            )
                    }))
                    .children((row.len()..columns).map(|_| div().flex_1())),
            );
        }
        if total == 0 {
            grid = grid.child(self.empty(
                "暂无符合条件的角色",
                "调整筛选，或同步雀魂内收藏后查看。",
                p,
            ));
        }
        body.child(browsing::scroll_content(
            "training-scroll",
            &self.training_scroll,
            grid,
        ))
        .into_any_element()
    }
}
