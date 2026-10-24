use super::form_dialog::FormDialog;
use super::*;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Modal {
    Import,
    Settings,
    Login,
    Export,
    SkinExport,
}
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Stage {
    Discovering,
    Choose,
    Running,
    Failed,
}
pub(super) struct ImportState {
    pub stage: Stage,
    pub candidates: Vec<PathBuf>,
    pub progress: crate::task_process::Progress,
    pub message: Option<String>,
    pub full_reimport: bool,
}
impl Default for ImportState {
    fn default() -> Self {
        Self {
            stage: Stage::Choose,
            candidates: vec![],
            progress: Default::default(),
            message: None,
            full_reimport: false,
        }
    }
}

impl Gallery {
    pub(super) fn dialog_back(&self, cx: &mut Context<Self>) -> Button {
        back_icon_button("dialog-back", "返回主页")
            .disabled(self.loading || self.export_busy())
            .on_click(cx.listener(|this, _, _, cx| this.dismiss_modal(cx)))
    }
    pub(super) fn dismiss_modal(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.export_busy() {
            return;
        }
        if self.modal == Some(Modal::SkinExport) {
            self.cancel_skin_export();
        }
        if self.modal == Some(Modal::Import) {
            self.import_generation += 1;
            self.import_job = None;
            self.import_state.stage = Stage::Choose;
            if self.library.root.as_os_str().is_empty() {
                cx.notify();
                return;
            }
        }
        self.modal = None;
        self.error = None;
        cx.notify();
    }
    pub(super) fn modal_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape" && self.modal.is_some() {
            self.dismiss_modal(cx);
            cx.stop_propagation();
        }
    }
    pub(super) fn check_installation_updates(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.preferences.installation.clone() else {
            return;
        };
        let active = self.library.root.clone();
        let generation = self.import_generation;
        cx.spawn(async move |this, cx| {
            let check_source = source.clone();
            let changed = cx
                .background_executor()
                .spawn(async move { crate::importer::has_updates(&check_source, &active) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.import_generation || this.modal.is_some() {
                    return;
                }
                match changed {
                    Ok(true) => this.begin_import(source, cx),
                    Err(error) => {
                        crate::diagnostics::record(format!("startup update check: {error:#}"))
                    }
                    _ => {}
                }
            });
        })
        .detach();
    }
    pub(super) fn discover_installation(&mut self, cx: &mut Context<Self>) {
        self.search_installation(true, cx);
    }
    fn search_installation(&mut self, automatic: bool, cx: &mut Context<Self>) {
        self.modal = Some(Modal::Import);
        self.import_state.stage = Stage::Discovering;
        self.import_state.message = None;
        self.import_generation += 1;
        let generation = self.import_generation;
        let saved = self.preferences.installation.clone();
        cx.spawn(async move |this, cx| {
            let candidates = cx
                .background_executor()
                .spawn(async move { crate::discovery::find_installs(saved.as_deref()) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.import_generation != generation {
                    return;
                }
                this.import_state.candidates = candidates;
                this.import_state.stage = Stage::Choose;
                if let Some(path) = this.import_state.candidates.first().cloned() {
                    this.import_input.update(cx, |input, cx| {
                        input.set_value(model::display_path(&path), window, cx)
                    });
                    if automatic && this.import_state.candidates.len() == 1 {
                        this.begin_import(path, cx);
                    }
                } else {
                    this.import_state.message = Some("未找到雀魂安装，请选择目录或程序。".into());
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn begin_import(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.begin_import_mode(path, false, cx);
    }
    fn begin_import_mode(&mut self, path: PathBuf, full: bool, cx: &mut Context<Self>) {
        if self.loading || self.import_job.is_some() {
            return;
        }
        self.error = None;
        self.animation_enabled = false;
        self.modal = Some(Modal::Import);
        self.import_state.message = None;
        self.import_state.progress = crate::task_process::Progress {
            label: "准备本地素材导入".into(),
            current: crate::model::display_path(&path),
            ..Default::default()
        };
        match crate::importer::start(&path, &model::state_dir(), full) {
            Ok(job) => {
                self.import_job = Some(job);
                self.import_state.stage = Stage::Running;
            }
            Err(error) => {
                self.import_state.stage = Stage::Failed;
                self.import_state.message = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        }
        self.import_generation += 1;
        let generation = self.import_generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let running = this.update(cx, |this, cx| {
                    if this.import_generation != generation {
                        return false;
                    }
                    let events = this
                        .import_job
                        .as_ref()
                        .map(|job| job.events.try_iter().collect::<Vec<_>>())
                        .unwrap_or_default();
                    for event in events {
                        match event["type"].as_str() {
                            Some("progress") => {
                                this.import_state.progress =
                                    crate::task_process::Progress::from_event(&event, "");
                            }
                            Some("complete") => {
                                this.import_job = None;
                                this.preferences.installation =
                                    event["source"].as_str().map(PathBuf::from);
                                if let Some(assets) = event["assets"].as_str() {
                                    this.load(PathBuf::from(assets), cx);
                                } else {
                                    this.import_state.stage = Stage::Failed;
                                    this.import_state.message = Some("导入结果缺少素材路径".into());
                                }
                                cx.notify();
                                return false;
                            }
                            Some("error") => {
                                this.import_job = None;
                                this.import_state.stage = Stage::Failed;
                                this.import_state.message =
                                    Some(event["message"].as_str().unwrap_or("导入失败").into());
                                cx.notify();
                                return false;
                            }
                            _ => {}
                        }
                    }
                    cx.notify();
                    this.import_job.is_some()
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn cancel_import(&mut self, cx: &mut Context<Self>) {
        self.import_generation += 1;
        self.import_job = None;
        self.import_state.stage = Stage::Choose;
        self.import_state.message = Some("已取消导入，已完成的素材会在下次导入时复用。".into());
        cx.notify();
    }

    fn choose_import_path(&mut self, executable: bool, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let dialog = rfd::AsyncFileDialog::new().set_title("选择雀魂安装目录或程序");
            let selected = if executable {
                dialog
                    .add_filter("Windows 程序", &["exe"])
                    .pick_file()
                    .await
            } else {
                dialog.pick_folder().await
            };
            if let Some(selected) = selected {
                let path = model::display_path(selected.path());
                let _ = this.update_in(cx, |this, window, cx| {
                    this.import_input
                        .update(cx, |input, cx| input.set_value(path, window, cx));
                    this.import_state.message = None;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn modal_content(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.modal == Some(Modal::SkinExport) {
            return self.skin_export_dialog(p, cx);
        }
        if self.modal == Some(Modal::Login) {
            return self.login_dialog(p, cx);
        }
        if self.modal == Some(Modal::Export) {
            return self.export_dialog(p, cx);
        }
        if self.modal == Some(Modal::Settings) {
            return FormDialog::new(
                text::SETTINGS,
                "外观与素材来源",
                v_flex()
                    .gap_4()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child("主题")
                            .child(
                                Button::new("settings-theme")
                                    .outline()
                                    .icon(if self.dark {
                                        IconName::Moon
                                    } else {
                                        IconName::Sun
                                    })
                                    .label(if self.dark { "深色" } else { "浅色" })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_theme(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        v_flex().gap_2().child("雀魂安装位置").child(
                            div().text_xs().text_color(p.muted).child(
                                self.preferences
                                    .installation
                                    .as_ref()
                                    .map(|p| model::display_path(p))
                                    .unwrap_or_else(|| "尚未设置".into()),
                            ),
                        ),
                    )
                    .child(
                        Button::new("settings-source")
                            .outline()
                            .icon(IconName::FolderOpen)
                            .label("管理素材来源")
                            .on_click(cx.listener(|this, _, _, cx| this.open_package(cx))),
                    ),
                div(),
                p,
            )
            .back(self.dialog_back(cx))
            .no_actions()
            .into_any_element();
        }
        let stage = self.import_state.stage;
        let busy = matches!(stage, Stage::Discovering | Stage::Running) || self.loading;
        let progress = &self.import_state.progress;
        let body = if busy {
            super::form_dialog::progress_body(
                if stage == Stage::Discovering {
                    "正在查找 Steam 中的雀魂安装…".to_owned()
                } else {
                    progress.label.clone()
                },
                progress,
                stage == Stage::Discovering,
                p,
            )
        } else {
            v_flex().gap_4()
                .child(ButtonGroup::new("asset-source-tabs").outline().compact().children([
                    Button::new("source-local").icon(IconName::FolderOpen).label("本地素材").selected(true),
                    Button::new("source-network").icon(IconName::CloudDownload).label("网络素材").disabled(true)
                        .tooltip("网络素材功能暂未实现"),
                ]))
                .when_some(self.import_state.message.clone(), |body, message| body.child(div().p_3().rounded_md().bg(p.bg).text_sm().text_color(p.gold).child(message)))
                .child(div().text_sm().child("雀魂安装目录或程序"))
                .child(Input::new(&self.import_input))
                .child(h_flex().gap_2().flex_wrap()
                    .child(Button::new("choose-game-folder").outline().icon(IconName::FolderOpen).label("选择目录").on_click(cx.listener(|this, _, _, cx| this.choose_import_path(false, cx))))
                    .child(Button::new("choose-game-exe").outline().icon(IconName::File).label("选择程序").on_click(cx.listener(|this, _, _, cx| this.choose_import_path(true, cx))))
                    .child(Button::new("search-game").outline().icon(IconName::Search).label("自动搜索").on_click(cx.listener(|this, _, _, cx| this.search_installation(false, cx)))))
                .when(self.import_state.candidates.len() > 1, |body| body.child(v_flex().gap_2().children(
                    self.import_state.candidates.iter().enumerate().map(|(i, path)| {
                        let path = model::display_path(path);
                        Button::new(format!("install-{i}")).outline().w_full().justify_start().icon(IconName::FolderOpen)
                            .label(path.clone()).on_click(cx.listener(move |this, _, window, cx| {
                                this.import_input.update(cx, |input, cx| input.set_value(path.clone(), window, cx));
                            }))
                    })
                )))
                .child(div().text_xs().text_color(p.muted).child("通常位于 Steam 库的 steamapps/common/MahjongSoul，也可选择 StandaloneWindows 资源目录。"))
                .child(div().h(px(1.)).bg(p.border))
                .child(gpui_kit::component::switch::Switch::new("full-reimport").label("重新生成全部素材").checked(self.import_state.full_reimport)
                    .on_click(cx.listener(|this, checked, _, cx| { this.import_state.full_reimport = *checked; cx.notify(); })))
                .child(div().text_xs().text_color(p.muted).child(if self.import_state.full_reimport {
                    "清理并重新生成当前素材库中的图片与资料。"
                } else { "自动复用已有素材，仅导入新增或变化的内容。" }))
                .into_any_element()
        };
        let actions = h_flex().justify_end().child(if busy {
            Button::new("cancel-import")
                .outline()
                .label("取消")
                .on_click(cx.listener(|this, _, _, cx| this.cancel_import(cx)))
        } else {
            Button::new("start-import")
                .primary()
                .icon(IconName::Download)
                .label("确定")
                .on_click(cx.listener(|this, _, _, cx| {
                    let path = this
                        .import_input
                        .read(cx)
                        .value()
                        .trim()
                        .trim_matches('"')
                        .to_owned();
                    let path = if path.is_empty() {
                        this.preferences.installation.clone()
                    } else {
                        Some(PathBuf::from(path))
                    };
                    if let Some(path) = path {
                        this.begin_import_mode(path, this.import_state.full_reimport, cx);
                    } else {
                        this.import_state.message =
                            Some("请选择目录或程序，或使用自动搜索。".into());
                        cx.notify();
                    }
                }))
        });
        FormDialog::new(
            "素材导入",
            "导入角色、皮肤与装扮，建立你的素材库。",
            body,
            actions,
            p,
        )
        .when(!self.library.root.as_os_str().is_empty(), |dialog| {
            dialog.back(self.dialog_back(cx))
        })
        .into_any_element()
    }
}
