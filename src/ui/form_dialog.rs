use super::*;

/// Shared modal shell: fixed header/actions, bounded and scrollable form content.
/// Callers supply their own fields, progress display and actions.
#[derive(IntoElement)]
pub(super) struct FormDialog {
    title: SharedString,
    description: SharedString,
    body: AnyElement,
    actions: Option<AnyElement>,
    back: Option<Button>,
    palette: Palette,
}

impl FormDialog {
    pub fn new(
        title: impl Into<SharedString>,
        description: impl Into<SharedString>,
        body: impl IntoElement,
        actions: impl IntoElement,
        palette: Palette,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            body: body.into_any_element(),
            actions: Some(actions.into_any_element()),
            back: None,
            palette,
        }
    }
    pub fn back(mut self, button: Button) -> Self {
        self.back = Some(button);
        self
    }
    pub fn no_actions(mut self) -> Self {
        self.actions = None;
        self
    }
}

impl RenderOnce for FormDialog {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let p = self.palette;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .child(
                v_flex()
                    .w(px(520.))
                    .max_w_full()
                    .max_h(px(
                        (f32::from(window.viewport_size().height) - 100.).max(300.)
                    ))
                    .min_h_0()
                    .rounded_xl()
                    .border_1()
                    .border_color(p.border)
                    .bg(p.panel)
                    .shadow_lg()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        v_flex()
                            .p_5()
                            .gap_2()
                            .flex_shrink_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .when_some(self.back, |row, button| row.child(button))
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(self.title),
                                    ),
                            )
                            .child(div().text_sm().text_color(p.muted).child(self.description)),
                    )
                    .child(
                        div()
                            .id("modal-form-content")
                            .max_h(px(
                                (f32::from(window.viewport_size().height) - 260.).max(140.)
                            ))
                            .px_5()
                            .pb_5()
                            .min_h_0()
                            .child(self.body)
                            .overflow_y_scroll(),
                    )
                    .when_some(self.actions, |panel, actions| {
                        panel.child(
                            div()
                                .px_5()
                                .py_3()
                                .border_t_1()
                                .border_color(p.border)
                                .flex_shrink_0()
                                .child(actions),
                        )
                    }),
            )
    }
}

/// Standard task body for any modal operation; progress details remain selectable by the caller.
pub(super) fn progress_body(
    label: String,
    progress: &crate::task_process::Progress,
    indeterminate: bool,
    p: Palette,
) -> AnyElement {
    v_flex()
        .w_full()
        .gap_3()
        .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
        .child(
            Progress::new("modal-progress")
                .value(progress.percent)
                .loading(indeterminate || progress.total == 0)
                .color(p.accent),
        )
        .child(
            h_flex()
                .justify_between()
                .text_xs()
                .text_color(p.muted)
                .child(if progress.total > 0 {
                    format!("{} / {}", progress.done, progress.total)
                } else {
                    "请稍候".into()
                })
                .child(if indeterminate {
                    String::new()
                } else {
                    format!("{:.0}%", progress.percent)
                }),
        )
        .when(!progress.current.is_empty(), |body| {
            body.child(
                div()
                    .w_full()
                    .h(px(20.))
                    .flex_shrink_0()
                    .truncate()
                    .text_xs()
                    .text_color(p.muted)
                    .overflow_hidden()
                    .child(progress.current.clone()),
            )
        })
        .into_any_element()
}
