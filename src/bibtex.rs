use crate::metadata::{Author, WorkRecord};

pub fn format_bibtex(record: &WorkRecord) -> String {
    let key = citation_key(record);
    let mut fields = Vec::new();

    push_field(&mut fields, "title", Some(escape_latex(&record.title)));
    if !record.authors.is_empty() {
        let authors = record
            .authors
            .iter()
            .map(format_author)
            .collect::<Vec<_>>()
            .join(" and ");
        push_field(&mut fields, "author", Some(authors));
    }
    if let Some(container) = &record.container_title {
        push_field(&mut fields, "journal", Some(escape_latex(container)));
    }
    push_field(
        &mut fields,
        "volume",
        record.volume.as_ref().map(|value| escape_latex(value)),
    );
    push_field(
        &mut fields,
        "number",
        record.number.as_ref().map(|value| escape_latex(value)),
    );
    push_field(
        &mut fields,
        "pages",
        record.pages.as_ref().map(|value| escape_latex(value)),
    );
    if let Some(year) = record.year {
        push_field(&mut fields, "year", Some(year.to_string()));
    }
    push_field(
        &mut fields,
        "publisher",
        record.publisher.as_ref().map(|value| escape_latex(value)),
    );
    push_field(
        &mut fields,
        "doi",
        record.doi.as_ref().map(|value| value.trim().to_string()),
    );
    push_field(
        &mut fields,
        "eprint",
        record
            .arxiv_id
            .as_ref()
            .map(|value| value.trim().to_string()),
    );

    let body = fields
        .into_iter()
        .map(|(name, value)| format!("  {}={{{}}}", name, value))
        .collect::<Vec<_>>()
        .join(",\n");

    format!("@{}{{{},\n{}\n}}", record.entry_type, key, body)
}

