use bibref_rs::{format_bibtex, BibSearchClient, SourceKind, WorkRecord};
use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, ClipboardItem, Context, Entity,
    IntoElement, Render, SharedString, Window, WindowBounds, WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    Root,
    Disableable,
};

struct BibRefApp {
    search_input: Entity<InputState>,
    client: BibSearchClient,
    results: Vec<WorkRecord>,
    selected: Option<usize>,
    loading: bool,
    error: Option<String>,
    status: Option<String>,
}

impl BibRefApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Title, author, DOI, or arXiv ID")
                .clean_on_escape()
        });

        Self {
            search_input,
            client: BibSearchClient::new().expect("HTTP client"),
            results: Vec::new(),
            selected: None,
            loading: false,
            error: None,
            status: None,
        }
    }

    fn search(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let query = self.search_input.read(cx).value().to_string();
        if query.trim().is_empty() {
            self.error = Some("Enter a DOI, title, author, or arXiv ID.".to_string());
            cx.notify();
            return;
        }

        self.loading = true;
        self.error = None;
        self.status = None;
        self.results.clear();
        self.selected = None;
        cx.notify();

        let client = self.client.clone();
        let search_task = cx.background_spawn(async move { client.search(&query) });
        cx.spawn(async move |this, cx| {
            let result = search_task.await;
            let _ = this.update(cx, |this, cx| {
                this.loading = false;
                match result {
                    Ok(records) if records.is_empty() => {
                        this.error = Some("No matching literature found.".to_string());
                    }
                    Ok(records) => {
                        this.selected = Some(0);
                        this.results = records;
                    }
                    Err(error) => {
                        this.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn select_result(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = Some(index);
        self.status = None;
        cx.notify();
    }

    fn open_result(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(url) = self
            .selected
            .and_then(|index| self.results.get(index))
            .and_then(WorkRecord::external_url)
        {
            cx.open_url(&url);
        }
    }

    fn copy_selected(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(bibtex) = self.selected_bibtex() {
            cx.write_to_clipboard(ClipboardItem::new_string(bibtex));
            self.status = Some("Copied BibTeX to clipboard.".to_string());
            cx.notify();
        }
    }

    fn selected_bibtex(&self) -> Option<String> {
        self.selected
            .and_then(|index| self.results.get(index))
            .map(format_bibtex)
    }

    fn render_result(&self, index: usize, record: &WorkRecord, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected == Some(index);
        let source = match record.source {
            SourceKind::Crossref => "Crossref",
            SourceKind::Arxiv => "arXiv",
        };
        let year = record
            .year
            .map(|year| year.to_string())
            .unwrap_or_else(|| "n.d.".to_string());
        let subtitle = format!("{} - {} - {}", record.author_summary(), year, source);

        div()
            .flex()
            .flex_col()
            .gap_1()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(if selected { rgb(0x2f6fed) } else { rgb(0xd8d8d8) })
            .bg(if selected { rgb(0xeef4ff) } else { rgb(0xffffff) })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    this.select_result(index, window, cx)
                }),
            )
            .child(
                div()
                    .w_full()
                    .text_left()
                    .whitespace_normal()
                    .line_clamp(3)
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(record.title.clone()),
            )
            .child(
                div()
                    .w_full()
                    .truncate()
                    .text_sm()
                    .text_color(rgb(0x555555))
                    .child(subtitle),
            )
    }
}

impl Render for BibRefApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bibtex = self
            .selected_bibtex()
            .unwrap_or_else(|| "Select a search result to preview BibTeX.".to_string());
        let status = self.status.clone().unwrap_or_default();
        let error = self.error.clone().unwrap_or_default();
        let has_selected_url = self
            .selected
            .and_then(|index| self.results.get(index))
            .and_then(WorkRecord::external_url)
            .is_some();
        let result_rows = self
            .results
            .iter()
            .enumerate()
            .map(|(index, record)| self.render_result(index, record, cx))
            .collect::<Vec<_>>();

        div()
            .flex()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0xf6f7f9))
            .text_color(rgb(0x202124))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink()
                    .flex_basis(px(320.0))
                    .gap_3()
                    .min_w(px(240.0))
                    .max_w(px(360.0))
                    .h_full()
                    .p_4()
                    .border_r_1()
                    .border_color(rgb(0xdadce0))
                    .bg(rgb(0xffffff))
                    .child(div().text_lg().font_weight(gpui::FontWeight::SEMIBOLD).child("BibTeX Lookup"))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Input::new(&self.search_input)
                                    .flex_1()
                                    .min_w(px(140.0))
                                    .cleanable(true),
                            )
                            .child(
                                Button::new("search")
                                    .label(if self.loading { "Searching" } else { "Search" })
                                    .primary()
                                    .loading(self.loading)
                                    .disabled(self.loading)
                                    .on_click(cx.listener(|this, event, window, cx| {
                                        let _ = event;
                                        this.search(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xb3261e))
                            .child(error),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .min_w(px(0.0))
                            .children(result_rows),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .p_4()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w(px(180.0))
                                    .flex_1()
                                    .text_lg()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Formatted BibTeX"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .flex_shrink()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("open-selected-url")
                                            .label("Open DOI/arXiv")
                                            .outline()
                                            .disabled(!has_selected_url)
                                            .on_click(cx.listener(|this, event, window, cx| {
                                                let _ = event;
                                                this.open_result(window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("copy")
                                            .label("Copy")
                                            .primary()
                                            .disabled(self.selected.is_none())
                                            .on_click(cx.listener(|this, event, window, cx| {
                                                let _ = event;
                                                this.copy_selected(window, cx)
                                            })),
                                    ),
                            ),
                    )
                    .child(div().text_sm().text_color(rgb(0x1a7340)).child(status))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .p_4()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(0xdadce0))
                            .bg(rgb(0xffffff))
                            .overflow_hidden()
                            .font_family("Consolas")
                            .text_sm()
                            .child(SharedString::from(bibtex)),
                    ),
            )
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(980.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| BibRefApp::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
