use bibref_rs::{format_bibtex, BibSearchClient, SourceKind, WorkRecord};
use gpui::{
    div, prelude::*, px, rgb, size, App, Application, AssetSource, Bounds, ClipboardItem, Context,
    Entity, IntoElement, Render, SharedString, TitlebarOptions, Window, WindowBounds,
    WindowOptions,
};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    resizable::{h_resizable, resizable_panel, ResizableState},
    scroll::ScrollableElement,
    IconNamed, Root,
    Disableable,
};
use std::borrow::Cow;

const SEARCH_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path d="M221.09 64a157.09 157.09 0 10157.09 157.09A157.09 157.09 0 00221.09 64z" fill="none" stroke="currentColor" stroke-miterlimit="10" stroke-width="32"/><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-miterlimit="10" stroke-width="32" d="M338.29 338.29L448 448"/></svg>"##;
const REMOVE_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path d="M256 48C141.31 48 48 141.31 48 256s93.31 208 208 208 208-93.31 208-208S370.69 48 256 48z" fill="none" stroke="currentColor" stroke-miterlimit="10" stroke-width="32"/><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-miterlimit="10" stroke-width="32" d="M160 256h192"/></svg>"##;
const COPY_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect x="128" y="128" width="336" height="336" rx="48" ry="48" fill="none" stroke="currentColor" stroke-linejoin="round" stroke-width="32"/><path d="M384 128V80a32 32 0 00-32-32H80a32 32 0 00-32 32v272a32 32 0 0032 32h48" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="32"/></svg>"##;
const OPEN_ICON: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><path d="M384 224v184a40 40 0 01-40 40H104a40 40 0 01-40-40V168a40 40 0 0140-40h184" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="32"/><path d="M336 64h112v112M224 288L440 72" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="32"/></svg>"##;

struct BibRefAssets;

impl AssetSource for BibRefAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "famicons/search-outline.svg" => Some(Cow::Borrowed(SEARCH_ICON)),
            "famicons/remove-outline.svg" => Some(Cow::Borrowed(REMOVE_ICON)),
            "famicons/copy-outline.svg" => Some(Cow::Borrowed(COPY_ICON)),
            "famicons/open-outline.svg" => Some(Cow::Borrowed(OPEN_ICON)),
            _ => None,
        })
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[derive(Clone, Copy)]
enum FamIcon {
    Search,
    Remove,
    Copy,
    Open,
}

impl IconNamed for FamIcon {
    fn path(self) -> SharedString {
        match self {
            Self::Search => "famicons/search-outline.svg",
            Self::Remove => "famicons/remove-outline.svg",
            Self::Copy => "famicons/copy-outline.svg",
            Self::Open => "famicons/open-outline.svg",
        }
        .into()
    }
}

struct BibRefApp {
    search_input: Entity<InputState>,
    bibtex_input: Entity<InputState>,
    panels: Entity<ResizableState>,
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
        let bibtex_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
                .placeholder("Select a search result to preview BibTeX.")
        });
        let panels = cx.new(|_| ResizableState::default());

        Self {
            search_input,
            bibtex_input,
            panels,
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

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.error = None;
        self.status = None;
        cx.notify();
    }

    fn selected_bibtex(&self) -> Option<String> {
        self.selected
            .and_then(|index| self.results.get(index))
            .map(format_bibtex)
    }

    fn author_lines(record: &WorkRecord) -> Vec<String> {
        let names = record
            .authors
            .iter()
            .map(|author| {
                let initials = author
                    .given
                    .as_deref()
                    .unwrap_or_default()
                    .split(|ch: char| ch.is_whitespace() || ch == '-')
                    .filter_map(|part| part.chars().next())
                    .map(|ch| ch.to_uppercase().collect::<String>())
                    .collect::<Vec<_>>()
                    .join(" ");

                if initials.is_empty() {
                    author.family.clone()
                } else {
                    format!("{} {}", initials, author.family)
                }
            })
            .collect::<Vec<_>>();

        match names.len() {
            0 => vec!["Unknown authors".to_string()],
            1..=3 => vec![names.join(", ")],
            len => vec![names[..len - 2].join(", "), names[len - 2..].join(", ")],
        }
    }

    fn venue_line(record: &WorkRecord) -> String {
        let venue = record
            .container_title
            .as_deref()
            .or(record.publisher.as_deref())
            .unwrap_or(match record.source {
                SourceKind::Crossref => "Published work",
                SourceKind::Arxiv => "arXiv preprint",
            });
        let mut parts = Vec::new();
        parts.push(venue.to_string());
        if let Some(year) = record.year {
            parts.push(year.to_string());
        }
        if let Some(publisher) = &record.publisher {
            if record.container_title.as_deref() != Some(publisher.as_str()) {
                parts.push(publisher.clone());
            }
        }
        parts.join(", ")
    }

    fn render_result(&self, index: usize, record: &WorkRecord, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected == Some(index);
        let author_lines = Self::author_lines(record);
        let venue_line = Self::venue_line(record);

        div()
            .flex()
            .flex_col()
            .flex_none()
            .gap_1()
            .mr_4()
            .mb_2()
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
            .children(author_lines.into_iter().map(|line| {
                div()
                    .w_full()
                    .overflow_hidden()
                    .whitespace_normal()
                    .line_clamp(1)
                    .text_xs()
                    .text_color(rgb(0x4f545c))
                    .child(line)
            }))
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .whitespace_normal()
                    .line_clamp(1)
                    .text_xs()
                    .text_color(rgb(0x6d7178))
                    .child(venue_line),
            )
    }
}

