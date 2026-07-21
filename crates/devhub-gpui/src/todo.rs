use crate::ui::section_label;
use devhub_core::TodoItem;
use devhub_gpui::{Theme, MONO_FONT};
use gpui::prelude::*;
use gpui::*;
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Icon, IconName, Sizable};

pub(crate) struct TodosChanged(pub Vec<TodoItem>);

pub(crate) struct TodoPanel {
    project_name: String,
    theme: Theme,
    items: Vec<TodoItem>,
    input: Entity<InputState>,
    _input_subscription: Subscription,
    scroll: ScrollHandle,
}

impl EventEmitter<TodosChanged> for TodoPanel {}

impl TodoPanel {
    pub(crate) fn new(
        project_name: String,
        items: Vec<TodoItem>,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Add a todo, Shift+Enter to save")
                .auto_grow(1, 6)
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { secondary: true } = event {
                this.add_from_input(window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            project_name,
            theme,
            items,
            input,
            _input_subscription: subscription,
            scroll: ScrollHandle::new(),
        }
    }

    pub(crate) fn set_project(
        &mut self,
        project_name: String,
        items: Vec<TodoItem>,
        cx: &mut Context<Self>,
    ) {
        self.project_name = project_name;
        self.items = items;
        cx.notify();
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub(crate) fn focus_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn add_from_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.items.push(TodoItem::new(text));
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.emit(TodosChanged(self.items.clone()));
        cx.notify();
    }

    fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(item) = self.items.get_mut(index) else {
            return;
        };
        item.done = !item.done;
        cx.emit(TodosChanged(self.items.clone()));
        cx.notify();
    }

    fn remove(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.items.len() {
            return;
        }
        self.items.remove(index);
        cx.emit(TodosChanged(self.items.clone()));
        cx.notify();
    }

    fn render_row(
        &self,
        index: usize,
        item: &TodoItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let done = item.done;
        div()
            .id(("todo-row", index))
            .min_h(px(26.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .border_b_1()
            .border_color(theme.border.opacity(0.35))
            .child(
                Checkbox::new(("todo-check", index))
                    .checked(done)
                    .small()
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle(index, cx))),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_size(px(11.0))
                    .text_color(if done {
                        theme.text_disabled
                    } else {
                        theme.text
                    })
                    .when(done, |label| label.line_through())
                    .child(item.text.clone()),
            )
            .child(
                div()
                    .id(("todo-remove", index))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_color(theme.text_disabled)
                    .hover(move |style| style.text_color(theme.error))
                    .on_click(cx.listener(move |this, _, _, cx| this.remove(index, cx)))
                    .child(Icon::new(IconName::Close).size(px(11.0))),
            )
    }
}

impl Render for TodoPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let open_count = self.items.iter().filter(|item| !item.done).count();
        let done_count = self.items.len() - open_count;

        let rows = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.done)
            .chain(self.items.iter().enumerate().filter(|(_, item)| item.done))
            .map(|(index, item)| self.render_row(index, item, cx).into_any_element())
            .collect::<Vec<_>>();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.panel_background)
            .child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(section_label("TODO", theme))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_disabled)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(self.project_name.clone()),
                    )
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_muted)
                            .child(format!("{open_count} open · {done_count} done")),
                    ),
            )
            .child(
                div()
                    .id("todo-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .when(rows.is_empty(), |list| {
                        list.child(
                            div()
                                .p_3()
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child("No todos yet. Type below and press Enter."),
                        )
                    })
                    .children(rows),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .border_t_1()
                    .border_color(theme.border)
                    .p_1()
                    .child(Input::new(&self.input).appearance(false)),
            )
    }
}
