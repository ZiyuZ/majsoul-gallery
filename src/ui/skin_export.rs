//! Skin export form and isolated task progress.
use super::form_dialog::FormDialog;
use super::*;
use crate::skin_export::{ExportJob, Format, Request};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

pub(super) struct State {
    animated: bool,
    format: Format,
    long_edge: u32,
    fps: u32,
    seconds: u32,
    white_background: bool,
    lossy: bool,
    action: String,
    actions: Vec<String>,
    #[cfg(feature = "spine")]
    metadata_player: Option<crate::animation::Player>,
    job: Option<ExportJob>,
    picking: bool,
    generation: u64,
    progress: crate::importer::Progress,
    message: Option<String>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            animated: false,
            format: Format::Png,
            long_edge: 1024,
            fps: 30,
            seconds: 0,
            white_background: false,
            lossy: false,
            action: String::new(),
            actions: Vec::new(),
            #[cfg(feature = "spine")]
            metadata_player: None,
            job: None,
            picking: false,
            generation: 0,
            progress: Default::default(),
            message: None,
        }
    }
}
impl Gallery {
    pub(super) fn open_skin_export(&mut self, cx: &mut Context<Self>) {
        let animated = cfg!(feature = "spine")
            && self.animation_enabled
            && self
                .selected
                .and_then(|i| self.library.characters[i].skins.get(self.skin))
                .is_some_and(|s| s.scene.is_some());
        let generation = self.skin_export.generation + 1;
        self.skin_export = State {
            animated,
            format: if animated { Format::Webp } else { Format::Png },
            action: self.animation_action.clone(),
            actions: self.animation_actions.clone(),
            generation,
            ..Default::default()
        };
        self.modal = Some(Modal::SkinExport);
        #[cfg(feature = "spine")]
        if animated {
            self.load_skin_export_actions(cx);
        }
        cx.notify();
    }
    pub(super) fn cancel_skin_export(&mut self) {
        self.skin_export.generation += 1;
        self.skin_export.job = None;
        #[cfg(feature = "spine")]
        {
            self.skin_export.metadata_player = None;
        }
        self.skin_export.picking = false;
        self.skin_export.message = Some("已取消导出".into());
    }
    fn skin_export_busy(&self) -> bool {
        self.skin_export.job.is_some() || self.skin_export.picking
    }

