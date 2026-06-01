#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub given: Option<String>,
    pub family: String,
}

impl Author {
    pub fn new(given: impl Into<Option<String>>, family: impl Into<String>) -> Self {
        Self {
            given: given.into(),
            family: family.into(),
        }
    }

    pub fn display_name(&self) -> String {
        match &self.given {
            Some(given) if !given.is_empty() => format!("{} {}", given, self.family),
            _ => self.family.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Crossref,
    Arxiv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkRecord {
    pub title: String,
    pub authors: Vec<Author>,
    pub year: Option<i32>,
    pub container_title: Option<String>,
    pub volume: Option<String>,
    pub number: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub source: SourceKind,
    pub entry_type: String,
}

impl WorkRecord {
    pub fn external_url(&self) -> Option<String> {
        if let Some(doi) = &self.doi {
            return Some(format!("https://doi.org/{}", doi));
        }

        self.arxiv_id
            .as_ref()
            .map(|id| format!("https://arxiv.org/abs/{}", id))
    }

    pub fn author_summary(&self) -> String {
        match self.authors.as_slice() {
            [] => "Unknown authors".to_string(),
            [one] => one.display_name(),
            [first, second] => format!("{} and {}", first.display_name(), second.display_name()),
            [first, ..] => format!("{} et al.", first.display_name()),
        }
    }

    pub fn merge_missing_from(&mut self, other: &WorkRecord) {
        if self.doi.is_none() {
            self.doi = other.doi.clone();
        }
        if self.arxiv_id.is_none() {
            self.arxiv_id = other.arxiv_id.clone();
        }
        if self.container_title.is_none() {
            self.container_title = other.container_title.clone();
        }
        if self.year.is_none() {
            self.year = other.year;
        }
    }
}
