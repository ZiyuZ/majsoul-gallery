mod filters;
use filters::{controls_panel, controls_row};
mod cards;
mod illustrations;
mod images;
mod inventory;
use images::app_icon;
mod components;
use cards::{collection_card, collection_image, collection_meta, collection_status, picture};
use components::{
    back_icon_button, badge, choice_group, filter_chip, material_color, material_icon,
    segment_button, view_tab,
};
mod browsing;
mod collection;
mod form_dialog;
mod home;
mod import_flow;
mod shell;
use import_flow::{ImportState, Modal, Stage};
mod account;
mod bond;
mod detail;
mod login;
#[cfg(feature = "spine")]
mod playback;
mod preview;
mod share;
mod skin_export;
mod training;

use crate::account::{AccountSession, AccountSnapshot};
use crate::bond::{BondProgress, MAX_BOND};
use crate::model::{self, Character, Decoration, Library, Skin, Title};
use crate::text::{self, decoration_name};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    button::*, input::InputEvent, input::*, progress::Progress, scroll::ScrollableElement, *,
};
use gpui_kit::{Point, prelude::FluentBuilder, *};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Collection,
    Inventory,
    Training,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Category {
    Characters,
    Skins,
    Decorations,
    Titles,
    Illustrations,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ContractFilter {
    #[default]
    All,
    Contracted,
    Uncontracted,
    Ready,
    Full,
}

#[derive(Clone, Copy)]
struct Palette {
    bg: Hsla,
    sidebar: Hsla,
    panel: Hsla,
    image: Hsla,
    border: Hsla,
    text: Hsla,
    muted: Hsla,
    accent: Hsla,
    accent_bg: Hsla,
    gold: Hsla,
}
impl Palette {
    fn new(dark: bool) -> Self {
        let c = |v| Hsla::from(rgb(v));
        if dark {
            Self {
                bg: c(0x10131a),
                sidebar: c(0x151923),
                panel: c(0x1b202c),
                image: c(0x202633),
                border: c(0x2b3242),
                text: c(0xe9eef5),
                muted: c(0x929eaf),
                accent: c(0x7de0c1),
                accent_bg: c(0x203a36),
                gold: c(0xe7bf83),
            }
        } else {
            Self {
                bg: c(0xf4f6f9),
                sidebar: c(0xffffff),
                panel: c(0xffffff),
                image: c(0xeef1f6),
                border: c(0xdde3ec),
                text: c(0x263244),
                muted: c(0x728096),
                accent: c(0x16836d),
                accent_bg: c(0xe0f3ed),
                gold: c(0xa36d26),
            }
        }
    }
}
fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub struct Gallery {
    library: Arc<Library>,
    preferences: crate::settings::Settings,
    focus: FocusHandle,
    startup_checked: bool,
    export_state: share::ExportState,
    skin_export: skin_export::State,
    inventory: inventory::State,
    illustration: Option<usize>,
    illustration_filter: illustrations::Filter,
    illustration_export: illustrations::ExportState,
    login_username: Entity<InputState>,
    login_password: Entity<InputState>,
    saved_password_username: Option<String>,
    login_message: Option<String>,
    account_connection: Option<crate::account::network::Connection>,
    account_busy: bool,
    account_generation: u64,
    account_last_attempt: Option<std::time::Instant>,
    modal: Option<Modal>,
    import_state: ImportState,
    import_input: Entity<InputState>,
    import_job: Option<crate::task_process::Job>,
    import_generation: u64,
    account: AccountSession,
    page: Page,
    category: Category,
    selected: Option<usize>,
    skin: usize,
    only_owned: bool,
    contract_filter: ContractFilter,
    character_order: crate::account::CharacterOrder,
    only_favorites: bool,
    character_sex: u64,
    character_limited: Option<bool>,
    character_collaboration: bool,
    decoration: String,
    card_size: usize,
    training_columns: usize,
    bond_hearts: bool,
    skins_grouped: bool,
    skin_full_art: bool,
    only_animated_skins: bool,
    descending: bool,
    sidebar_compact: bool,
    collection_list: ListState,
    collection_key: String,
    training_scroll: ScrollHandle,
    animation_enabled: bool,
    animation_path: Option<PathBuf>,
    #[cfg(feature = "spine")]
    animation_player: Option<crate::animation::Player>,
    animation_frame: Option<Arc<RenderImage>>,
    animation_error: Option<String>,
    #[cfg(feature = "spine")]
    animation_generation: u64,
    #[cfg(feature = "spine")]
    animation_resolution: (u32, u32),
    animation_actions: Vec<String>,
    animation_action: String,
    preview_scale: f32,
    preview_background: Option<preview::PreviewBackground>,
    preview_pan: Point<Pixels>,
    preview_drag: Option<Point<Pixels>>,
    preview_selection: Option<(usize, usize)>,
    dark: bool,
    loading: bool,
    error: Option<String>,
    search: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}
impl Gallery {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(text::SEARCH_PLACEHOLDER));
        let preferences = crate::settings::Settings::load();
        let import_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("例如 D:\\Steam\\steamapps\\common\\MahjongSoul")
                .default_value(
                    preferences
                        .installation
                        .as_ref()
                        .map(|path| model::display_path(path))
                        .unwrap_or_default(),
                )
        });
        let subscription = cx.subscribe(&search, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                if this.page == Page::Home && !this.search.read(cx).value().is_empty() {
                    this.page = Page::Collection;
                    this.category = Category::Characters;
                    this.selected = None;
                }
                cx.notify();
            }
        });
        let library = Arc::new(Library::default());
        let login_username = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("雀魂账号")
                .default_value(preferences.login_username.clone())
        });
        let remembered = preferences.remember_password
            && crate::account::credentials::load(&model::state_dir(), &preferences.login_username)
                .ok()
                .flatten()
                .is_some();
        let login_password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("密码")
                .masked(true)
                .default_value(if remembered {
                    login::SAVED_PASSWORD_MASK
                } else {
                    ""
                })
        });
        let username_subscription =
            cx.subscribe_in(&login_username, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.saved_password_username = None;
                    this.login_password
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.refresh_saved_password(window, cx);
                    cx.notify();
                }
            });
        let password_subscription = cx.subscribe(&login_password, |this, _, event, cx| {
            if matches!(event, InputEvent::Change)
                && this.login_password.read(cx).value().as_ref() != login::SAVED_PASSWORD_MASK
            {
                this.saved_password_username = None;
            }
        });
        Theme::change(
            if preferences.dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        configure_theme(cx, preferences.dark);
        let mut this = Self {
            account: AccountSession::default(),
            library,
            focus: cx.focus_handle(),
            startup_checked: false,
            export_state: Default::default(),
            skin_export: Default::default(),
            inventory: Default::default(),
            illustration: None,
            illustration_filter: Default::default(),
            illustration_export: Default::default(),
            login_username,
            login_password,
            saved_password_username: remembered.then(|| preferences.login_username.clone()),
            login_message: None,
            account_connection: None,
            account_busy: false,
            account_generation: 0,
            account_last_attempt: None,
            page: Page::Home,
            category: Category::Characters,
            selected: None,
            skin: 0,
            only_owned: false,
            contract_filter: ContractFilter::All,
            character_order: crate::account::CharacterOrder::Game,
            only_favorites: false,
            character_sex: 0,
            character_limited: None,
            character_collaboration: false,
            decoration: String::new(),
            card_size: preferences.card_size,
            training_columns: preferences.training_columns,
            bond_hearts: preferences.bond_hearts,
            skins_grouped: preferences.skins_grouped,
            skin_full_art: preferences.skin_full_art,
            only_animated_skins: false,
            descending: preferences.descending,
            sidebar_compact: preferences.sidebar_compact,
            collection_list: ListState::new(0, ListAlignment::Top, px(400.)),
            collection_key: String::new(),
            training_scroll: ScrollHandle::new(),
            animation_enabled: false,
            animation_path: None,
            #[cfg(feature = "spine")]
            animation_player: None,
            animation_frame: None,
            animation_error: None,
            #[cfg(feature = "spine")]
            animation_generation: 0,
            #[cfg(feature = "spine")]
            animation_resolution: (0, 0),
            animation_actions: Vec::new(),
            animation_action: String::new(),
            preview_scale: 1.,
            preview_background: None,
            preview_pan: point(px(0.), px(0.)),
            preview_drag: None,
            preview_selection: None,
            dark: preferences.dark,
            loading: false,
            error: None,
            search,
            _subscriptions: vec![subscription, username_subscription, password_subscription],
            preferences,
            modal: Some(Modal::Import),
            import_state: ImportState::default(),
            import_input,
            import_job: None,
            import_generation: 0,
        };
        this.focus.focus(window, cx);
        if let Some(path) = model::initial_source(&this.preferences) {
            this.load(path, cx);
        } else {
            this.discover_installation(cx);
        }
        if this.preferences.auto_login {
            crate::account::network::prefetch();
            cx.spawn_in(window, async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;
                    let waiting = this.update_in(cx, |this, window, cx| {
                        if !this.preferences.auto_login
                            || this.account.is_connected()
                            || this.account_busy
                        {
                            return false;
                        }
                        if this.loading
                            || this.library.root.as_os_str().is_empty()
                            || this.modal == Some(Modal::Import)
                        {
                            return true;
                        }
                        this.modal = Some(Modal::Login);
                        this.begin_account_login(window, cx, true);
                        false
                    });
                    if !matches!(waiting, Ok(true)) {
                        break;
                    }
                }
            })
            .detach();
        }
        this
    }
    fn load(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.loading = true;
        self.modal = Some(Modal::Import);
        self.import_state.stage = Stage::Running;
        self.import_state.progress = crate::task_process::Progress {
            label: "正在整理素材包…".into(),
            percent: 98.,
            ..Default::default()
        };
        self.error = None;
        self.animation_enabled = false;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { Library::load(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(lib) => {
                        this.account.refresh_library(&lib);
                        this.library = Arc::new(lib);
                        this.selected = None;
                        this.illustration = None;
                        this.skin = 0;
                        this.collection_key.clear();
                        this.modal = None;
                        if !this.startup_checked {
                            this.startup_checked = true;
                            this.check_installation_updates(cx);
                        }
                        #[cfg(debug_assertions)]
                        if let Ok(id) = std::env::var("MAJSOUL_GALLERY_PREVIEW_SKIN")
                            && let Some((character, skin)) = this
                                .library
                                .characters
                                .iter()
                                .enumerate()
                                .find_map(|(i, c)| {
                                    c.skins.iter().position(|s| s.id == id).map(|s| (i, s))
                                })
                        {
                            this.selected = Some(character);
                            this.skin = skin;
                            this.animation_enabled = cfg!(feature = "spine");
                        }
                    }
                    Err(e) => {
                        this.import_state.stage = Stage::Failed;
                        this.import_state.message = Some(format!("{e:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn persist_preferences(&mut self) {
        let mut preferences = self.preferences.clone();
        preferences.dark = self.dark;
        preferences.card_size = self.card_size;
        preferences.training_columns = self.training_columns;
        preferences.bond_hearts = self.bond_hearts;
        preferences.skins_grouped = self.skins_grouped;
        preferences.skin_full_art = self.skin_full_art;
        preferences.descending = self.descending;
        preferences.sidebar_compact = self.sidebar_compact;
        if !self.library.root.as_os_str().is_empty() {
            preferences.assets = Some(self.library.root.clone());
        }
        if preferences != self.preferences {
            if let Err(error) = preferences.save() {
                crate::diagnostics::record(format!("save settings: {error:#}"));
            }
            self.preferences = preferences;
        }
    }
    fn open_package(&mut self, cx: &mut Context<Self>) {
        if self.loading || self.import_job.is_some() {
            return;
        }
        self.import_state = ImportState::default();
        self.modal = Some(Modal::Import);
        cx.notify();
    }
    fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.selected = None;
        self.illustration = None;

        cx.notify();
    }
    fn open_character(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        self.skin = 0;
        cx.notify();
    }
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        self.preview_background = None;
        Theme::change(
            if self.dark {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        configure_theme(cx, self.dark);
        cx.notify();
    }
    fn favorite_indicator(&self, id: &str, p: Palette) -> AnyElement {
        let favorite = self.account.is_connected()
            && self
                .account
                .snapshot
                .characters
                .get(id)
                .is_some_and(|s| s.favorite);
        div()
            .id(format!("favorite-{id}"))
            .w(px(22.))
            .text_color(p.gold)
            .when(favorite, |d| d.child(Icon::new(IconName::Star).size_4()))
            .into_any_element()
    }
    fn heading(&self, title: &str, subtitle: impl Into<SharedString>, p: Palette) -> Div {
        v_flex()
            .gap_2()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title.to_owned()),
            )
            .child(div().text_sm().text_color(p.muted).child(subtitle.into()))
    }
    fn empty(&self, title: &str, subtitle: &str, p: Palette) -> AnyElement {
        v_flex()
            .w_full()
            .py_12()
            .items_center()
            .gap_4()
            .text_color(p.muted)
            .child(Icon::new(IconName::FolderOpen).size_8())
            .child(div().text_lg().text_color(p.text).child(title.to_owned()))
            .child(div().text_sm().child(subtitle.to_owned()))
            .into_any_element()
    }
}
pub fn configure_theme(cx: &mut App, dark: bool) {
    let p = Palette::new(dark);
    Theme::update(cx, |t| {
        t.font_family = "Microsoft YaHei UI".into();
        t.primary = p.accent;
        t.primary_foreground = if dark {
            rgb(0x10221d).into()
        } else {
            rgb(0xffffff).into()
        };
        t.radius = px(8.);
        t.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Scrolling;
    });
    let base = gpui_kit::base::Theme::global_mut(cx);
    base.scrollbar = base
        .scrollbar
        .clone()
        .with_motion(browsing::scrollbar_motion());
}
impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.selected.map(|i| (i, self.skin));
        if self.preview_selection != selection {
            self.preview_selection = selection;
            self.preview_scale = 1.;
            self.preview_pan = point(px(0.), px(0.));
            self.preview_drag = None;
        }
        self.persist_preferences();
        #[cfg(feature = "spine")]
        self.update_animation(window, cx);
        let p = Palette::new(self.dark);
        if self.modal == Some(Modal::Import) && self.library.root.as_os_str().is_empty() {
            return v_flex()
                .track_focus(&self.focus)
                .capture_key_down(cx.listener(Self::modal_key_down))
                .size_full()
                .bg(p.bg)
                .text_color(p.text)
                .text_sm()
                .child(self.app_title_bar(window, p, false))
                .child(div().flex_1().min_h_0().child(self.modal_content(p, cx)))
                .into_any_element();
        }
        let content = if self.loading {
            v_flex()
                .gap_5()
                .py_12()
                .items_center()
                .child(div().text_lg().child("正在整理素材包…"))
                .child(
                    div().w(px(280.)).child(
                        Progress::new("loading-package")
                            .loading(true)
                            .color(p.accent),
                    ),
                )
                .into_any_element()
        } else if let Some(error) = &self.error {
            v_flex()
                .gap_4()
                .child(self.heading("素材包载入失败", error.clone(), p))
                .child(
                    Button::new("retry-open")
                        .primary()
                        .label("重新选择素材包")
                        .on_click(cx.listener(|this, _, _, cx| this.open_package(cx))),
                )
                .when(!self.library.root.as_os_str().is_empty(), |d| {
                    d.child(
                        Button::new("keep-current-library")
                            .outline()
                            .icon(IconName::ArrowLeft)
                            .tooltip("返回当前页面")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            })),
                    )
                })
                .into_any_element()
        } else if self.library.root.as_os_str().is_empty() {
            v_flex()
                .gap_5()
                .items_center()
                .py_12()
                .child(self.heading(
                    "欢迎来到雀魂藏馆",
                    "打开素材包，开始浏览收藏与培养计划。",
                    p,
                ))
                .child(
                    Button::new("first-open")
                        .primary()
                        .icon(IconName::FolderOpen)
                        .label(text::OPEN_PACKAGE)
                        .on_click(cx.listener(|this, _, _, cx| this.open_package(cx))),
                )
                .into_any_element()
        } else if let Some(index) = self.illustration {
            self.illustration_detail(index, p, cx)
        } else if let Some(index) = self.selected {
            self.detail(index, window, p, cx)
        } else if self.page == Page::Home {
            self.home(window, p, cx)
        } else if self.page == Page::Training {
            self.training(window, p, cx)
        } else if self.page == Page::Inventory {
            self.inventory_page(window, p, cx)
        } else {
            self.collection(window, p, cx)
        };
        let library_page_visible = !self.loading
            && self.error.is_none()
            && self.selected.is_none()
            && self.illustration.is_none()
            && !self.library.root.as_os_str().is_empty();
        let body = div()
            .id("page-scroll")
            .flex_1()
            .min_h_0()
            .px_4()
            .pt_4()
            .pb_2()
            .child(content);
        let body = if library_page_visible || self.selected.is_some() || self.illustration.is_some()
        {
            body.into_any_element()
        } else {
            body.overflow_y_scrollbar().into_any_element()
        };
        v_flex()
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(Self::modal_key_down))
            .relative()
            .size_full()
            .bg(p.bg)
            .text_color(p.text)
            .text_sm()
            .child(self.app_title_bar(window, p, true))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.sidebar(p, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .when(
                                self.library.status != "complete"
                                    && !self.library.status.is_empty(),
                                |d| {
                                    d.child(
                                        div()
                                            .px_4()
                                            .py_1()
                                            .text_color(p.gold)
                                            .child("素材库尚未准备完成，部分图片暂不可用。"),
                                    )
                                },
                            )
                            .child(body),
                    ),
            )
            .when(self.modal.is_some(), |root| {
                root.child(
                    div()
                        .absolute()
                        .top(px(shell::TITLE_BAR_HEIGHT))
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .occlude()
                        .bg(rgba(0x00000060))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.dismiss_modal(cx);
                            }),
                        )
                        .child(self.modal_content(p, cx)),
                )
            })
            .into_any_element()
    }
}
