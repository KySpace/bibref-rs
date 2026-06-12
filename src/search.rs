use crate::metadata::{Author, SourceKind, WorkRecord};
use anyhow::{Context, Result};
use quick_xml::de::from_str;
use serde::Deserialize;
use std::time::Duration;

const CROSSREF_BASE: &str = "https://api.crossref.org/works";
const ARXIV_BASE: &str = "https://export.arxiv.org/api/query";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryKind {
    Doi(String),
    Arxiv(String),
    Bibliographic(String),
}

pub fn classify_query(input: &str) -> QueryKind {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("https://doi.org/") {
        return QueryKind::Doi(trimmed["https://doi.org/".len()..].trim().to_string());
    }
    if let Some(index) = lower.find("10.") {
        let candidate = &trimmed[index..];
        if candidate.contains('/') && !candidate.contains(' ') {
            return QueryKind::Doi(candidate.trim_end_matches('.').to_string());
        }
    }
    if lower.starts_with("10.") && lower.contains('/') {
        return QueryKind::Doi(trimmed.trim_end_matches('.').to_string());
    }

    if let Some(id) = trimmed.strip_prefix("arXiv:") {
        return QueryKind::Arxiv(id.trim().to_string());
    }
    if let Some(rest) = lower.strip_prefix("https://arxiv.org/abs/") {
        return QueryKind::Arxiv(rest.trim().to_string());
    }
    if looks_like_arxiv_id(trimmed) {
        return QueryKind::Arxiv(trimmed.to_string());
    }

    if let Some(query) = citation_key_to_query(trimmed) {
        return QueryKind::Bibliographic(query);
    }

    QueryKind::Bibliographic(trimmed.to_string())
}

fn citation_key_to_query(input: &str) -> Option<String> {
    let key = input.trim();
    if key.contains(char::is_whitespace)
        || key.len() < 8
        || !key.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return None;
    }

    let bytes = key.as_bytes();
    for year_start in 2..=bytes.len().saturating_sub(5) {
        let year_end = year_start + 4;
        let year = &key[year_start..year_end];
        if !year.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }

        let author = &key[..year_start];
        let title_word = &key[year_end..];
        if !author.chars().all(|ch| ch.is_ascii_alphabetic())
            || !title_word.chars().all(|ch| ch.is_ascii_alphabetic())
        {
            continue;
        }

        if !looks_like_year_or_arxiv_month(year) {
            continue;
        }

        return Some(format!("{} {} {}", author, year, title_word));
    }

    None
}

fn looks_like_year_or_arxiv_month(value: &str) -> bool {
    let Ok(number) = value.parse::<i32>() else {
        return false;
    };
    if (1800..=2100).contains(&number) {
        return true;
    }

    let Some(month) = value.get(2..4).and_then(|part| part.parse::<i32>().ok()) else {
        return false;
    };
    (1..=12).contains(&month)
}

fn looks_like_arxiv_id(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.len() < 9 {
        return false;
    }

    let mut split = trimmed.split('.');
    matches!(
        (split.next(), split.next(), split.next()),
        (Some(month), Some(number), None)
            if month.len() == 4
                && number.len() >= 4
                && month.chars().all(|ch| ch.is_ascii_digit())
                && number.chars().all(|ch| ch.is_ascii_digit())
    )
}

#[derive(Clone)]
pub struct BibSearchClient {
    http: reqwest::blocking::Client,
}

impl BibSearchClient {
    pub fn new() -> Result<Self> {
        let user_agent = std::env::var("BIBREF_USER_AGENT").unwrap_or_else(|_| {
            "bibref-rs/0.1 (mailto:please-set-BIBREF_USER_AGENT@example.invalid)".to_string()
        });
        let http = reqwest::blocking::Client::builder()
            .user_agent(user_agent)
            .timeout(Duration::from_secs(12))
            .build()
            .context("building HTTP client")?;

        Ok(Self { http })
    }

