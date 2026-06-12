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
    CitationKey(CitationKeyQuery),
    Bibliographic(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationKeyQuery {
    pub author: String,
    pub year_fragment: String,
    pub title_word: String,
    pub publication_year: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterField {
    General,
    Author,
    Journal,
    Year,
    Title,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchFilter {
    pub field: FilterField,
    pub keyword: String,
}

impl SearchFilter {
    pub fn new(field: FilterField, keyword: impl Into<String>) -> Self {
        Self {
            field,
            keyword: keyword.into(),
        }
    }
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

    if let Some(query) = parse_citation_key(trimmed) {
        return QueryKind::CitationKey(query);
    }

    QueryKind::Bibliographic(trimmed.to_string())
}

fn parse_citation_key(input: &str) -> Option<CitationKeyQuery> {
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

        let publication_year = publication_year(year);
        if publication_year.is_none() && !looks_like_arxiv_month(year) {
            continue;
        }

        return Some(CitationKeyQuery {
            author: author.to_string(),
            year_fragment: year.to_string(),
            title_word: title_word.to_string(),
            publication_year,
        });
    }

    None
}

fn publication_year(value: &str) -> Option<i32> {
    value
        .parse::<i32>()
        .ok()
        .filter(|year| (1800..=2100).contains(year))
}

fn looks_like_arxiv_month(value: &str) -> bool {
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
        self.search_with_filter(input, None)
    }

    pub fn search_with_filter(
        &self,
        input: &str,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<WorkRecord>> {
        match classify_query(input) {
            QueryKind::Doi(doi) => self
                .crossref_doi(&doi)
                .map(|record| filtered_records(vec![record], filter)),
            QueryKind::Arxiv(id) => self
                .arxiv_id(&id)
                .map(|record| filtered_records(vec![record], filter)),
            QueryKind::CitationKey(query) => self.search_citation_key(&query, filter),
            QueryKind::Bibliographic(query) => self.search_bibliographic(&query, filter),
        }
    }

    fn search_citation_key(
        &self,
        query: &CitationKeyQuery,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<WorkRecord>> {
        let mut records = self
            .crossref_citation_query(query, filter)
            .unwrap_or_default();

        for arxiv_record in self.arxiv_citation_query(query, filter).unwrap_or_default() {
            merge_or_push(&mut records, arxiv_record);
        }

        records.retain(|record| citation_candidate_matches(record, query));
        if let Some(filter) = filter {
            records.retain(|record| record_matches_filter(record, filter));
        }
        records.sort_by(|left, right| {
            citation_title_starts_with(right, query)
                .cmp(&citation_title_starts_with(left, query))
                .then_with(|| {
                    citation_match_score(right, query).cmp(&citation_match_score(left, query))
                })
                .then_with(|| left.title.cmp(&right.title))
        });
        Ok(records.into_iter().take(5).collect())
    }

    fn search_bibliographic(
        &self,
        query: &str,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<WorkRecord>> {
        let mut records = self.crossref_query(query, filter).unwrap_or_default();

        if records.len() < 3 {
            for arxiv_record in self.arxiv_query(query, filter).unwrap_or_default() {
                merge_or_push(&mut records, arxiv_record);
            }
        }

        Ok(filtered_records(records, filter)
            .into_iter()
            .take(5)
            .collect())
    }

    fn crossref_doi(&self, doi: &str) -> Result<WorkRecord> {
        let url = format!("{}/{}", CROSSREF_BASE, urlencoding::encode(doi));
        let response: CrossrefWorkResponse =
            self.http.get(url).send()?.error_for_status()?.json()?;
        Ok(crossref_item_to_record(response.message))
    }

    fn crossref_query(
        &self,
        query: &str,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<WorkRecord>> {
        let mut url = format!(
            "{}?query.bibliographic={}&rows={}",
            CROSSREF_BASE,
            urlencoding::encode(query),
            if filter.is_some() { 25 } else { 5 }
        );
        append_crossref_filter(&mut url, filter);
        let response: CrossrefSearchResponse =
            self.http.get(url).send()?.error_for_status()?.json()?;
        Ok(response
            .message
            .items
            .into_iter()
            .map(crossref_item_to_record)
            .collect())
    }

    fn crossref_citation_query(
        &self,
        query: &CitationKeyQuery,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<WorkRecord>> {
        let mut url = format!(
            "{}?query.author={}&query.title={}&rows=100",
            CROSSREF_BASE,
            urlencoding::encode(&query.author),
            urlencoding::encode(&query.title_word)
        );
        if let Some(year) = query.publication_year {
            url.push_str(&format!(
                "&filter=from-pub-date:{year}-01-01,until-pub-date:{year}-12-31"
            ));
        }
        append_crossref_filter(&mut url, filter);

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

    fn arxiv_query(&self, query: &str, filter: Option<&SearchFilter>) -> Result<Vec<WorkRecord>> {
        let search_query = arxiv_filter_query(format!("all:{query}"), filter);
        let url = format!(
            "{}?search_query={}&start=0&max_results={}",
            ARXIV_BASE,
            urlencoding::encode(&search_query),
            if filter.is_some() { 25 } else { 5 }
        );
        let text = self.http.get(url).send()?.error_for_status()?.text()?;
        let feed: ArxivFeed = from_str(&text).context("parsing arXiv Atom response")?;
        Ok(feed
            .entries
            .into_iter()
            .map(arxiv_entry_to_record)
            .collect())
    }

    fn arxiv_citation_query(
        &self,
        query: &CitationKeyQuery,
        filter: Option<&SearchFilter>,
    ) -> Result<Vec<WorkRecord>> {
        let search_query = arxiv_filter_query(
            format!("au:{} AND ti:{}", query.author, query.title_word),
            filter,
        );
        let url = format!(
            "{}?search_query={}&start=0&max_results=25",
            ARXIV_BASE,
            urlencoding::encode(&search_query)
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

fn append_crossref_filter(url: &mut String, filter: Option<&SearchFilter>) {
    let Some(filter) = filter.filter(|filter| !filter.keyword.trim().is_empty()) else {
        return;
    };
    let keyword = urlencoding::encode(filter.keyword.trim());
    match filter.field {
        FilterField::General => url.push_str(&format!("&query.bibliographic={keyword}")),
        FilterField::Author => url.push_str(&format!("&query.author={keyword}")),
        FilterField::Journal => url.push_str(&format!("&query.container-title={keyword}")),
        FilterField::Title => url.push_str(&format!("&query.title={keyword}")),
        FilterField::Year => {
            if let Ok(year) = filter.keyword.trim().parse::<i32>() {
                if !url.contains("&filter=") {
                    url.push_str(&format!(
                        "&filter=from-pub-date:{year}-01-01,until-pub-date:{year}-12-31"
                    ));
                }
            }
        }
    }
}

fn arxiv_filter_query(base: String, filter: Option<&SearchFilter>) -> String {
    let Some(filter) = filter.filter(|filter| !filter.keyword.trim().is_empty()) else {
        return base;
    };
    let keyword = filter.keyword.trim();
    match filter.field {
        FilterField::General => format!("{base} AND all:{keyword}"),
        FilterField::Author => format!("{base} AND au:{keyword}"),
        FilterField::Title => format!("{base} AND ti:{keyword}"),
        FilterField::Journal | FilterField::Year => base,
    }
}

fn filtered_records(records: Vec<WorkRecord>, filter: Option<&SearchFilter>) -> Vec<WorkRecord> {
    let Some(filter) = filter.filter(|filter| !filter.keyword.trim().is_empty()) else {
        return records;
    };
    records
        .into_iter()
        .filter(|record| record_matches_filter(record, filter))
        .collect()
}

fn record_matches_filter(record: &WorkRecord, filter: &SearchFilter) -> bool {
    let keyword = filter.keyword.trim().to_lowercase();
    if keyword.is_empty() {
        return true;
    }
    let contains = |value: &str| value.to_lowercase().contains(&keyword);

    match filter.field {
        FilterField::General => {
            contains(&record.title)
                || record
                    .authors
                    .iter()
                    .any(|author| contains(&author.display_name()))
                || record.container_title.as_deref().is_some_and(contains)
                || record.publisher.as_deref().is_some_and(contains)
                || record.year.is_some_and(|year| contains(&year.to_string()))
                || record.doi.as_deref().is_some_and(contains)
                || record.arxiv_id.as_deref().is_some_and(contains)
        }
        FilterField::Author => record
            .authors
            .iter()
            .any(|author| contains(&author.display_name())),
        FilterField::Journal => record.container_title.as_deref().is_some_and(contains),
        FilterField::Year => record.year.is_some_and(|year| contains(&year.to_string())),
        FilterField::Title => contains(&record.title),
    }
}

fn citation_candidate_matches(record: &WorkRecord, query: &CitationKeyQuery) -> bool {
    let author_matches = record.authors.first().is_some_and(|author| {
        normalize_key_part(&author.family) == query.author.to_ascii_lowercase()
    });
    let year_matches = query
        .publication_year
        .is_none_or(|year| record.year == Some(year));
    let title_matches =
        title_words(&record.title).any(|word| word == query.title_word.to_ascii_lowercase());

    author_matches && year_matches && title_matches
}

fn citation_match_score(record: &WorkRecord, query: &CitationKeyQuery) -> u8 {
    let mut score = 0;

    if record.authors.first().is_some_and(|author| {
        normalize_key_part(&author.family) == query.author.to_ascii_lowercase()
    }) {
        score += 4;
    }
    if query
        .publication_year
        .is_some_and(|year| record.year == Some(year))
    {
        score += 3;
    }
    if title_words(&record.title).any(|word| word == query.title_word.to_ascii_lowercase()) {
        score += 3;
    }
    if citation_title_starts_with(record, query) {
        score += 2;
    }

    score
}

fn citation_title_starts_with(record: &WorkRecord, query: &CitationKeyQuery) -> bool {
    first_title_word(&record.title).as_deref() == Some(&query.title_word.to_ascii_lowercase())
}

fn first_title_word(title: &str) -> Option<String> {
    title_words(title).find(|word| !matches!(word.as_str(), "a" | "an" | "the"))
}

fn title_words(title: &str) -> impl Iterator<Item = String> + '_ {
    title
        .split(|ch: char| !ch.is_alphanumeric())
        .map(normalize_key_part)
        .filter(|word| !word.is_empty())
}

fn normalize_key_part(input: &str) -> String {
    input
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
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
            QueryKind::CitationKey(CitationKeyQuery {
                author: "blakie".to_string(),
                year_fragment: "2023".to_string(),
                title_word: "compressibility".to_string(),
                publication_year: Some(2023),
            })
        );
        assert_eq!(
            classify_query("Gallemi2025Excitation"),
            QueryKind::CitationKey(CitationKeyQuery {
                author: "Gallemi".to_string(),
                year_fragment: "2025".to_string(),
                title_word: "Excitation".to_string(),
                publication_year: Some(2025),
            })
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
                citation_query("tanzi", "2019", "supersolid", Some(2019)),
                citation_query("natale", "2019", "excitation", Some(2019)),
                citation_query("scheiermann", "2025", "excitation", Some(2025)),
                citation_query("lin", "2412", "ai", None),
                citation_query("vsindik", "2024", "sound", Some(2024)),
                citation_query("sanchez", "2023", "heating", Some(2023)),
                citation_query("staub", "2024", "new", Some(2024)),
                citation_query("biss", "2023", "probing", Some(2023)),
            ]
        );
    }

    fn citation_query(
        author: &str,
        year_fragment: &str,
        title_word: &str,
        publication_year: Option<i32>,
    ) -> QueryKind {
        QueryKind::CitationKey(CitationKeyQuery {
            author: author.to_string(),
            year_fragment: year_fragment.to_string(),
            title_word: title_word.to_string(),
            publication_year,
        })
    }

    #[test]
    fn ranks_citation_key_matches_by_author_year_and_title_word() {
        let query = CitationKeyQuery {
            author: "ma".to_string(),
            year_fragment: "2023".to_string(),
            title_word: "high".to_string(),
            publication_year: Some(2023),
        };
        let exact = WorkRecord {
            title:
                "High-fidelity gates with mid-circuit erasure conversion in a metastable neutral atom qubit"
                    .to_string(),
            authors: vec![Author::new(Some("Shuo".to_string()), "Ma")],
            year: Some(2023),
            container_title: Some("Nature".to_string()),
            volume: Some("622".to_string()),
            number: None,
            pages: None,
            publisher: None,
            doi: Some("10.1038/s41586-023-06438-1".to_string()),
            arxiv_id: None,
            source: SourceKind::Crossref,
            entry_type: "article".to_string(),
        };
        let unrelated = WorkRecord {
            title: "High-energy physics".to_string(),
            authors: vec![Author::new(Some("Ada".to_string()), "Lovelace")],
            year: Some(2022),
            container_title: None,
            volume: None,
            number: None,
            pages: None,
            publisher: None,
            doi: None,
            arxiv_id: None,
            source: SourceKind::Crossref,
            entry_type: "article".to_string(),
        };

        assert!(citation_match_score(&exact, &query) > citation_match_score(&unrelated, &query));
        assert!(citation_title_starts_with(&exact, &query));
        assert!(record_matches_filter(
            &exact,
            &SearchFilter::new(FilterField::Journal, "nature")
        ));
        assert!(record_matches_filter(
            &exact,
            &SearchFilter::new(FilterField::Author, "shuo ma")
        ));
        assert!(record_matches_filter(
            &exact,
            &SearchFilter::new(FilterField::Year, "2023")
        ));
        assert!(record_matches_filter(
            &exact,
            &SearchFilter::new(FilterField::Title, "mid-circuit")
        ));
        assert!(!record_matches_filter(
            &exact,
            &SearchFilter::new(FilterField::Journal, "science")
        ));

        let contains_later = WorkRecord {
            title: "A model with high accuracy".to_string(),
            authors: vec![Author::new(Some("Shuo".to_string()), "Ma")],
            year: Some(2023),
            source: SourceKind::Arxiv,
            ..unrelated
        };
        assert!(!citation_title_starts_with(&contains_later, &query));
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
