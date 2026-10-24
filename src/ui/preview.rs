use super::*;
#[cfg(feature = "spine")]
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewBackground {
    White,
    Black,
    Transparent,
}

pub(super) fn fitted_bounds(
    view: Bounds<Pixels>,
    width: f32,
    height: f32,
    zoom: f32,
    pan: Point<Pixels>,
) -> Bounds<Pixels> {
    let scale =
        (f32::from(view.size.width) / width).min(f32::from(view.size.height) / height) * zoom;
    let extent = size(px(width * scale), px(height * scale));
    Bounds::new(
        point(
            view.origin.x + (view.size.width - extent.width) * 0.5 + pan.x,
            view.origin.y + (view.size.height - extent.height) * 0.5 + pan.y,
        ),
        extent,
    )
}
impl Gallery {
    fn active_preview_background(&self) -> PreviewBackground {
        self.preview_background.unwrap_or(if self.dark {
            PreviewBackground::Black
        } else {
            PreviewBackground::White
        })
    }
    pub(super) fn preview_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .items_center()
            .child(
                Button::new("zoom-fit")
                    .ghost()
                    .small()
                    .label("适应")
                    .on_click(cx.listener(|this, _, _, cx| this.reset_preview(cx))),
            )
            .child(
                Button::new("zoom-minus")
                    .ghost()
                    .small()
                    .icon(IconName::Minus)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preview_scale = (this.preview_scale / 1.25).max(0.1);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .w(px(48.))
                    .text_xs()
                    .text_center()
                    .child(format!("{:.0}%", self.preview_scale * 100.)),
            )
            .child(
                Button::new("zoom-plus")
                    .ghost()
                    .small()
                    .icon(IconName::Plus)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.preview_scale = (this.preview_scale * 1.25).min(8.);
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .w(px(1.))
                    .h(px(18.))
                    .mx_1()
                    .bg(Palette::new(self.dark).border),
            )
            .children(
                [
                    (PreviewBackground::White, "白色背景", IconName::Sun),
                    (PreviewBackground::Black, "黑色背景", IconName::Moon),
                    (
                        PreviewBackground::Transparent,
                        "透明背景",
                        IconName::Grid2x2,
                    ),
                ]
                .into_iter()
                .enumerate()
                .map(|(i, (background, label, icon))| {
                    Button::new(format!("preview-background-{i}"))
                        .outline()
                        .small()
                        .w(px(28.))
                        .h(px(28.))
                        .icon(icon)
                        .tooltip(label)
                        .selected(self.active_preview_background() == background)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.preview_background = Some(background);
                            cx.notify();
                        }))
                }),
            )
            .into_any_element()
    }
    #[cfg(feature = "spine")]
    pub(super) fn animation_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let actions = self.animation_actions.clone();
        let selected = self.animation_action.clone();
        let entity = cx.entity().downgrade();
        Button::new("animation-actions")
            .outline()
            .small()
            .w(px(170.))
            .label(if selected.is_empty() {
                "动作".into()
            } else {
                crate::text::animation_name(&selected)
            })
            .icon(IconName::ChevronDown)
            .disabled(actions.is_empty())
            .dropdown_menu(move |mut menu, _, _| {
                for name in &actions {
                    let action = name.clone();
                    let entity = entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(crate::text::animation_name(name))
                            .checked(name == &selected)
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |this, cx| {
                                    if let Some(player) = this.animation_player.as_mut() {
                                        match player.set_animation(&action) {
                                            Ok(()) => {
                                                this.animation_action = action.clone();
                                                this.reset_preview(cx);
                                            }
                                            Err(error) => {
                                                this.animation_error =
                                                    Some(format!("动作切换失败：{error:#}"))
                                            }
                                        }
                                        cx.notify();
                                    }
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
    pub(super) fn preview(
        &self,
        path: Option<PathBuf>,
        _p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let frame = self
            .animation_frame
            .clone()
            .filter(|_| self.animation_enabled);
        let zoom = self.preview_scale;
        let pan = self.preview_pan;
        let background = self.active_preview_background();
        div()
            .id("art-preview")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .size_full()
            .min_h_0()
            .bg(match background {
                PreviewBackground::Black => rgb(0x000000),
                _ => rgb(0xffffff),
            })
            .overflow_hidden()
            .cursor(CursorStyle::OpenHand)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = f32::from(event.delta.pixel_delta(px(20.)).y);
                this.preview_scale = (this.preview_scale * (delta * 0.003).exp()).clamp(0.1, 8.);
                cx.stop_propagation();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.preview_drag = Some(event.position);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.preview_drag = None),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.dragging() {
                    if let Some(previous) = this.preview_drag {
                        this.preview_pan += event.position - previous;
                        this.preview_drag = Some(event.position);
                        cx.notify();
                    }
                } else {
                    this.preview_drag = None;
                }
            }))
            .on_double_click(cx.listener(|this, _, _, cx| {
                this.reset_preview(cx);
            }))
            .child(
                canvas(
                    move |_, window, cx| {
                        frame.clone().or_else(|| {
                            path.as_ref().and_then(|path| {
                                window
                                    .use_asset::<ImageAssetLoader>(
                                        &Resource::from(path.clone()),
                                        cx,
                                    )
                                    .and_then(Result::ok)
                            })
                        })
                    },
                    move |bounds, frame, window, _| {
                        if background == PreviewBackground::Transparent {
                            let width = f32::from(bounds.size.width);
                            let height = f32::from(bounds.size.height);
                            for row in 0..(height / 20.).ceil() as u32 {
                                for column in 0..(width / 20.).ceil() as u32 {
                                    if (row + column) % 2 == 0 {
                                        let x = column as f32 * 20.;
                                        let y = row as f32 * 20.;
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                bounds.origin + point(px(x), px(y)),
                                                size(
                                                    px((width - x).min(20.)),
                                                    px((height - y).min(20.)),
                                                ),
                                            ),
                                            rgb(0xe4e7eb),
                                        ));
                                    }
                                }
                            }
                        }
                        if let Some(frame) = frame {
                            let extent = frame.size(0);
                            let dest = fitted_bounds(
                                bounds,
                                extent.width.0 as f32,
                                extent.height.0 as f32,
                                zoom,
                                pan,
                            );
                            if let Err(error) = window.paint_image(
                                bounds,
                                dest,
                                Corners::default(),
                                frame,
                                0,
                                false,
                            ) {
                                crate::diagnostics::record(format!(
                                    "preview paint failed: {error:#}"
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
    pub(super) fn reset_preview(&mut self, cx: &mut Context<Self>) {
        self.preview_scale = 1.;
        self.preview_pan = point(px(0.), px(0.));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::fitted_bounds;
    use gpui_kit::{Bounds, point, px, size};
    #[test]
    fn fit_keeps_tall_and_wide_images_inside_viewport() {
        let view = Bounds::new(point(px(10.), px(20.)), size(px(600.), px(400.)));
        for (w, h) in [(1000., 3000.), (3000., 1000.)] {
            let result = fitted_bounds(view, w, h, 1., point(px(0.), px(0.)));
            assert!(
                result.origin.x + px(0.01) >= view.origin.x
                    && result.origin.y + px(0.01) >= view.origin.y
            );
            assert!(
                result.origin.x + result.size.width <= view.origin.x + view.size.width + px(0.01)
            );
            assert!(
                result.origin.y + result.size.height <= view.origin.y + view.size.height + px(0.01)
            );
        }
    }
}
