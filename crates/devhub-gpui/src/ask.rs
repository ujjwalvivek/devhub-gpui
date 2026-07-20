use std::collections::{HashMap, VecDeque};
use std::sync::mpsc;
use std::time::Duration;

use devhub_core::{
    delete_zen_api_key, fetch_opencode_models, load_or_build_project_context,
    parse_architecture_response, question_excerpts, store_zen_api_key, stream_opencode_answer,
    ArchitectureGraph, ArchitectureResponse, CancellationToken, Project, ZenError, ZenErrorKind,
    ZenModel,
};
use devhub_gpui::{omit_markdown_images, Theme, MONO_FONT, UI_FONT};
use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::text::{TextView, TextViewStyle};
use gpui_component::{Disableable, IconName, Sizable};

const STREAM_TICK: Duration = Duration::from_millis(24);

pub(crate) struct CloseAskPanel;
pub(crate) struct OpenAskPath(pub std::path::PathBuf);

pub(crate) struct AskPanel {
    window_handle: AnyWindowHandle,
    project: Project,
    theme: Theme,
    api_key_input: Entity<InputState>,
    _api_key_subscription: Subscription,
    api_key: String,
    prompt_input: Entity<InputState>,
    _prompt_subscription: Subscription,
    prompt: String,
    models: LoadState<Vec<ZenModel>>,
    selected_model: Option<ZenModel>,
    model_menu_open: bool,
    exchanges: Vec<Exchange>,
    status: Option<String>,
    generation: u64,
    cancellation: Option<CancellationToken>,
    scroll: ScrollHandle,
}

impl EventEmitter<CloseAskPanel> for AskPanel {}
impl EventEmitter<OpenAskPath> for AskPanel {}

impl Drop for AskPanel {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Clone)]
struct Exchange {
    question: String,
    answer: String,
    pending: bool,
    diagram: Option<ArchitectureGraph>,
    diagram_error: Option<String>,
}

enum LoadState<T> {
    Idle,
    Loading,
    Loaded(T),
    NeedsCredential,
    Error(String),
}

enum StreamEvent {
    Status(String),
    Delta(String),
    Diagram(Result<ArchitectureResponse, String>),
    Finished(Result<(), ZenError>),
}

