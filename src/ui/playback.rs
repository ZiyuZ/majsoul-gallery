use super::*;
use std::time::Duration;

impl Gallery {
    pub(super) fn update_animation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self
            .selected
            .and_then(|i| self.library.characters.get(i))
            .and_then(|c| c.skins.get(self.skin))
            .filter(|_| {
                self.animation_enabled
                    && !self.loading
                    && self.error.is_none()
                    && self.modal.is_none()
            })
            .and_then(|s| s.scene.clone());
        let viewport = window.viewport_size();
        let dpi = window.scale_factor() * self.preview_scale.clamp(1., 2.);
        let resolution = (
            ((f32::from(viewport.width) - self.sidebar_width() - 340.) * dpi).clamp(320., 1800.)
                as u32,
            ((f32::from(viewport.height) - shell::TITLE_BAR_HEIGHT - 174.) * dpi).clamp(320., 1800.)
                as u32,
        );
        if path == self.animation_path {
            if path.is_some() && resolution != self.animation_resolution {
                self.animation_resolution = resolution;
                if let Some(player) = self.animation_player.as_mut()
                    && let Err(error) = player.resize(resolution)
                {
                    crate::diagnostics::record(format!("animation resize: {error:#}"));
                }
            }
            return;
        }
        self.animation_generation += 1;
        self.animation_player = None;
        self.animation_actions.clear();
        self.animation_action.clear();
        self.animation_error = None;
        if let Some(frame) = self.animation_frame.take() {
            cx.drop_image(frame, None);
        }
        self.animation_path = path.clone();
        self.animation_resolution = resolution;
        let Some(path) = path else {
            return;
        };
        self.animation_player = Some(crate::animation::Player::start(
            self.library.root.clone(),
            path,
            resolution,
        ));
        let generation = self.animation_generation;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep = this.update(cx, |this, cx| {
                    if generation != this.animation_generation {
                        return false;
                    }
                    let Some(player) = &this.animation_player else {
                        return false;
                    };
                    match player.frames.try_recv() {
                        Ok(Ok(crate::animation::Event::Actions { names, selected })) => {
                            this.animation_actions = names;
                            this.animation_action = selected;
                            cx.notify();
                        }
                        Ok(Ok(crate::animation::Event::Frame(frame))) => {
                            if this.animation_frame.is_none() {
                                crate::diagnostics::record("animation first frame received");
                            }
                            if let Some(old) = this.animation_frame.take() {
                                cx.drop_image(old, None);
                            }
                            this.animation_frame =
                                Some(Arc::new(RenderImage::new(vec![image::Frame::new(frame)])));
                            cx.notify();
                        }
                        Ok(Err(error)) => {
                            this.animation_error = Some(error);
                            this.animation_player = None;
                            cx.notify();
                            return false;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            this.animation_error =
                                Some("动画渲染线程已结束，请关闭后重试。".into());
                            this.animation_player = None;
                            cx.notify();
                            return false;
                        }
                        _ => {}
                    }
                    true
                });
                if !matches!(keep, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
}