    pub(super) fn skin_export_dialog(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(character) = self.selected.and_then(|i| self.library.characters.get(i)) else {
            return div().into_any_element();
        };
        let Some(skin) = character.skins.get(self.skin) else {
            return div().into_any_element();
        };
        let state = &self.skin_export;
        let busy = self.skin_export_busy();
        let mut body = v_flex().gap_4();
        if busy {
            body = body.child(form_dialog::progress_body(
                state.progress.label.clone(),
                &state.progress,
                state.progress.total == 0,
                p,
            ));
        } else {
            body = body
                .when(cfg!(feature = "spine"), |body| {
                    body.child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child("内容")
                            .child(
                                ButtonGroup::new("skin-export-content")
                                    .outline()
                                    .compact()
                                    .small()
                                    .children([
                                        segment_button(
                                            "skin-export-still",
                                            "静态立绘",
                                            !state.animated,
                                            p,
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.skin_export.animated = false;
                                                #[cfg(feature = "spine")]
                                                {
                                                    this.skin_export.metadata_player = None;
                                                }
                                                cx.notify();
                                            }),
                                        ),
                                        segment_button(
                                            "skin-export-animation",
                                            "动画",
                                            state.animated,
                                            p,
                                        )
                                        .disabled(skin.scene.is_none())
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.skin_export.animated = true;
                                                this.skin_export.format = Format::Webp;
                                                #[cfg(feature = "spine")]
                                                this.load_skin_export_actions(cx);
                                                cx.notify();
                                            }),
                                        ),
                                    ]),
                            ),
                    )
                })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child("格式")
                        .child(
                            ButtonGroup::new("skin-export-format")
                                .outline()
                                .compact()
                                .small()
                                .children([
                                    segment_button(
                                        "skin-export-png",
                                        "PNG",
                                        state.format == Format::Png,
                                        p,
                                    )
                                    .disabled(state.animated)
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.skin_export.format = Format::Png;
                                            cx.notify();
                                        },
                                    )),
                                    segment_button(
                                        "skin-export-webp",
                                        "WebP",
                                        state.format == Format::Webp,
                                        p,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.skin_export.format = Format::Webp;
                                            cx.notify();
                                        },
                                    )),
                                ]),
                        ),
                )
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child("背景")
                        .child(
                            ButtonGroup::new("skin-export-background")
                                .outline()
                                .compact()
                                .small()
                                .children([
                                    segment_button(
                                        "skin-export-transparent",
                                        "透明",
                                        !state.white_background,
                                        p,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.skin_export.white_background = false;
                                            cx.notify();
                                        },
                                    )),
                                    segment_button(
                                        "skin-export-white",
                                        "白色",
                                        state.white_background,
                                        p,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.skin_export.white_background = true;
                                            cx.notify();
                                        },
                                    )),
                                ]),
                        ),
                );
            if state.format == Format::Webp {
                body = body.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child("压缩")
                        .child(
                            ButtonGroup::new("webp-compression")
                                .outline()
                                .compact()
                                .small()
                                .children([(false, "无损"), (true, "有损 · 90")].into_iter().map(
                                    |(lossy, label)| {
                                        segment_button(
                                            format!("webp-lossy-{lossy}"),
                                            label,
                                            state.lossy == lossy,
                                            p,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.skin_export.lossy = lossy;
                                                cx.notify();
                                            }),
                                        )
                                    },
                                )),
                        ),
                );
            }
            if state.animated {
                let actions = state.actions.clone();
                let chosen = state.action.clone();
                let entity = cx.entity().downgrade();
                body = body
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child("动作")
                            .child(
                                Button::new("skin-export-action")
                                    .outline()
                                    .small()
                                    .w(px(190.))
                                    .icon(IconName::ChevronDown)
                                    .label(if chosen.is_empty() {
                                        "默认动作".into()
                                    } else {
                                        crate::text::animation_name(&chosen)
                                    })
                                    .disabled(actions.is_empty())
                                    .dropdown_menu(move |mut menu, _, _| {
                                        for name in &actions {
                                            let action = name.clone();
                                            let entity = entity.clone();
                                            menu = menu.item(
                                                PopupMenuItem::new(crate::text::animation_name(
                                                    name,
                                                ))
                                                .checked(name == &chosen)
                                                .on_click(move |_, _, cx| {
                                                    let _ = entity.update(cx, |this, cx| {
                                                        this.skin_export.action = action.clone();
                                                        cx.notify();
                                                    });
                                                }),
                                            );
                                        }
                                        menu
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child("长边尺寸")
                            .child(
                                ButtonGroup::new("skin-export-size")
                                    .outline()
                                    .compact()
                                    .small()
                                    .children([512, 1024, 1536, 2048].into_iter().map(|size| {
                                        segment_button(
                                            format!("skin-size-{size}"),
                                            size.to_string(),
                                            state.long_edge == size,
                                            p,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.skin_export.long_edge = size;
                                                cx.notify();
                                            }),
                                        )
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child("帧率")
                            .child(
                                ButtonGroup::new("skin-export-fps")
                                    .outline()
                                    .compact()
                                    .small()
                                    .children([24, 30, 60].into_iter().map(|fps| {
                                        segment_button(
                                            format!("skin-fps-{fps}"),
                                            format!("{fps} FPS"),
                                            state.fps == fps,
                                            p,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.skin_export.fps = fps;
                                                cx.notify();
                                            }),
                                        )
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child("时长")
                            .child(
                                ButtonGroup::new("skin-export-duration")
                                    .outline()
                                    .compact()
                                    .small()
                                    .children(
                                        [
                                            (0, "一个周期"),
                                            (5, "5 秒"),
                                            (10, "10 秒"),
                                            (15, "15 秒"),
                                        ]
                                        .into_iter()
                                        .map(
                                            |(seconds, label)| {
                                                segment_button(
                                                    format!("skin-seconds-{seconds}"),
                                                    label,
                                                    state.seconds == seconds,
                                                    p,
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.skin_export.seconds = seconds;
                                                    cx.notify();
                                                }))
                                            },
                                        ),
                                    ),
                            ),
                    );
            } else {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(p.muted)
                        .child("按原图尺寸保存静态立绘。"),
                );
            }
            body = body.when_some(state.message.clone(), |body, message| {
                body.child(div().text_xs().text_color(p.muted).child(message))
            });
        }
        let actions = h_flex().justify_end().child(if busy {
            Button::new("skin-export-cancel")
                .outline()
                .label("取消导出")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.cancel_skin_export();
                    cx.notify();
                }))
        } else {
            Button::new("skin-export-save")
                .primary()
                .icon(IconName::Download)
                .label("保存为…")
                .on_click(cx.listener(|this, _, _, cx| this.save_skin_export(cx)))
        });
        FormDialog::new(
            "导出皮肤",
            format!(
                "{} · {}",
                character.name,
                skin.display_name(&character.name)
            ),
            body,
            actions,
            p,
        )
        .back(self.dialog_back(cx))
        .into_any_element()
    }
    #[cfg(feature = "spine")]
    fn load_skin_export_actions(&mut self, cx: &mut Context<Self>) {
        if !self.skin_export.actions.is_empty() || self.skin_export.metadata_player.is_some() {
            return;
        }
        let Some(scene) = self
            .selected
            .and_then(|i| self.library.characters.get(i))
            .and_then(|c| c.skins.get(self.skin))
            .and_then(|s| s.scene.clone())
        else {
            return;
        };
        self.skin_export.metadata_player = Some(crate::animation::Player::start(
            self.library.root.clone(),
            scene,
            (1, 1),
        ));
        let generation = self.skin_export.generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    if generation != this.skin_export.generation {
                        return false;
                    }
                    let Some(player) = &this.skin_export.metadata_player else {
                        return false;
                    };
                    match player.frames.try_recv() {
                        Ok(Ok(crate::animation::Event::Actions { names, selected })) => {
                            this.skin_export.actions = names;
                            if this.skin_export.action.is_empty() {
                                this.skin_export.action = selected;
                            }
                            this.skin_export.metadata_player = None;
                            cx.notify();
                            false
                        }
                        Ok(Err(error)) => {
                            this.skin_export.message = Some(error);
                            this.skin_export.metadata_player = None;
                            cx.notify();
                            false
                        }
                        _ => true,
                    }
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
    fn save_skin_export(&mut self, cx: &mut Context<Self>) {
        if self.skin_export_busy() {
            return;
        }
        let Some(character) = self.selected.and_then(|i| self.library.characters.get(i)) else {
            return;
        };
        let Some(skin) = character.skins.get(self.skin) else {
            return;
        };
        let Some(image) = skin.full_image() else {
            return;
        };
        let state = &self.skin_export;
        let mut request = Request {
            root: self.library.root.clone(),
            image,
            scene: skin.scene.clone(),
            action: state.action.clone(),
            output: PathBuf::new(),
            animated: state.animated,
            format: state.format,
            long_edge: state.long_edge,
            fps: state.fps,
            seconds: state.seconds,
            white_background: state.white_background,
            lossy: state.lossy,
        };
        let name = format!("{}-{}", character.name, skin.display_name(&character.name))
            .replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_");
        let extension = state.format.extension();
        let generation = state.generation;
        #[cfg(feature = "spine")]
        {
            self.skin_export.metadata_player = None;
        }
        self.skin_export.picking = true;
        self.skin_export.progress = crate::importer::Progress {
            label: "选择保存位置".into(),
            ..Default::default()
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title("导出皮肤")
                .add_filter(extension, &[extension])
                .set_file_name(format!("{name}.{extension}"))
                .save_file()
                .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.skin_export.generation {
                    return;
                }
                this.skin_export.picking = false;
                if let Some(file) = file {
                    request.output = file.path().with_extension(extension);
                    match ExportJob::start(&request) {
                        Ok(job) => {
                            this.skin_export.job = Some(job);
                            this.skin_export.progress.label = "准备导出".into();
                            this.poll_skin_export(cx);
                        }
                        Err(error) => {
                            this.skin_export.message = Some(format!("导出失败：{error:#}"))
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn poll_skin_export(&self, cx: &mut Context<Self>) {
        let generation = self.skin_export.generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(100))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    if generation != this.skin_export.generation {
                        return false;
                    }
                    let Some(job) = &this.skin_export.job else {
                        return false;
                    };
                    let events = job.task.events.try_iter().collect::<Vec<_>>();
                    for event in events {
                        match event["type"].as_str() {
                            Some("progress") => {
                                this.skin_export.progress = crate::importer::Progress {
                                    label: event["label"].as_str().unwrap_or("正在导出").into(),
                                    current: event["current"].as_str().unwrap_or("").into(),
                                    done: event["done"].as_u64().unwrap_or(0) as usize,
                                    total: event["total"].as_u64().unwrap_or(0) as usize,
                                    percent: event["percent"].as_f64().unwrap_or(0.) as f32,
                                }
                            }
                            Some("complete") => {
                                this.skin_export.job = None;
                                this.skin_export.message = Some(format!(
                                    "已导出：{}",
                                    model::display_path(&PathBuf::from(
                                        event["output"].as_str().unwrap_or("")
                                    ))
                                ));
                            }
                            Some("error") => {
                                this.skin_export.job = None;
                                this.skin_export.message = Some(format!(
                                    "导出失败：{}",
                                    event["message"].as_str().unwrap_or("未知错误")
                                ));
                            }
                            _ => {}
                        }
                    }
                    cx.notify();
                    this.skin_export.job.is_some()
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
}