    pub fn search(&self, input: &str) -> Result<Vec<WorkRecord>> {
        match classify_query(input) {
            QueryKind::Doi(doi) => self.crossref_doi(&doi).map(|record| vec![record]),
            QueryKind::Arxiv(id) => self.arxiv_id(&id).map(|record| vec![record]),
            QueryKind::Bibliographic(query) => self.search_bibliographic(&query),
        }
    }

    fn search_bibliographic(&self, query: &str) -> Result<Vec<WorkRecord>> {
        let mut records = self.crossref_query(query).unwrap_or_default();

        if records.len() < 3 {
            for arxiv_record in self.arxiv_query(query).unwrap_or_default() {
                merge_or_push(&mut records, arxiv_record);
            }
        }

        Ok(records.into_iter().take(5).collect())
    }

    fn crossref_doi(&self, doi: &str) -> Result<WorkRecord> {
        let url = format!("{}/{}", CROSSREF_BASE, urlencoding::encode(doi));
        let response: CrossrefWorkResponse =
            self.http.get(url).send()?.error_for_status()?.json()?;
        Ok(crossref_item_to_record(response.message))
    }

    fn crossref_query(&self, query: &str) -> Result<Vec<WorkRecord>> {
        let url = format!(
            "{}?query.bibliographic={}&rows=5",
            CROSSREF_BASE,
            urlencoding::encode(query)
        );
        let response: CrossrefSearchResponse =
            self.http.get(url).send()?.error_for_status()?.json()?;
        Ok(response
            .message
            .items
            .into_iter()
            .map(crossref_item_to_record)
            .collect())
    }

    fn arxiv_id(&self, id: &str) -> Result<WorkRecord> {
        let url = format!(
            "{}?id_list={}&max_results=1",
            ARXIV_BASE,
            urlencoding::encode(id)
        );
        let text = self.http.get(url).send()?.error_for_status()?.text()?;
        let feed: ArxivFeed = from_str(&text).context("parsing arXiv Atom response")?;
        feed.entries
            .into_iter()
            .next()
            .map(arxiv_entry_to_record)
            .context("arXiv returned no records")
    }

    fn arxiv_query(&self, query: &str) -> Result<Vec<WorkRecord>> {
        let url = format!(
            "{}?search_query=all:{}&start=0&max_results=5",
            ARXIV_BASE,
            urlencoding::encode(query)
        );
        let text = self.http.get(url).send()?.error_for_status()?.text()?;
        let feed: ArxivFeed = from_str(&text).context("parsing arXiv Atom response")?;
        Ok(feed
            .entries
            .into_iter()
            .map(arxiv_entry_to_record)
            .collect())
    }
}

fn merge_or_push(records: &mut Vec<WorkRecord>, candidate: WorkRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|record| same_work(record, &candidate))
    {
        existing.merge_missing_from(&candidate);
    } else {
        records.push(candidate);
    }
}

fn same_work(left: &WorkRecord, right: &WorkRecord) -> bool {
    match (&left.doi, &right.doi) {
        (Some(left), Some(right)) if left.eq_ignore_ascii_case(right) => return true,
        _ => {}
    }

    normalize_title(&left.title) == normalize_title(&right.title)
}

fn normalize_title(title: &str) -> String {
    title
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ch.is_whitespace())
        .flat_map(|ch| ch.to_lowercase())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Deserialize)]
struct CrossrefWorkResponse {
    message: CrossrefItem,
}

#[derive(Debug, Deserialize)]
struct CrossrefSearchResponse {
    message: CrossrefSearchMessage,
}

#[derive(Debug, Deserialize)]
struct CrossrefSearchMessage {
    #[serde(default, rename = "items")]
    items: Vec<CrossrefItem>,
}

