use super::form_dialog::FormDialog;
use super::*;
use gpui_kit::component::checkbox::Checkbox;

pub(super) use crate::account::credentials::SAVED_PASSWORD_MASK;

impl Gallery {
    pub(super) fn login_dialog(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        crate::account::network::prefetch();
        let profile = &self.account.snapshot.profile;
        let needs_login = !self.account.is_connected()
            || (!self.account.snapshot.is_demo && self.account_connection.is_none());
        let body = v_flex()
            .gap_3()
            .when(self.account.is_connected(), |body| {
                body.child(
                    v_flex()
                        .gap_3()
                        .p_3()
                        .rounded_lg()
                        .bg(p.bg)
                        .border_1()
                        .border_color(p.border)
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(profile.nickname.clone()),
                                )
                                .when(self.account.snapshot.is_demo, |row| {
                                    row.child(badge(text::DEBUG_PREVIEW, p.gold, p.panel))
                                }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(p.muted)
                                .child(format!("UID {}", profile.uid)),
                        )
                        .child(
                            h_flex().gap_3().children(
                                [
                                    ("四人麻将", &profile.rank_four),
                                    ("三人麻将", &profile.rank_three),
                                ]
                                .into_iter()
                                .map(|(mode, id)| {
                                    let rank = self.library.ranks.get(id);
                                    h_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_2()
                                        .items_center()
                                        .child(div().w(px(48.)).flex_shrink_0().child(picture(
                                            rank.and_then(|r| r.icon.clone()),
                                            48.,
                                            p,
                                        )))
                                        .child(
                                            v_flex()
                                                .gap_1()
                                                .child(
                                                    div().text_xs().text_color(p.muted).child(mode),
                                                )
                                                .child(
                                                    rank.map(|r| r.name.clone())
                                                        .unwrap_or_else(|| "暂无段位".into()),
                                                )
                                                .child(div().text_xs().text_color(p.muted).child(if mode == "四人麻将" { profile.rank_four_details.clone() } else { profile.rank_three_details.clone() })),
                                        )
                                }),
                            ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(p.muted)
                                .child(format!("上次同步：{}", self.account_sync_label())),
                        ),
                )
            })
            .when(needs_login, |body| {
                body.child(div().text_xs().text_color(p.muted).child(
                    if self.account.is_connected() {
                        "重新连接账号"
                    } else {
                        "账号登录"
                    },
                ))
                .child("账号")
                .child(Input::new(&self.login_username).disabled(self.account_busy))
                .child("密码")
                .child(Input::new(&self.login_password).mask_toggle().disabled(self.account_busy))
                .child(h_flex().gap_4().items_center()
                    .child(Checkbox::new("remember-password").label("记住密码")
                        .checked(self.preferences.remember_password).disabled(self.account_busy)
                        .on_click(cx.listener(|this, checked: &bool, window, cx| {
                            this.preferences.remember_password = *checked;
                            if !checked {
                                this.preferences.auto_login = false;
                                let username = this.login_username.read(cx).value().trim().to_owned();
                                if let Err(error) = crate::account::credentials::save(&model::state_dir(), &username, None) {
                                    this.login_message = Some(format!("清除登录信息失败：{error:#}"));
                                }
                            }
                            this.refresh_saved_password(window, cx);
                            this.save_login_preferences(cx);
                        })))
                    .child(Checkbox::new("auto-login").label("自动登录")
                        .checked(self.preferences.auto_login).disabled(self.account_busy)
                        .on_click(cx.listener(|this, checked: &bool, window, cx| {
                            this.preferences.auto_login = *checked;
                            if *checked { this.preferences.remember_password = true; }
                            this.refresh_saved_password(window, cx);
                            this.save_login_preferences(cx);
                        })))
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(p.muted)
                        .child(if self.preferences.remember_password {
                            "密码派生值将明文保存在 accounts/credentials.json，可用于直接登录。已有记录会显示密码掩码。"
                        } else { "密码仅用于本次登录。" }),
                )
            })
            .child(div().mt_2().child("自动同步周期"))
            .child(
                h_flex().gap_2().children(
                    [
                        (0, "仅手动"),
                        (15, "15 分钟"),
                        (60, "1 小时"),
                        (180, "3 小时"),
                    ]
                    .into_iter()
                    .map(|(minutes, label)| {
                        filter_chip(
                            format!("sync-{minutes}"),
                            label,
                            self.preferences.sync_minutes == minutes,
                            p,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.preferences.sync_minutes = minutes;
                            this.save_login_preferences(cx);
                        }))
                    }),
                ),
            )
            .when_some(self.login_message.clone(), |body, message| {
                body.child(
                    div()
                        .p_3()
                        .rounded_md()
                        .bg(p.bg)
                        .text_sm()
                        .text_color(p.muted)
                        .child(message),
                )
            });
        let actions = h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .when_some(self.debug_account_toggle(cx), |row, toggle| {
                row.child(toggle)
            })
            .child(div().flex_1())
            .child(
                Button::new("manual-sync")
                    .outline()
                    .icon(IconName::RefreshCw)
                    .label(text::SYNC_NOW)
                    .disabled(!self.account.is_connected() || self.account_busy || needs_login)
                    .on_click(cx.listener(|this, _, _, cx| this.sync_account(cx))),
            )
            .when(self.account.is_connected(), |row| {
                row.child(
                    Button::new("dialog-logout")
                        .danger()
                        .icon(IconName::LogOut)
                        .label(text::LOGOUT)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.logout_account(window, cx)),
                        ),
                )
            })
            .when(needs_login, |row| {
                row.child(
                    Button::new("login-submit")
                        .primary()
                        .icon(IconName::LogIn)
                        .label(if self.account_busy {
                            "正在连接…"
                        } else {
                            "登录并同步"
                        })
                        .disabled(self.account_busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.save_login_preferences(cx);
                            this.begin_account_login(window, cx, false);
                        })),
                )
            });
        FormDialog::new(
            text::ACCOUNT_SYNC,
            if self.account.is_connected() {
                "管理账号与同步偏好。"
            } else {
                "登录后同步收藏、好感与材料库存。"
            },
            body,
            actions,
            p,
        )
        .back(self.dialog_back(cx))
        .into_any_element()
    }

    pub(super) fn refresh_saved_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.login_password.read(cx).value().to_string();
        if !current.is_empty() && self.saved_password_username.is_none() {
            return;
        }
        let username = self.login_username.read(cx).value().trim().to_owned();
        let saved = self.preferences.remember_password
            && crate::account::credentials::load(&model::state_dir(), &username)
                .ok()
                .flatten()
                .is_some();
        self.saved_password_username = saved.then_some(username);
        self.login_password.update(cx, |input, cx| {
            input.set_value(if saved { SAVED_PASSWORD_MASK } else { "" }, window, cx)
        });
    }

    fn save_login_preferences(&mut self, cx: &mut Context<Self>) {
        if !self.account.is_connected() {
            self.preferences.login_username = self.login_username.read(cx).value().trim().into();
        }
        if let Err(error) = self.preferences.save() {
            crate::diagnostics::record(format!("save login preferences: {error:#}"));
        }
        if self.account.is_connected()
            && !self.account.snapshot.is_demo
            && let Err(error) = crate::account::save_sync_interval(
                &model::state_dir(),
                &self.account.snapshot.profile.uid,
                self.preferences.sync_minutes,
            )
        {
            self.login_message = Some(format!("同步设置保存失败：{error:#}"));
        }
        cx.notify();
    }
}
