use super::form_dialog::FormDialog;
use super::*;

#[derive(Clone, Copy, PartialEq)]
enum ReportKind {
    Gifts,
    Owned,
}
impl ReportKind {
    fn label(self) -> &'static str {
        match self {
            Self::Gifts => "角色喜好表",
            Self::Owned => "已拥有清单",
        }
    }
    fn filename(self, png: bool) -> &'static str {
        match (self, png) {
            (Self::Gifts, false) => "雀魂角色喜好表.html",
            (Self::Owned, false) => "雀魂已拥有清单.html",
            (Self::Gifts, true) => "雀魂角色喜好表.png",
            (Self::Owned, true) => "雀魂已拥有清单.png",
        }
    }
}
pub(super) struct ExportState {
    options: crate::report::GiftOptions,
    active: Option<ReportKind>,
    result: Option<String>,
    owned_scope: [bool; 3],
    png: bool,
}
impl Default for ExportState {
    fn default() -> Self {
        Self {
            options: Default::default(),
            active: None,
            result: None,
            owned_scope: [true; 3],
            png: false,
        }
    }
}
impl Gallery {
    pub(super) fn export_dialog(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let options = &self.export_state.options;
        let busy = self.export_busy();
        let export_button = |id: &str, kind: ReportKind, disabled: bool| {
            Button::new(SharedString::from(id.to_owned()))
                .outline()
                .small()
                .w(px(32.))
                .h(px(32.))
                .icon(IconName::Download)
                .tooltip(format!("导出{}", kind.label()))
                .disabled(busy || disabled)
                .on_click(cx.listener(move |this, _, _, cx| this.export_report(kind, cx)))
        };
        let body = v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child("格式")
                    .child(
                        choice_group("report-format").children(
                            [(false, "HTML · 可搜索"), (true, "PNG · 紧凑图表")]
                                .into_iter()
                                .map(|(png, label)| {
                                    segment_button(
                                        format!("report-png-{png}"),
                                        label,
                                        self.export_state.png == png,
                                        p,
                                    )
                                    .disabled(busy)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            this.export_state.png = png;
                                            cx.notify();
                                        },
                                    ))
                                }),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("角色喜好表"))
                    .child(export_button(
                        "export-gifts",
                        ReportKind::Gifts,
                        !(options.sakura || options.bamboo)
                            || !(options.permanent || options.limited),
                    )),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(p.muted)
                    .child("紫色礼物居中，樱花与竹林分列两侧；限定角色用金色边框标记。"),
            )
            .child(
                h_flex().gap_2().children(
                    [
                        (0, text::SAKURA, options.sakura),
                        (1, text::BAMBOO, options.bamboo),
                        (2, text::STANDARD, options.permanent),
                        (3, text::LIMITED, options.limited),
                    ]
                    .into_iter()
                    .map(|(id, label, selected)| {
                        filter_chip(format!("export-scope-{id}"), label, selected, p)
                            .icon(IconName::Check)
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let o = &mut this.export_state.options;
                                match id {
                                    0 => o.sakura = !o.sakura,
                                    1 => o.bamboo = !o.bamboo,
                                    2 => o.permanent = !o.permanent,
                                    _ => o.limited = !o.limited,
                                };
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(div().h(px(1.)).bg(p.border).my_2())
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("已拥有清单"))
                    .child(export_button(
                        "export-owned",
                        ReportKind::Owned,
                        !self.account.is_connected()
                            || !self.export_state.owned_scope.iter().any(|v| *v),
                    )),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(p.muted)
                    .child(if !self.account.is_connected() {
                        "登录后可导出已拥有清单。"
                    } else if self.account.snapshot.is_demo {
                        "当前使用示例账号，导出清单会标注调试预览。"
                    } else {
                        "按当前账号的收藏状态导出角色、皮肤和装扮。"
                    }),
            )
            .child(
                h_flex().gap_2().children(
                    [text::CHARACTERS, text::SKINS, text::DECORATIONS]
                        .into_iter()
                        .enumerate()
                        .map(|(i, label)| {
                            filter_chip(
                                format!("owned-export-{i}"),
                                label,
                                self.export_state.owned_scope[i],
                                p,
                            )
                            .icon(IconName::Check)
                            .disabled(busy)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.export_state.owned_scope[i] =
                                        !this.export_state.owned_scope[i];
                                    cx.notify();
                                },
                            ))
                        }),
                ),
            );
        let logs = v_flex()
            .gap_2()
            .child(div().text_xs().text_color(p.muted).child("导出日志"))
            .when(busy, |body| {
                body.child(
                    Progress::new("share-progress")
                        .loading(true)
                        .color(p.accent)
                        .small(),
                )
            })
            .when(self.export_state.result.is_none() && !busy, |body| {
                body.child(
                    div()
                        .text_xs()
                        .text_color(p.muted)
                        .child("导出结果会显示在这里"),
                )
            })
            .children(self.export_state.result.iter().enumerate().map(|(i, log)| {
                let tooltip = log.clone();
                div()
                    .id(("export-log", i))
                    .text_xs()
                    .text_color(p.muted)
                    .truncate()
                    .child(log.clone())
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(tooltip.clone())
                            .build(window, cx)
                    })
            }));
        FormDialog::new(
            "导出图表与清单",
            "导出可搜索的 HTML，或适合保存分享的紧凑 PNG 图表。",
            body,
            logs,
            p,
        )
        .back(self.dialog_back(cx))
        .into_any_element()
    }
    fn export_report(&mut self, kind: ReportKind, cx: &mut Context<Self>) {
        if self.export_busy() || (kind == ReportKind::Owned && !self.account.is_connected()) {
            return;
        }
        self.export_state.active = Some(kind);
        self.export_state.result = None;
        let library = self.library.clone();
        let account = self.account.snapshot.clone();
        let gifts = self.export_state.options.clone();
        let scope = self.export_state.owned_scope;
        let png = self.export_state.png;
        cx.spawn(async move |this, cx| {
            let result = if let Some(file) = rfd::AsyncFileDialog::new()
                .set_title(format!("导出{}", kind.label()))
                .add_filter(
                    if png { "PNG 图片" } else { "HTML 网页" },
                    &[if png { "png" } else { "html" }],
                )
                .set_file_name(kind.filename(png))
                .save_file()
                .await
            {
                let path = file.path().to_owned();
                Some(
                    cx.background_executor()
                        .spawn(async move {
                            let result = match kind {
                                ReportKind::Gifts => {
                                    crate::report::write_gifts(&library, &gifts, &path)
                                }
                                ReportKind::Owned => crate::report::write_owned(
                                    &library,
                                    &account,
                                    &crate::report::OwnedOptions {
                                        characters: scope[0],
                                        skins: scope[1],
                                        decorations: scope[2],
                                    },
                                    &path,
                                ),
                            };
                            result.map(|_| path)
                        })
                        .await,
                )
            } else {
                None
            };
            let _ = this.update(cx, |this, cx| {
                this.export_state.active = None;
                let message = match result {
                    Some(Ok(path)) => {
                        format!("{} · 已导出：{}", kind.label(), model::display_path(&path))
                    }
                    Some(Err(error)) => format!("{} · 导出失败：{error:#}", kind.label()),
                    None => format!("{} · 已取消", kind.label()),
                };
                this.export_state.result = Some(message);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    pub(super) fn export_busy(&self) -> bool {
        self.export_state.active.is_some()
    }
}