#[derive(Debug, Deserialize)]
struct CrossrefItem {
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    author: Vec<CrossrefAuthor>,
    #[serde(default, rename = "container-title")]
    container_title: Vec<String>,
    #[serde(rename = "published-print")]
    published_print: Option<CrossrefDate>,
    #[serde(rename = "published-online")]
    published_online: Option<CrossrefDate>,
    published: Option<CrossrefDate>,
    volume: Option<String>,
    issue: Option<String>,
    page: Option<String>,
    publisher: Option<String>,
    #[serde(rename = "DOI")]
    doi: Option<String>,
    #[serde(rename = "type")]
    item_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossrefAuthor {
    given: Option<String>,
    family: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossrefDate {
    #[serde(rename = "date-parts")]
    date_parts: Vec<Vec<i32>>,
}

fn crossref_item_to_record(item: CrossrefItem) -> WorkRecord {
    let entry_type = match item.item_type.as_deref() {
        Some("book") | Some("monograph") => "book",
        Some("book-chapter") | Some("book-section") => "incollection",
        Some("proceedings-article") => "inproceedings",
        _ => "article",
    }
    .to_string();

    WorkRecord {
        title: clean_text(item.title.into_iter().next().unwrap_or_default()),
        authors: item
            .author
            .into_iter()
            .filter_map(|author| {
                let family = author.family.or(author.name)?;
                Some(Author {
                    given: author.given,
                    family,
                })
            })
            .collect(),
        year: extract_year(
            item.published_print
                .as_ref()
                .or(item.published_online.as_ref())
                .or(item.published.as_ref()),
        ),
        container_title: item.container_title.into_iter().next().map(clean_text),
        volume: item.volume,
        number: item.issue,
        pages: item.page,
        publisher: item.publisher,
        doi: item.doi.map(|doi| doi.trim().to_string()),
        arxiv_id: None,
        source: SourceKind::Crossref,
        entry_type,
    }
}

fn extract_year(date: Option<&CrossrefDate>) -> Option<i32> {
    date.and_then(|date| date.date_parts.first())
        .and_then(|parts| parts.first())
        .copied()
}

#[derive(Debug, Deserialize)]
struct ArxivFeed {
    #[serde(default, rename = "entry")]
    entries: Vec<ArxivEntry>,
}

#[derive(Debug, Deserialize)]
struct ArxivEntry {
    id: String,
    title: String,
    published: Option<String>,
    #[serde(default, rename = "author")]
    authors: Vec<ArxivAuthor>,
}

#[derive(Debug, Deserialize)]
struct ArxivAuthor {
    name: String,
}

fn arxiv_entry_to_record(entry: ArxivEntry) -> WorkRecord {
    let id = entry
        .id
        .trim()
        .trim_start_matches("https://arxiv.org/abs/")
        .to_string();
    WorkRecord {
        title: clean_text(entry.title),
        authors: entry
            .authors
            .into_iter()
            .map(|author| split_arxiv_author(&author.name))
            .collect(),
        year: entry
            .published
            .as_deref()
            .and_then(|published| published.get(0..4))
            .and_then(|year| year.parse().ok()),
        container_title: Some("arXiv preprint".to_string()),
        volume: None,
        number: None,
        pages: None,
        publisher: None,
        doi: None,
        arxiv_id: Some(id),
        source: SourceKind::Arxiv,
        entry_type: "article".to_string(),
    }
}

fn split_arxiv_author(name: &str) -> Author {
    let name = clean_text(name);
    let mut parts = name.rsplitn(2, ' ');
    let family = parts.next().unwrap_or_default().to_string();
    let given = parts.next().map(|value| value.to_string());
    Author { given, family }
}

fn clean_text(input: impl AsRef<str>) -> String {
    input
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_doi_forms() {
        assert_eq!(
            classify_query("https://doi.org/10.1103/PhysRevLett.1.1"),
            QueryKind::Doi("10.1103/PhysRevLett.1.1".to_string())
        );
        assert_eq!(
            classify_query("10.1103/PhysRevLett.1.1"),
            QueryKind::Doi("10.1103/PhysRevLett.1.1".to_string())
        );
    }

    #[test]
    fn classifies_arxiv_forms() {
        assert_eq!(
            classify_query("arXiv:2401.00001"),
            QueryKind::Arxiv("2401.00001".to_string())
        );
        assert_eq!(
            classify_query("2401.00001"),
            QueryKind::Arxiv("2401.00001".to_string())
        );
    }

    #[test]
    fn classifies_google_scholar_style_citation_keys() {
        assert_eq!(
            classify_query("blakie2023compressibility"),
            QueryKind::Bibliographic("blakie 2023 compressibility".to_string())
        );
        assert_eq!(
            classify_query("Gallemi2025Excitation"),
            QueryKind::Bibliographic("Gallemi 2025 Excitation".to_string())
        );
        assert_eq!(
            classify_query("compressibility"),
            QueryKind::Bibliographic("compressibility".to_string())
        );
    }

    #[test]
    fn classifies_citation_keys_from_samples() {
        let samples = include_str!("../Samples/references.bib");
        let keys = samples
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if !line.starts_with('@') {
                    return None;
                }
                let key_start = line.find('{')? + 1;
                let key_end = line[key_start..].find(',')? + key_start;
                Some(&line[key_start..key_end])
            })
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                "tanzi2019supersolid",
                "natale2019excitation",
                "scheiermann2025excitation",
                "lin2412ai",
                "vsindik2024sound",
                "sanchez2023heating",
                "staub2024new",
                "biss2023probing",
            ]
        );

        let queries = keys.into_iter().map(classify_query).collect::<Vec<_>>();

        assert_eq!(
            queries,
            vec![
                QueryKind::Bibliographic("tanzi 2019 supersolid".to_string()),
                QueryKind::Bibliographic("natale 2019 excitation".to_string()),
                QueryKind::Bibliographic("scheiermann 2025 excitation".to_string()),
                QueryKind::Bibliographic("lin 2412 ai".to_string()),
                QueryKind::Bibliographic("vsindik 2024 sound".to_string()),
                QueryKind::Bibliographic("sanchez 2023 heating".to_string()),
                QueryKind::Bibliographic("staub 2024 new".to_string()),
                QueryKind::Bibliographic("biss 2023 probing".to_string()),
            ]
        );
    }

    #[test]
    fn maps_crossref_json() {
        let item: CrossrefItem = serde_json::from_str(
            r#"{
                "title": ["A Sample Paper"],
                "author": [{"given": "Ada", "family": "Lovelace"}],
                "container-title": ["Journal of Samples"],
                "published-print": {"date-parts": [[2023, 1, 1]]},
                "volume": "7",
                "issue": "2",
                "page": "10-20",
                "publisher": "Example Press",
                "DOI": "10.1234/example",
                "type": "journal-article"
            }"#,
        )
        .unwrap();

        let record = crossref_item_to_record(item);

        assert_eq!(record.title, "A Sample Paper");
        assert_eq!(record.authors[0].family, "Lovelace");
        assert_eq!(record.year, Some(2023));
        assert_eq!(record.doi.as_deref(), Some("10.1234/example"));
    }

    #[test]
    fn maps_arxiv_atom() {
        let feed: ArxivFeed = from_str(
            r#"<feed>
                <entry>
                  <id>https://arxiv.org/abs/2401.00001</id>
                  <title> A Sample Preprint </title>
                  <published>2024-01-01T00:00:00Z</published>
                  <author><name>Ada Lovelace</name></author>
                </entry>
            </feed>"#,
        )
        .unwrap();

        let record = arxiv_entry_to_record(feed.entries.into_iter().next().unwrap());

        assert_eq!(record.title, "A Sample Preprint");
        assert_eq!(record.year, Some(2024));
        assert_eq!(record.arxiv_id.as_deref(), Some("2401.00001"));
        assert_eq!(record.authors[0].family, "Lovelace");
    }
}
