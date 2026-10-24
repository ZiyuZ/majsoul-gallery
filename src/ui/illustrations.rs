//! Illustration cards and full-resolution viewing share the collection and preview components.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Filter {
    #[default]
    All,
    Year(u16),
    Guardians,
}

impl Filter {
    pub fn matches(self, item: &model::Illustration) -> bool {
        match self {
            Self::All => true,
            Self::Year(year) => item.year() == Some(year),
            Self::Guardians => item.is_guardian(),
        }
    }
}

#[derive(Default)]
pub(super) struct ExportState {
    busy: bool,
    message: Option<String>,
}

impl Gallery {
    pub(super) fn illustration_filters(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let years = self
            .library
            .illustrations
            .iter()
            .filter_map(model::Illustration::year)
            .collect::<std::collections::BTreeSet<_>>();
        let filters = std::iter::once((Filter::All, text::ALL.to_string()))
            .chain(
                years
                    .into_iter()
                    .map(|year| (Filter::Year(year), year.to_string())),
            )
            .chain(std::iter::once((
                Filter::Guardians,
                "守护者系列".to_string(),
            )));
        div()
            .id("illustration-filters")
            .flex()
            .items_center()
            .min_w_0()
            .h_full()
            .flex_shrink_1()
            .child(
                choice_group("illustration-series").children(filters.enumerate().map(
                    |(i, (filter, label))| {
                        segment_button(
                            format!("illustration-filter-{i}"),
                            label,
                            self.illustration_filter == filter,
                            p,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.illustration_filter = filter;
                            cx.notify();
                        }))
                    },
                )),
            )
            .overflow_x_scrollbar()
            .into_any_element()
    }
    pub(super) fn illustration_card(
        &self,
        index: usize,
        item: &model::Illustration,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connected = self.account.is_connected();
        let owned = self.account.snapshot.owns_illustration(item);
        collection_card(
            format!("illustration-{}", item.id),
            width,
            connected,
            owned,
            p,
        )
        .cursor_pointer()
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selected = None;
            this.illustration = Some(index);
            this.illustration_export.message = None;
            this.reset_preview(cx);
        }))
        .child(super::cards::collection_cover(
            item.thumbnail.clone(),
            width - 2.,
            (width - 2.) * 9. / 16.,
            p,
        ))
        .child(
            v_flex()
                .p_3()
                .gap_2()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(item.name.clone()),
                )
                .child(collection_meta(
                    div().child(text::ILLUSTRATIONS),
                    connected.then(|| collection_status(owned, false, p)),
                    p,
                )),
        )
        .into_any_element()
    }

    pub(super) fn illustration_detail(
        &self,
        index: usize,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(item) = self.library.illustrations.get(index) else {
            return self.empty("找不到插画", "请返回收藏重新选择。", p);
        };
        let owned = self.account.snapshot.owns_illustration(item);
        v_flex()
            .size_full()
            .min_h_0()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        back_icon_button("illustration-back", "返回收藏").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.illustration = None;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(item.name.clone()),
                    )
                    .when(self.account.is_connected(), |row| {
                        row.child(collection_status(owned, false, p))
                    })
                    .child(
                        Button::new("export-illustration")
                            .outline()
                            .small()
                            .icon(IconName::Download)
                            .tooltip("导出插画原图（PNG）")
                            .disabled(item.image.is_none() || self.illustration_export.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.export_illustration(cx))),
                    )
                    .child(self.preview_controls(cx)),
            )
            .when_some(self.illustration_export.message.clone(), |body, message| {
                body.child(
                    div()
                        .text_xs()
                        .text_color(p.muted)
                        .truncate()
                        .child(message),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .rounded_lg()
                    .border_1()
                    .border_color(p.border)
                    .overflow_hidden()
                    .child(self.preview(item.image.clone(), p, cx)),
            )
            .into_any_element()
    }

    fn export_illustration(&mut self, cx: &mut Context<Self>) {
        if self.illustration_export.busy {
            return;
        }
        let Some(item) = self
            .illustration
            .and_then(|index| self.library.illustrations.get(index))
        else {
            return;
        };
        let Some(source) = item.image.clone() else {
            return;
        };
        let name = item
            .name
            .replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_");
        self.illustration_export.busy = true;
        self.illustration_export.message = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title("导出插画原图")
                .add_filter("PNG", &["png"])
                .set_file_name(format!("{name}.png"))
                .save_file()
                .await;
            let result = if let Some(file) = file {
                let output = file.path().with_extension("png");
                Some(
                    cx.background_executor()
                        .spawn(async move {
                            let bytes = std::fs::read(&source)?;
                            crate::files::write_atomic(&output, &bytes)?;
                            anyhow::Ok(model::display_path(&output))
                        })
                        .await,
                )
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                this.illustration_export.busy = false;
                this.illustration_export.message = result.map(|result| match result {
                    Ok(path) => format!("已导出：{path}"),
                    Err(error) => format!("导出失败：{error:#}"),
                });
                cx.notify();
            });
        })
        .detach();
    }
}