impl Render for BibRefApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bibtex = self
            .selected_bibtex()
            .unwrap_or_else(|| "Select a search result to preview BibTeX.".to_string());
        if self.bibtex_input.read(cx).value().to_string() != bibtex {
            self.bibtex_input.update(cx, |state, cx| {
                state.set_value(bibtex.clone(), window, cx);
            });
        }
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

        let left_panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .size_full()
            .p_4()
            .border_r_1()
            .border_color(rgb(0xdadce0))
            .bg(rgb(0xffffff))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("BibTeX Lookup"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Input::new(&self.search_input)
                            .flex_1()
                            .min_w(px(140.0))
                            .suffix(
                                Button::new("clear-search")
                                    .icon(FamIcon::Remove)
                                    .ghost()
                                    .tooltip("Clear search")
                                    .on_click(cx.listener(|this, event, window, cx| {
                                        let _ = event;
                                        this.clear_search(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        Button::new("search")
                            .icon(FamIcon::Search)
                            .tooltip(if self.loading { "Searching" } else { "Search" })
                            .primary()
                            .loading(self.loading)
                            .loading_icon(FamIcon::Search)
                            .disabled(self.loading)
                            .on_click(cx.listener(|this, event, window, cx| {
                                let _ = event;
                                this.search(window, cx)
                            })),
                    ),
            )
            .child(div().text_sm().text_color(rgb(0xb3261e)).child(error))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .pr_2()
                    .h_full()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .children(result_rows)
                    .overflow_y_scrollbar(),
            );

        let right_panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .size_full()
            .min_w(px(0.0))
            .p_4()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("open-selected-url")
                            .icon(FamIcon::Open)
                            .tooltip("Open DOI/arXiv")
                            .outline()
                            .disabled(!has_selected_url)
                            .on_click(cx.listener(|this, event, window, cx| {
                                let _ = event;
                                this.open_result(window, cx)
                            })),
                    )
                    .child(
                        Button::new("copy")
                            .icon(FamIcon::Copy)
                            .tooltip("Copy")
                            .primary()
                            .disabled(self.selected.is_none())
                            .on_click(cx.listener(|this, event, window, cx| {
                                let _ = event;
                                this.copy_selected(window, cx)
                            })),
                    ),
            )
            .when(!status.is_empty(), |this| {
                this.child(div().text_sm().text_color(rgb(0x1a7340)).child(status))
            })
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xdadce0))
                    .bg(rgb(0xffffff))
                    .overflow_hidden()
                    .child(
                        Input::new(&self.bibtex_input)
                            .h_full()
                            .w_full()
                            .font_family("Consolas")
                            .text_sm(),
                    ),
            );

        div()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0xf6f7f9))
            .text_color(rgb(0x202124))
            .child(
                h_resizable("main-panels")
                    .with_state(&self.panels)
                    .child(
                        resizable_panel()
                            .size(px(490.0))
                            .size_range(px(260.0)..px(1200.0))
                            .child(left_panel),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(320.0)..px(2000.0))
                            .child(right_panel),
                    ),
            )
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    Application::new().with_assets(BibRefAssets).run(|cx: &mut App| {
        gpui_component::init(cx);
        let bounds = Bounds::centered(None, size(px(980.0), px(640.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("BibTeX Lookup".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
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
