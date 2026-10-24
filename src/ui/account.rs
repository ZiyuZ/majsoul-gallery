use super::*;

impl Gallery {
    pub(super) fn account_area(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.sidebar_compact {
            return v_flex()
                .w_full()
                .gap_2()
                .child(
                    Button::new("sidebar-account")
                        .ghost()
                        .w_full()
                        .icon(if self.account.is_connected() {
                            IconName::UserRound
                        } else {
                            IconName::LogIn
                        })
                        .tooltip(if self.account.is_connected() {
                            text::ACCOUNT_SYNC
                        } else {
                            text::LOGIN
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.refresh_saved_password(window, cx);
                            this.modal = Some(Modal::Login);
                            cx.notify();
                        })),
                )
                .when(self.account.is_connected(), |column| {
                    column
                        .child(self.sync_account_button(cx).w_full())
                        .child(self.logout_account_button(cx).w_full())
                })
                .into_any_element();
        }
        if self.account.is_connected() {
            let owned = self
                .account
                .snapshot
                .characters
                .values()
                .filter(|c| c.owned)
                .count();
            return self.account_card(owned, p, cx);
        }
        v_flex()
            .w_full()
            .p_3()
            .gap_3()
            .rounded_lg()
            .bg(p.bg)
            .border_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .size(px(32.))
                            .rounded_full()
                            .bg(p.accent_bg)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(Icon::new(IconName::UserRound).size_4().text_color(p.accent)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(text::OFFLINE)
                            .child(div().text_xs().text_color(p.muted).child("浏览全部素材")),
                    ),
            )
            .child(
                Button::new("sidebar-login")
                    .primary()
                    .w_full()
                    .icon(IconName::LogIn)
                    .label(text::LOGIN)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.refresh_saved_password(window, cx);
                        this.modal = Some(Modal::Login);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
    fn sync_account_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("sidebar-sync")
            .ghost()
            .icon(IconName::RefreshCw)
            .tooltip("同步账号")
            .disabled(self.account_busy)
            .on_click(cx.listener(|this, _, _, cx| this.sync_account(cx)))
    }
    fn logout_account_button(&self, cx: &mut Context<Self>) -> Button {
        Button::new("sidebar-logout")
            .ghost()
            .icon(IconName::LogOut)
            .tooltip(text::LOGOUT)
            .on_click(cx.listener(|this, _, window, cx| this.logout_account(window, cx)))
    }
    pub(super) fn sync_account(&mut self, cx: &mut Context<Self>) {
        if self.account_busy {
            return;
        }
        if self.account.is_connected() && self.account.snapshot.is_demo {
            self.account.snapshot.synced_at = Some(std::time::SystemTime::now());
            self.login_message = Some("已更新调试预览的同步状态。".into());
        } else if let Some(connection) = &self.account_connection {
            match connection.sync() {
                Ok(()) => {
                    self.account_busy = true;
                    self.account_last_attempt = Some(std::time::Instant::now());
                    self.login_message = Some("读取角色、皮肤与库存…".into());
                }
                Err(error) => {
                    self.login_message = Some(format!("{error:#}，请重新登录。"));
                    self.account_connection = None;
                    self.modal = Some(Modal::Login);
                }
            }
        } else {
            self.modal = Some(Modal::Login);
            self.login_message = Some("请输入密码以重新连接账号。".into());
        }
        cx.notify();
    }
    pub(super) fn begin_account_login(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        automatic: bool,
    ) {
        if self.account_busy {
            return;
        }
        let username = self.login_username.read(cx).value().trim().to_owned();
        let password =
            majsoul_api::Zeroizing::new(self.login_password.read(cx).value().to_string());
        let hash = match crate::account::credentials::resolve(
            &model::state_dir(),
            &username,
            &password,
            self.saved_password_username.as_deref(),
            self.preferences.remember_password,
        ) {
            Ok(hash) => hash,
            Err(error) => {
                self.login_message = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        };
        drop(password);
        if username.is_empty() || hash.is_none() {
            self.login_message = Some("请输入账号和密码。".into());
            cx.notify();
            return;
        }
        self.login_password
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.account_generation += 1;
        self.account_connection = None;
        match crate::account::network::Connection::login(
            username,
            hash.unwrap(),
            self.preferences.device_id.clone(),
            self.preferences.remember_password,
        ) {
            Ok(connection) => self.account_connection = Some(connection),
            Err(error) => {
                self.login_message = Some(format!("{error:#}"));
                cx.notify();
                return;
            }
        }
        self.account_busy = true;
        self.account_last_attempt = Some(std::time::Instant::now());
        self.login_message = Some("获取版本与线路…".into());
        let generation = self.account_generation;
        cx.spawn(async move |this, cx| {
            let mut close_after_login = automatic;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let running = this.update(cx, |this, cx| {
                    if this.account_generation != generation {
                        return false;
                    }
                    let events = this
                        .account_connection
                        .as_ref()
                        .map(|connection| connection.events.try_iter().collect::<Vec<_>>())
                        .unwrap_or_default();
                    let mut changed = false;
                    for event in events {
                        use crate::account::network::Event;
                        changed = true;
                        match event {
                            Event::Progress(message) => this.login_message = Some(message.into()),
                            Event::Snapshot(source) => {
                                this.account_busy = false;
                                match AccountSnapshot::from_api(&source, &this.library) {
                                    Ok(snapshot) => {
                                        if close_after_login && this.modal == Some(Modal::Login) {
                                            this.modal = None;
                                        }
                                        close_after_login = false;
                                        let switched = !this.account.is_connected()
                                            || this.account.snapshot.profile.uid
                                                != snapshot.profile.uid;
                                        let saved = snapshot.save_account(
                                            &source,
                                            &model::state_dir(),
                                            this.preferences.sync_minutes,
                                        );
                                        this.account.activate(snapshot);
                                        if switched {
                                            this.reset_account_filters(true);
                                        }
                                        this.collection_key.clear();
                                        this.login_message = Some(match saved {
                                            Ok(()) => "账号数据已同步。".into(),
                                            Err(error) => {
                                                format!("账号数据已同步，保存失败：{error:#}")
                                            }
                                        });
                                    }
                                    Err(error) => {
                                        this.login_message =
                                            Some(format!("账号数据解析失败：{error:#}"))
                                    }
                                }
                            }
                            Event::Error(message) => {
                                this.account_busy = false;
                                this.login_message = Some(message);
                                if this
                                    .account_connection
                                    .as_ref()
                                    .is_some_and(|c| c.is_closed())
                                {
                                    this.account_connection = None;
                                }
                            }
                            Event::Disconnected(message) => {
                                this.account_busy = false;
                                this.login_message = Some(message);
                                this.account_connection = None;
                            }
                        }
                    }
                    if this
                        .account_connection
                        .as_ref()
                        .is_some_and(|c| c.is_closed())
                    {
                        this.account_connection = None;
                        this.account_busy = false;
                        changed = true;
                    }
                    if this.account.is_connected()
                        && !this.account.snapshot.is_demo
                        && !this.account_busy
                        && this.preferences.sync_minutes > 0
                        && this
                            .account_connection
                            .as_ref()
                            .is_some_and(|c| !c.is_closed())
                        && this.account_last_attempt.is_some_and(|t| {
                            t.elapsed().as_secs() >= this.preferences.sync_minutes * 60
                        })
                    {
                        this.sync_account(cx);
                    }
                    if changed {
                        cx.notify();
                    }
                    this.account_connection.is_some()
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }
    pub(super) fn logout_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.account_generation += 1;
        self.account_connection = None;
        self.account_busy = false;
        self.account_last_attempt = None;
        self.account.disconnect();
        self.reset_account_filters(false);
        self.login_message = None;
        self.login_password
            .update(cx, |password, cx| password.set_value("", window, cx));
        self.saved_password_username = None;
        self.refresh_saved_password(window, cx);
        cx.notify();
    }
    pub(super) fn account_sync_label(&self) -> String {
        match self
            .account
            .snapshot
            .synced_at
            .map(|t| t.elapsed().unwrap_or_default().as_secs() / 60)
        {
            None => "尚未同步".into(),
            Some(0) => "刚刚同步".into(),
            Some(minutes) => format!("{minutes} 分钟前同步"),
        }
    }
    fn reset_account_filters(&mut self, connected: bool) {
        self.only_owned = connected;
        self.only_favorites = false;
        self.contract_filter = ContractFilter::All;
        self.character_order = crate::account::CharacterOrder::Game;
        self.collection_key.clear();
    }
    pub(super) fn debug_account_toggle(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        #[cfg(debug_assertions)]
        {
            Some(
                gpui_kit::component::switch::Switch::new("debug-account-preview")
                    .small()
                    .label(text::DEBUG_PREVIEW)
                    .checked(self.account.snapshot.is_demo)
                    .accessibility_label("登录效果预览")
                    .tooltip("使用示例账号数据预览登录效果，重启后恢复离线。")
                    .on_click(cx.listener(|this, checked, _, cx| {
                        this.account_generation += 1;
                        this.account_connection = None;
                        this.account_busy = false;
                        if *checked {
                            this.account.activate(AccountSnapshot::demo(&this.library));
                        } else {
                            this.account.disconnect();
                        }
                        this.reset_account_filters(*checked);
                        this.login_message = None;
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = cx;
            None
        }
    }
    fn account_card(&self, owned: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let profile = &self.account.snapshot.profile;
        v_flex()
            .mt_3()
            .p_3()
            .gap_3()
            .rounded_lg()
            .bg(p.bg)
            .border_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        Button::new("account-profile")
                            .ghost()
                            .small()
                            .px_0()
                            .justify_start()
                            .label(profile.nickname.clone())
                            .tooltip(text::ACCOUNT_SYNC)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_saved_password(window, cx);
                                this.modal = Some(Modal::Login);
                                cx.notify();
                            })),
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
                h_flex().gap_2().children(
                    [("四麻", &profile.rank_four), ("三麻", &profile.rank_three)]
                        .into_iter()
                        .map(|(mode, id)| {
                            let rank = self.library.ranks.get(id);
                            v_flex()
                                .flex_1()
                                .gap_1()
                                .items_center()
                                .py_2()
                                .rounded_md()
                                .bg(p.sidebar)
                                .child(div().w(px(56.)).child(picture(
                                    rank.and_then(|r| r.icon.clone()),
                                    56.,
                                    p,
                                )))
                                .child(div().text_xs().child(format!(
                                    "{mode} · {}",
                                    rank.map(|r| r.name.as_str()).unwrap_or("—")
                                )))
                                .child(div().text_xs().text_color(p.muted).child(
                                    if mode == "四麻" {
                                        profile.rank_four_details.clone()
                                    } else {
                                        profile.rank_three_details.clone()
                                    },
                                ))
                        }),
                ),
            )
            .child(div().text_xs().text_color(p.muted).child(format!(
                "拥有角色 {owned} / {}",
                self.library.characters.len()
            )))
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(p.muted)
                            .child(self.account_sync_label()),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(self.sync_account_button(cx).small())
                            .child(self.logout_account_button(cx).small()),
                    ),
            )
            .into_any_element()
    }
}
