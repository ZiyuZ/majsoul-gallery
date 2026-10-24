use super::*;
use crate::resource_catalog::TitleCategory;

impl Gallery {
    pub(super) fn title_filters(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let categories = std::iter::once(None).chain(
            TitleCategory::ALL
                .into_iter()
                .filter(|category| {
                    self.library
                        .titles
                        .iter()
                        .any(|title| title.category == *category)
                })
                .map(Some),
        );
        div()
            .id("title-category-filters")
            .flex()
            .items_center()
            .min_w_0()
            .h_full()
            .flex_shrink_1()
            .child(
                choice_group("title-categories").children(categories.enumerate().map(
                    |(i, category)| {
                        segment_button(
                            format!("title-category-{i}"),
                            category.map(TitleCategory::label).unwrap_or(text::ALL),
                            self.title_category == category,
                            p,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.title_category = category;
                            cx.notify();
                        }))
                    },
                )),
            )
            .overflow_x_scrollbar()
            .into_any_element()
    }
}
