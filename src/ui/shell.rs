//! Shared application chrome: title bar, navigation and account utilities.
use super::*;

pub(super) const TITLE_BAR_HEIGHT: f32 = 48.;

impl Gallery {
    pub(super) fn app_title_bar(&self, window: &Window, p: Palette, browsing: bool) -> AnyElement {
        let search_width = (f32::from(window.viewport_size().width) - 650.).clamp(240., 360.);
        TitleBar::new()
            .h(px(TITLE_BAR_HEIGHT))
            .bg(p.sidebar)
            .text_color(p.text)
            .border_color(p.border)
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .h_full()
                    .gap_3()
                    .pr_4()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .flex_shrink_0()
                            .items_center()
                            .child(app_icon(26., window.scale_factor()))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(text::APP_NAME),
                            ),
                    )
                    .child(div().flex_1().min_w(px(64.)))
                    .when(browsing, |bar| {
                        bar.child(
                            div()
                                .id("titlebar-search")
                                .w(px(search_width))
                                .flex_shrink_0()
                                // Exclude this surface from the title bar's drag hitbox.
                                .occlude()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                                .child(
                                    Input::new(&self.search)
                                        .disabled(self.loading || self.modal.is_some())
                                        .prefix(Icon::new(IconName::Search).size_4()),
                                ),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(super) fn sidebar_width(&self) -> f32 {
        if self.sidebar_compact { 64. } else { 218. }
    }

    fn sidebar_toggle(&self, cx: &mut Context<Self>) -> Button {
        Button::new("toggle-sidebar")
            .ghost()
            .icon(if self.sidebar_compact {
                IconName::PanelLeftOpen
            } else {
                IconName::PanelLeftClose
            })
            .tooltip(if self.sidebar_compact {
                text::EXPAND_SIDEBAR
            } else {
                text::COLLAPSE_SIDEBAR
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.sidebar_compact = !this.sidebar_compact;
                cx.notify();
            }))
    }

    pub(super) fn sidebar(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let compact = self.sidebar_compact;
        v_flex()
            .w(px(self.sidebar_width()))
            .h_full()
            .flex_shrink_0()
            .px(if compact { px(8.) } else { px(16.) })
            .py_3()
            .gap_2()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .children(
                [
                    (Page::Home, text::HOME, IconName::House),
                    (Page::Collection, text::COLLECTION, IconName::LayoutGrid),
                    (Page::Training, text::TRAINING, IconName::Heart),
                ]
                .into_iter()
                .map(|(page, label, icon)| {
                    Button::new(match page {
                        Page::Home => "nav-home",
                        Page::Collection => "nav-collection",
                        Page::Training => "nav-training",
                    })
                    .ghost()
                    .w_full()
                    .icon(Icon::new(icon).size(if compact { px(16.) } else { px(20.) }))
                    .when(compact, |button| button.tooltip(label))
                    .selected(self.page == page)
                    .when(!compact, |button| {
                        button.justify_start().h(px(48.)).text_lg().label(label)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.navigate(page, cx)))
                }),
            )
            .child(div().flex_1())
            .child(self.account_area(p, cx))
            .child(self.sidebar_utilities(cx))
            .into_any_element()
    }

    fn sidebar_utilities(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .gap_1()
            .when(self.sidebar_compact, |row| row.flex_col())
            .when(!self.sidebar_compact, |row| row.justify_center())
            .child(
                Button::new("share-export")
                    .ghost()
                    .icon(IconName::Download)
                    .tooltip(text::EXPORT)
                    .disabled(self.library.characters.is_empty())
                    .when(self.sidebar_compact, |button| button.w_full())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.modal = Some(Modal::Export);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("settings")
                    .ghost()
                    .icon(IconName::Settings)
                    .tooltip(text::SETTINGS)
                    .when(self.sidebar_compact, |button| button.w_full())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.modal = Some(Modal::Settings);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("theme")
                    .ghost()
                    .icon(if self.dark {
                        IconName::Sun
                    } else {
                        IconName::Moon
                    })
                    .tooltip(text::TOGGLE_THEME)
                    .when(self.sidebar_compact, |button| button.w_full())
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx))),
            )
            .child(
                self.sidebar_toggle(cx)
                    .when(self.sidebar_compact, |button| button.w_full()),
            )
            .into_any_element()
    }
}