fn push_field(fields: &mut Vec<(&'static str, String)>, name: &'static str, value: Option<String>) {
    if let Some(value) = value {
        let value = value.trim();
        if !value.is_empty() {
            fields.push((name, value.to_string()));
        }
    }
}

fn citation_key(record: &WorkRecord) -> String {
    let first_author = record
        .authors
        .first()
        .map(|author| citation_family_key(&author.family))
        .filter(|part| !part.is_empty())
        .unwrap_or_else(|| "ref".to_string());
    let year = record
        .year
        .map(|year| year.to_string())
        .unwrap_or_else(|| "nodate".to_string());
    let title_word = title_key_word(&record.title).unwrap_or_else(|| "work".to_string());

    format!(
        "{}{}{}",
        first_author.to_lowercase(),
        year,
        title_word.to_lowercase()
    )
}

fn citation_family_key(family: &str) -> String {
    family
        .split_whitespace()
        .next_back()
        .map(ascii_key_part)
        .unwrap_or_default()
}

fn title_key_word(title: &str) -> Option<String> {
    title
        .split(|ch: char| !ch.is_alphanumeric())
        .map(ascii_key_part)
        .find(|word| !word.is_empty() && !matches!(word.as_str(), "a" | "an" | "the"))
}

fn ascii_key_part(input: &str) -> String {
    input
        .chars()
        .flat_map(|ch| ch.to_lowercase())
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn format_author(author: &Author) -> String {
    let family = escape_latex(&author.family);
    match &author.given {
        Some(given) if !given.trim().is_empty() => format!("{}, {}", family, escape_latex(given)),
        _ => family,
    }
}

fn escape_latex(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '$' => out.push_str("\\$"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '~' => out.push_str("\\~{}"),
            '^' => out.push_str("\\^{}"),
            'à' => out.push_str("\\`{a}"),
            'á' => out.push_str("\\'{a}"),
            'â' => out.push_str("\\^{a}"),
            'ä' => out.push_str("\\\"{a}"),
            'ã' => out.push_str("\\~{a}"),
            'å' => out.push_str("\\r{a}"),
            'æ' => out.push_str("\\ae{}"),
            'ç' => out.push_str("\\c{c}"),
            'è' => out.push_str("\\`{e}"),
            'é' => out.push_str("\\'{e}"),
            'ê' => out.push_str("\\^{e}"),
            'ë' => out.push_str("\\\"{e}"),
            'ì' => out.push_str("\\`{i}"),
            'í' => out.push_str("\\'{i}"),
            'î' => out.push_str("\\^{i}"),
            'ï' => out.push_str("\\\"{i}"),
            'ñ' => out.push_str("\\~{n}"),
            'ò' => out.push_str("\\`{o}"),
            'ó' => out.push_str("\\'{o}"),
            'ô' => out.push_str("\\^{o}"),
            'ö' => out.push_str("\\\"{o}"),
            'õ' => out.push_str("\\~{o}"),
            'ø' => out.push_str("\\o{}"),
            'ù' => out.push_str("\\`{u}"),
            'ú' => out.push_str("\\'{u}"),
            'û' => out.push_str("\\^{u}"),
            'ü' => out.push_str("\\\"{u}"),
            'ý' => out.push_str("\\'{y}"),
            'ÿ' => out.push_str("\\\"{y}"),
            'À' => out.push_str("\\`{A}"),
            'Á' => out.push_str("\\'{A}"),
            'Â' => out.push_str("\\^{A}"),
            'Ä' => out.push_str("\\\"{A}"),
            'Ã' => out.push_str("\\~{A}"),
            'Å' => out.push_str("\\r{A}"),
            'Æ' => out.push_str("\\AE{}"),
            'Ç' => out.push_str("\\c{C}"),
            'È' => out.push_str("\\`{E}"),
            'É' => out.push_str("\\'{E}"),
            'Ê' => out.push_str("\\^{E}"),
            'Ë' => out.push_str("\\\"{E}"),
            'Ì' => out.push_str("\\`{I}"),
            'Í' => out.push_str("\\'{I}"),
            'Î' => out.push_str("\\^{I}"),
            'Ï' => out.push_str("\\\"{I}"),
            'Ñ' => out.push_str("\\~{N}"),
            'Ò' => out.push_str("\\`{O}"),
            'Ó' => out.push_str("\\'{O}"),
            'Ô' => out.push_str("\\^{O}"),
            'Ö' => out.push_str("\\\"{O}"),
            'Õ' => out.push_str("\\~{O}"),
            'Ø' => out.push_str("\\O{}"),
            'Ù' => out.push_str("\\`{U}"),
            'Ú' => out.push_str("\\'{U}"),
            'Û' => out.push_str("\\^{U}"),
            'Ü' => out.push_str("\\\"{U}"),
            'Ý' => out.push_str("\\'{Y}"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{SourceKind, WorkRecord};

    #[test]
    fn includes_doi_and_latex_accents() {
        let record = WorkRecord {
            title: "Café & résumé".to_string(),
            authors: vec![Author::new(Some("René".to_string()), "García")],
            year: Some(2024),
            container_title: Some("Journal".to_string()),
            volume: Some("1".to_string()),
            number: None,
            pages: Some("1-2".to_string()),
            publisher: None,
            doi: Some("10.1234/example".to_string()),
            arxiv_id: None,
            source: SourceKind::Crossref,
            entry_type: "article".to_string(),
        };

        let bib = format_bibtex(&record);

        assert!(bib.starts_with("@article{garca2024caf"));
        assert!(bib.contains("title={Caf\\'{e} \\& r\\'{e}sum\\'{e}}"));
        assert!(bib.contains("author={Garc\\'{i}a, Ren\\'{e}}"));
        assert!(bib.contains("doi={10.1234/example}"));
    }

    #[test]
    fn citation_key_splits_hyphenated_title_words() {
        let record = WorkRecord {
            title: "High-fidelity gates with mid-circuit erasure conversion in an atomic qubit"
                .to_string(),
            authors: vec![Author::new(Some("Shuo".to_string()), "Ma")],
            year: Some(2023),
            container_title: Some("Nature".to_string()),
            volume: Some("622".to_string()),
            number: None,
            pages: None,
            publisher: Some("Springer Science and Business Media LLC".to_string()),
            doi: Some("10.1038/s41586-023-06438-1".to_string()),
            arxiv_id: None,
            source: SourceKind::Crossref,
            entry_type: "article".to_string(),
        };

        assert!(format_bibtex(&record).starts_with("@article{ma2023high,"));
    }

    #[test]
    fn citation_key_skips_leading_articles() {
        let record = WorkRecord {
            title: "A new experiment".to_string(),
            authors: vec![Author::new(Some("Etienne".to_string()), "Staub")],
            year: Some(2024),
            container_title: None,
            volume: None,
            number: None,
            pages: None,
            publisher: None,
            doi: None,
            arxiv_id: None,
            source: SourceKind::Crossref,
            entry_type: "phdthesis".to_string(),
        };

        assert!(format_bibtex(&record).starts_with("@phdthesis{staub2024new,"));
    }

    #[test]
    fn citation_key_uses_terminal_component_of_compound_family_name() {
        let record = WorkRecord {
            title: "Ground-state properties of dipolar Bose polarons".to_string(),
            authors: vec![Author::new(Some("L. A.".to_string()), "Peña Ardila")],
            year: Some(2019),
            container_title: Some(
                "Journal of Physics B: Atomic, Molecular and Optical Physics".to_string(),
            ),
            volume: Some("52".to_string()),
            number: None,
            pages: None,
            publisher: None,
            doi: Some("10.1088/1361-6455/aaf35e".to_string()),
            arxiv_id: None,
            source: SourceKind::Crossref,
            entry_type: "article".to_string(),
        };

        assert!(format_bibtex(&record).starts_with("@article{ardila2019ground,"));
    }
}