impl AskPanel {
    pub(crate) fn new(
        project: Project,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let api_key_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("OpenCode API key")
                .masked(true)
        });
        let _api_key_subscription =
            cx.subscribe_in(&api_key_input, window, Self::on_api_key_input_event);
        let prompt_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Ask about this project")
                .multi_line(true)
                .rows(3)
        });
        let _prompt_subscription =
            cx.subscribe_in(&prompt_input, window, Self::on_prompt_input_event);

        let mut panel = Self {
            window_handle: window.window_handle(),
            project,
            theme,
            api_key_input,
            _api_key_subscription,
            api_key: String::new(),
            prompt_input,
            _prompt_subscription,
            prompt: String::new(),
            models: LoadState::Idle,
            selected_model: None,
            model_menu_open: false,
            exchanges: Vec::new(),
            status: None,
            generation: 0,
            cancellation: None,
            scroll: ScrollHandle::new(),
        };
        panel.load_models(cx);
        panel
    }

    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub(crate) fn set_project(&mut self, project: Project, cx: &mut Context<Self>) {
        if self.project == project {
            return;
        }
        self.cancel();
        self.project = project;
        self.exchanges.clear();
        self.status = None;
        self.model_menu_open = false;
        self.prompt.clear();
        self.generation = self.generation.wrapping_add(1);
        cx.notify();
    }

    pub(crate) fn end_session(&mut self) {
        self.cancel();
        self.generation = self.generation.wrapping_add(1);
        self.exchanges.clear();
        self.status = None;
    }

    fn on_api_key_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.api_key = state.read(cx).value().to_string();
                cx.notify();
            }
            InputEvent::PressEnter { .. } => self.connect(cx),
            _ => {}
        }
    }

    fn on_prompt_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.prompt = state.read(cx).value().to_string();
                cx.notify();
            }
            InputEvent::PressEnter { secondary: true } => self.submit(window, cx),
            _ => {}
        }
    }

    fn load_models(&mut self, cx: &mut Context<Self>) {
        self.cancel();
        self.models = LoadState::Loading;
        self.status = Some("Connecting to OpenCode...".into());
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let cancellation = CancellationToken::new();
        self.cancellation = Some(cancellation.clone());

        let task = cx
            .background_executor()
            .spawn(async move { fetch_opencode_models(&cancellation) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.cancellation = None;
                this.status = None;
                match result {
                    Ok(models) if models.is_empty() => {
                        this.models = LoadState::Error("OpenCode returned no models.".into());
                    }
                    Ok(models) => {
                        let selected_exists = this
                            .selected_model
                            .as_ref()
                            .is_some_and(|selected| models.iter().any(|model| model == selected));
                        if !selected_exists {
                            this.selected_model = models
                                .iter()
                                .find(|model| model.free)
                                .or_else(|| models.first())
                                .cloned();
                        }
                        this.models = LoadState::Loaded(models);
                    }
                    Err(error) if error.kind == ZenErrorKind::Credential => {
                        this.models = LoadState::NeedsCredential;
                    }
                    Err(error) => {
                        this.models = LoadState::Error(format!(
                            "{} {}",
                            error.status_text(),
                            concise_error(&error.detail)
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let key = self.api_key.trim().to_string();
        if key.is_empty() || matches!(self.models, LoadState::Loading) {
            return;
        }
        self.cancel();
        self.models = LoadState::Loading;
        self.status = Some("Saving key...".into());
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let cancellation = CancellationToken::new();
        self.cancellation = Some(cancellation.clone());
        let task = cx.background_executor().spawn(async move {
            store_zen_api_key(&key)?;
            fetch_opencode_models(&cancellation)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.cancellation = None;
                this.status = None;
                match result {
                    Ok(models) if !models.is_empty() => {
                        this.selected_model = models
                            .iter()
                            .find(|model| model.free)
                            .or_else(|| models.first())
                            .cloned();
                        this.models = LoadState::Loaded(models);
                        this.clear_api_key(cx);
                    }
                    Ok(_) => this.models = LoadState::Error("OpenCode returned no models.".into()),
                    Err(error) => {
                        this.models = LoadState::Error(format!(
                            "{} {}",
                            error.status_text(),
                            concise_error(&error.detail)
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let question = self.prompt.trim().to_string();
        let Some(model) = self.selected_model.clone() else {
            return;
        };
        if question.is_empty() || self.cancellation.is_some() {
            return;
        }

        self.prompt.clear();
        self.prompt_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.exchanges.push(Exchange {
            question: question.clone(),
            answer: String::new(),
            pending: true,
            diagram: None,
            diagram_error: None,
        });
        self.status = Some("Reading project...".into());
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let project = self.project.clone();
        let cancellation = CancellationToken::new();
        self.cancellation = Some(cancellation.clone());
        self.scroll.scroll_to_bottom();

        let (sender, receiver) = mpsc::sync_channel::<StreamEvent>(256);
        cx.background_executor()
            .spawn(async move {
                let result = (|| {
                    let context = load_or_build_project_context(&project, &cancellation).map_err(
                        |error| ZenError {
                            kind: ZenErrorKind::Provider,
                            detail: error,
                        },
                    )?;
                    let extra = question_excerpts(&project, &context, &question, &cancellation)
                        .map_err(|error| ZenError {
                            kind: ZenErrorKind::Provider,
                            detail: error,
                        })?;
                    let prompt = context.prompt_for(&question, &extra);
                    let _ = sender.send(StreamEvent::Status("Answering...".into()));
                    let mut complete_answer = String::new();
                    stream_opencode_answer(&model, &prompt, &cancellation, |delta| {
                        complete_answer.push_str(&delta);
                        let _ = sender.send(StreamEvent::Delta(delta));
                    })?;
                    let parsed = match parse_architecture_response(
                        &complete_answer,
                        &context.repository_map,
                    ) {
                        Ok(parsed) => Ok(parsed),
                        Err(first_error) => {
                            let repair_prompt = format!(
                                "Repair the invalid architecture diagram below. Return a concise answer and exactly one valid `devhub-diagram` fence. Keep every path within the supplied repository map. Validation error: {first_error}\n\n{prompt}\n\nInvalid response:\n{complete_answer}"
                            );
                            let mut repaired = String::new();
                            match stream_opencode_answer(
                                &model,
                                &repair_prompt,
                                &cancellation,
                                |delta| repaired.push_str(&delta),
                            ) {
                                Ok(()) => parse_architecture_response(
                                    &repaired,
                                    &context.repository_map,
                                ),
                                Err(error) => Err(error.to_string()),
                            }
                        }
                    };
                    let _ = sender.send(StreamEvent::Diagram(parsed));
                    Ok(())
                })();
                let _ = sender.send(StreamEvent::Finished(result));
            })
            .detach();

        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(STREAM_TICK).await;
            let mut finished = false;
            let active = this
                .update(cx, |this, cx| {
                    if this.generation != generation {
                        return false;
                    }
                    while let Ok(event) = receiver.try_recv() {
                        match event {
                            StreamEvent::Status(status) => this.status = Some(status),
                            StreamEvent::Delta(delta) => {
                                if let Some(exchange) = this.exchanges.last_mut() {
                                    exchange.answer.push_str(&delta);
                                }
                                this.scroll.scroll_to_bottom();
                            }
                            StreamEvent::Diagram(result) => {
                                if let Some(exchange) = this.exchanges.last_mut() {
                                    match result {
                                        Ok(parsed) => {
                                            exchange.answer = parsed.narrative;
                                            exchange.diagram = parsed.graph;
                                            exchange.diagram_error = None;
                                        }
                                        Err(error) => exchange.diagram_error = Some(error),
                                    }
                                }
                            }
                            StreamEvent::Finished(result) => {
                                this.cancellation = None;
                                this.status = None;
                                if let Some(exchange) = this.exchanges.last_mut() {
                                    exchange.pending = false;
                                    if let Err(error) = result {
                                        exchange.answer = if error.kind == ZenErrorKind::Cancelled {
                                            "Answer cancelled.".into()
                                        } else {
                                            format!(
                                                "{} {}",
                                                error.status_text(),
                                                concise_error(&error.detail)
                                            )
                                        };
                                    } else if exchange.answer.trim().is_empty() {
                                        exchange.answer =
                                            "OpenCode returned an empty answer.".into();
                                    }
                                }
                                finished = true;
                            }
                        }
                    }
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !active || finished {
                break;
            }
        })
        .detach();
    }

    fn cancel(&mut self) {
        if let Some(cancellation) = self.cancellation.take() {
            cancellation.cancel();
        }
    }

    fn forget_credential(&mut self, cx: &mut Context<Self>) {
        self.cancel();
        self.models = LoadState::Loading;
        self.status = Some("Removing key...".into());
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let task = cx
            .background_executor()
            .spawn(async move { delete_zen_api_key() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.status = None;
                match result {
                    Ok(()) => {
                        this.models = LoadState::NeedsCredential;
                        this.selected_model = None;
                        this.clear_api_key(cx);
                    }
                    Err(error) => {
                        this.models = LoadState::Error(format!(
                            "{} {}",
                            error.status_text(),
                            concise_error(&error.detail)
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn selected_model_is_free(&self) -> bool {
        self.selected_model.as_ref().is_some_and(|model| model.free)
    }

    fn clear_api_key(&mut self, cx: &mut Context<Self>) {
        self.api_key.clear();
        let input = self.api_key_input.clone();
        let _ = cx.update_window(self.window_handle, |_, window, cx| {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        });
    }

    fn render_setup(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.entity();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .justify_center()
            .gap_3()
            .px_4()
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Connect OpenCode"),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.text_muted)
                    .child(
                        "One key connects Zen and Go. It is protected by your operating system.",
                    ),
            )
            .child(
                div()
                    .h(px(30.0))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface_background)
                    .child(Input::new(&self.api_key_input).appearance(false).px_2()),
            )
            .child(
                Button::new("connect-zen")
                    .label("Connect")
                    .small()
                    .primary()
                    .disabled(self.api_key.trim().is_empty())
                    .on_click(move |_, _, cx| {
                        app.update(cx, |this, cx| this.connect(cx));
                    }),
            )
            .into_any_element()
    }

    fn render_error(&self, message: String, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.entity();
        let replace = app.clone();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_3()
            .px_4()
            .text_center()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme.error)
                    .child(message),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(Button::new("retry-zen").label("Retry").small().on_click(
                        move |_, _, cx| {
                            app.update(cx, |this, cx| this.load_models(cx));
                        },
                    ))
                    .child(
                        Button::new("replace-zen-key")
                            .label("Replace key")
                            .small()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                replace.update(cx, |this, cx| this.forget_credential(cx));
                            }),
                    ),
            )
            .into_any_element()
    }

    fn render_conversation(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let diagram_starter = cx.entity();
        let exchanges = self
            .exchanges
            .iter()
            .enumerate()
            .map(|(index, exchange)| {
                let answer = if exchange.answer.is_empty() && exchange.pending {
                    self.status.clone().unwrap_or_else(|| "Thinking...".into())
                } else {
                    exchange.answer.clone()
                };
                div()
                    .id(("ask-exchange", index))
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .py_3()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.45))
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_disabled)
                            .child("YOU"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme.text)
                            .child(exchange.question.clone()),
                    )
                    .child(
                        div()
                            .mt_1()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.accent)
                            .child("DEVHUB"),
                    )
                    .child(if answer.is_empty() {
                        div().into_any_element()
                    } else if exchange.pending && exchange.answer.is_empty() {
                        div()
                            .text_size(px(11.0))
                            .text_color(theme.text_muted)
                            .child(answer)
                            .into_any_element()
                    } else {
                        TextView::markdown(
                            ("ask-answer", index),
                            omit_markdown_images(&answer),
                            window,
                            cx,
                        )
                        .style(TextViewStyle {
                            heading_base_font_size: px(12.0),
                            highlight_theme: gpui_component::Theme::global(cx)
                                .highlight_theme
                                .clone(),
                            is_dark: !theme.is_light,
                            ..Default::default()
                        })
                        .selectable(true)
                        .font_family(UI_FONT)
                        .text_size(px(12.0))
                        .text_color(theme.text)
                        .into_any_element()
                    })
                    .when_some(exchange.diagram.as_ref(), |entry, graph| {
                        entry.child(self.render_architecture_graph(graph, theme, index, cx))
                    })
                    .when_some(exchange.diagram_error.as_ref(), |entry, error| {
                        entry.child(
                            div()
                                .font_family(MONO_FONT)
                                .text_size(px(9.0))
                                .text_color(theme.error)
                                .child(format!("Diagram unavailable: {error}")),
                        )
                    })
            })
            .collect::<Vec<_>>();

        div()
            .id("ask-conversation")
            .min_h_0()
            .flex_1()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px_3()
            .when(self.exchanges.is_empty(), |view| {
                view.flex().items_center().justify_center().child(
                    div()
                        .max_w(px(240.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .text_center()
                        .text_size(px(11.0))
                        .text_color(theme.text_muted)
                        .child("Ask anything about this project.")
                        .child(
                            Button::new("ask-diagram-starter")
                                .label("Ask for an architecture diagram")
                                .small()
                                .ghost()
                                .on_click(move |_, window, cx| {
                                    diagram_starter.update(cx, |this, cx| {
                                        let prompt =
                                            "Show me an architecture diagram for this project.";
                                        this.prompt = prompt.into();
                                        this.prompt_input.update(cx, |input, cx| {
                                            input.set_value(prompt, window, cx);
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    });
                                }),
                        ),
                )
            })
            .children(exchanges)
            .into_any_element()
    }

    fn render_architecture_graph(
        &self,
        graph: &ArchitectureGraph,
        theme: Theme,
        exchange_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const NODE_WIDTH: f32 = 126.0;
        const NODE_HEIGHT: f32 = 58.0;
        const COLUMN_GAP: f32 = 42.0;
        const ROW_GAP: f32 = 22.0;
        const PADDING: f32 = 12.0;

        let node_indices = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id.as_str(), index))
            .collect::<HashMap<_, _>>();
        let mut indegrees = vec![0usize; graph.nodes.len()];
        let mut outgoing = vec![Vec::<usize>::new(); graph.nodes.len()];
        for edge in &graph.edges {
            let (Some(&from), Some(&to)) = (
                node_indices.get(edge.from.as_str()),
                node_indices.get(edge.to.as_str()),
            ) else {
                continue;
            };
            indegrees[to] += 1;
            outgoing[from].push(to);
        }
        let mut levels = vec![0usize; graph.nodes.len()];
        let mut queue = indegrees
            .iter()
            .enumerate()
            .filter_map(|(index, indegree)| (*indegree == 0).then_some(index))
            .collect::<VecDeque<_>>();
        let mut visited = vec![false; graph.nodes.len()];
        while let Some(index) = queue.pop_front() {
            visited[index] = true;
            for &target in &outgoing[index] {
                levels[target] = levels[target].max(levels[index].saturating_add(1));
                indegrees[target] = indegrees[target].saturating_sub(1);
                if indegrees[target] == 0 {
                    queue.push_back(target);
                }
            }
        }
        let fallback_level = levels.iter().copied().max().unwrap_or_default();
        for (index, was_visited) in visited.into_iter().enumerate() {
            if !was_visited {
                levels[index] = fallback_level;
            }
        }
        let column_count = levels.iter().copied().max().unwrap_or_default() + 1;
        let mut row_counts = vec![0usize; column_count];
        let mut positions = Vec::with_capacity(graph.nodes.len());
        for (index, level) in levels.into_iter().enumerate() {
            let row = row_counts[level];
            row_counts[level] += 1;
            positions.push((
                PADDING + level as f32 * (NODE_WIDTH + COLUMN_GAP),
                PADDING + row as f32 * (NODE_HEIGHT + ROW_GAP),
                index,
            ));
        }
        let width = PADDING * 2.0
            + column_count as f32 * NODE_WIDTH
            + column_count.saturating_sub(1) as f32 * COLUMN_GAP;
        let height = PADDING * 2.0
            + row_counts.iter().copied().max().unwrap_or(1) as f32 * NODE_HEIGHT
            + row_counts
                .iter()
                .copied()
                .max()
                .unwrap_or(1)
                .saturating_sub(1) as f32
                * ROW_GAP;
        let by_index = positions
            .iter()
            .map(|(x, y, index)| (*index, (*x, *y)))
            .collect::<HashMap<_, _>>();
        let edge_lines = graph
            .edges
            .iter()
            .filter_map(|edge| {
                let from = *node_indices.get(edge.from.as_str())?;
                let to = *node_indices.get(edge.to.as_str())?;
                let (from_x, from_y) = *by_index.get(&from)?;
                let (to_x, to_y) = *by_index.get(&to)?;
                Some((
                    from_x + NODE_WIDTH,
                    from_y + NODE_HEIGHT / 2.0,
                    to_x,
                    to_y + NODE_HEIGHT / 2.0,
                ))
            })
            .collect::<Vec<_>>();
        let line_color = theme.border_strong;
        let project_root = self.project.path.clone();
        let entity = cx.entity();
        let nodes = positions.into_iter().map(|(x, y, index)| {
            let node = graph.nodes[index].clone();
            let path = node.path.clone();
            let entity = entity.clone();
            let project_root = project_root.clone();
            div()
                .id((
                    "diagram-node",
                    ((exchange_index as u64) << 32) | index as u64,
                ))
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(NODE_WIDTH))
                .h(px(NODE_HEIGHT))
                .flex()
                .flex_col()
                .justify_center()
                .gap_1()
                .px_2()
                .border_1()
                .border_color(theme.border_strong)
                .bg(theme.surface_background)
                .when(path.is_some(), |node| {
                    node.cursor_pointer()
                        .hover(move |style| style.bg(theme.surface_hover))
                        .on_click(move |_, _, cx| {
                            if let Some(path) = path.as_ref() {
                                entity.update(cx, |_this, cx| {
                                    cx.emit(OpenAskPath(project_root.join(path)));
                                });
                            }
                        })
                })
                .child(
                    div()
                        .text_size(px(10.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(node.label),
                )
                .when(!node.detail.is_empty(), |entry| {
                    entry.child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(8.0))
                            .text_color(theme.text_muted)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(node.detail),
                    )
                })
        });

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .font_family(MONO_FONT)
                    .text_size(px(9.0))
                    .text_color(theme.text_muted)
                    .child(graph.title.clone()),
            )
            .child(
                div()
                    .id(("architecture-diagram", exchange_index))
                    .w_full()
                    .overflow_x_scroll()
                    .child(
                        div()
                            .relative()
                            .w(px(width.max(280.0)))
                            .h(px(height.max(96.0)))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, _, window, _| {
                                        for (from_x, from_y, to_x, to_y) in &edge_lines {
                                            let start = point(
                                                bounds.origin.x + px(*from_x),
                                                bounds.origin.y + px(*from_y),
                                            );
                                            let end = point(
                                                bounds.origin.x + px(*to_x),
                                                bounds.origin.y + px(*to_y),
                                            );
                                            let middle_x = start.x + (end.x - start.x) / 2.0;
                                            let mut builder = PathBuilder::stroke(px(1.0));
                                            builder.move_to(start);
                                            builder.line_to(point(middle_x, start.y));
                                            builder.line_to(point(middle_x, end.y));
                                            builder.line_to(end);
                                            if let Ok(path) = builder.build() {
                                                window.paint_path(path, line_color);
                                            }
                                            let mut arrow = PathBuilder::stroke(px(1.0));
                                            arrow.move_to(point(end.x - px(5.0), end.y - px(3.0)));
                                            arrow.line_to(end);
                                            arrow.line_to(point(end.x - px(5.0), end.y + px(3.0)));
                                            if let Ok(path) = arrow.build() {
                                                window.paint_path(path, line_color);
                                            }
                                        }
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            )
                            .children(nodes),
                    ),
            )
            .into_any_element()
    }

    fn render_model_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.model_menu_open {
            return None;
        }
        let LoadState::Loaded(models) = &self.models else {
            return None;
        };
        let rows = models.iter().enumerate().map(|(index, model)| {
            let selected_model = model.clone();
            let selected = self.selected_model.as_ref() == Some(model);
            let app = cx.entity();
            div()
                .id(("zen-model", index))
                .h(px(26.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .border_b_1()
                .border_color(theme.border.opacity(0.35))
                .bg(if selected {
                    theme.surface_selected
                } else {
                    theme.surface_background
                })
                .hover(move |style| style.bg(theme.surface_hover))
                .cursor_pointer()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .font_family(MONO_FONT)
                        .text_size(px(10.0))
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(model.id.clone()),
                )
                .child(
                    div()
                        .font_family(MONO_FONT)
                        .text_size(px(9.0))
                        .text_color(theme.text_disabled)
                        .child(model.service.label()),
                )
                .when(model.free, |row| {
                    row.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme.success)
                            .child("FREE"),
                    )
                })
                .on_click(move |_, _, cx| {
                    let selected_model = selected_model.clone();
                    app.update(cx, |this, cx| {
                        this.selected_model = Some(selected_model);
                        this.model_menu_open = false;
                        cx.notify();
                    });
                })
        });
        Some(
            div()
                .id("zen-model-menu")
                .absolute()
                .top(px(28.0))
                .left(px(8.0))
                .right(px(8.0))
                .max_h(px(260.0))
                .overflow_y_scroll()
                .border_1()
                .border_color(theme.border)
                .bg(theme.surface_background)
                .shadow_md()
                .children(rows)
                .into_any_element(),
        )
    }
}

impl Render for AskPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let close = cx.entity();
        let model_menu = cx.entity();
        let submit = cx.entity();
        let cancel = cx.entity();
        let selected_model = self
            .selected_model
            .as_ref()
            .map(|model| format!("{} · {}", model.service.label(), model.id))
            .unwrap_or_else(|| "Select model".into());
        let working = self.cancellation.is_some();
        let model_style = ButtonCustomVariant::new(cx)
            .foreground(theme.text_muted)
            .hover(theme.surface_hover)
            .active(theme.surface_selected);

        let body = match &self.models {
            LoadState::Idle | LoadState::Loading => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(self.status.clone().unwrap_or_else(|| "Loading...".into()))
                .into_any_element(),
            LoadState::NeedsCredential => self.render_setup(theme, cx),
            LoadState::Error(message) => self.render_error(message.clone(), theme, cx),
            LoadState::Loaded(_) => div()
                .min_h_0()
                .flex_1()
                .flex()
                .flex_col()
                .child(self.render_conversation(theme, window, cx))
                .child(
                    div()
                        .flex_shrink_0()
                        .p_2()
                        .border_t_1()
                        .border_color(theme.border)
                        .child(
                            div()
                                .min_h(px(72.0))
                                .flex()
                                .flex_col()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface_background)
                                .child(
                                    Input::new(&self.prompt_input)
                                        .appearance(false)
                                        .min_h(px(48.0))
                                        .px_2()
                                        .py_1(),
                                )
                                .child(
                                    div()
                                        .h(px(24.0))
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .px_1()
                                        .child(if working {
                                            Button::new("cancel-ask")
                                                .icon(IconName::CircleX)
                                                .tooltip("Cancel answer")
                                                .xsmall()
                                                .compact()
                                                .ghost()
                                                .on_click(move |_, _, cx| {
                                                    cancel.update(cx, |this, cx| {
                                                        this.cancel();
                                                        cx.notify();
                                                    });
                                                })
                                                .into_any_element()
                                        } else {
                                            Button::new("submit-ask")
                                                .icon(IconName::ArrowUp)
                                                .tooltip("Ask (Ctrl+Enter)")
                                                .xsmall()
                                                .compact()
                                                .primary()
                                                .disabled(self.prompt.trim().is_empty())
                                                .on_click(move |_, window, cx| {
                                                    submit.update(cx, |this, cx| {
                                                        this.submit(window, cx);
                                                    });
                                                })
                                                .into_any_element()
                                        }),
                                ),
                        )
                        .when(self.selected_model_is_free(), |footer| {
                            footer.child(
                                div()
                                    .mt_1()
                                    .text_size(px(9.0))
                                    .text_color(theme.warning)
                                    .child(
                                    "Free Zen models may retain prompts. Avoid confidential code.",
                                ),
                            )
                        }),
                )
                .into_any_element(),
        };

        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.panel_background)
            .font_family(UI_FONT)
            .text_color(theme.text)
            .child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(11.0))
                            .child("Ask Project"),
                    )
                    .child(div().flex_1())
                    .when(matches!(self.models, LoadState::Loaded(_)), |header| {
                        header.child(
                            Button::new("zen-model-picker")
                                .label(selected_model)
                                .tooltip("Choose OpenCode model")
                                .xsmall()
                                .compact()
                                .custom(model_style)
                                .max_w(px(170.0))
                                .on_click(move |_, _, cx| {
                                    model_menu.update(cx, |this, cx| {
                                        this.model_menu_open = !this.model_menu_open;
                                        cx.notify();
                                    });
                                }),
                        )
                    })
                    .child(
                        Button::new("close-ask-project")
                            .icon(IconName::Close)
                            .tooltip("Close Ask Project")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                close.update(cx, |_this, cx| cx.emit(CloseAskPanel));
                            }),
                    ),
            )
            .child(body)
            .when_some(self.render_model_menu(theme, cx), |panel, menu| {
                panel.child(menu)
            })
    }
}

fn concise_error(detail: &str) -> String {
    let detail = detail.trim();
    if detail.len() <= 180 {
        detail.to_string()
    } else {
        format!("{}...", &detail[..180])
    }
}
