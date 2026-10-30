use crate::model::*;
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

pub fn normalize(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}
pub fn pin_key(library: &str, uuid: &str) -> String {
    format!("{library}::{uuid}")
}
pub fn model_universe(book: &Book) -> bool {
    book.hierarchy.iter().any(|v| {
        matches!(
            normalize(v.name.as_deref().unwrap_or("")).as_str(),
            "greaterfoundationuniverse" | "greatereternalchampionsaga"
        )
    })
}
pub fn validate_reference(reference: &Reference) -> Result<()> {
    if reference.title.trim().is_empty() {
        bail!("A reference needs a work title");
    }
    if reference.approved && reference.evidence.trim().is_empty() {
        bail!("An approved rule needs evidence or an explicit user decision");
    }
    for (slot, value) in reference.hierarchy.iter().enumerate() {
        if let Some(name) = &value.name {
            if name.trim().is_empty() || name.trim() != name {
                bail!("Trim series names");
            }
            if reference.approved && value.index.is_none() {
                bail!("Named series need a number");
            }
        } else if slot > 0 && value.index.is_some() {
            bail!("Empty custom series must not have an index");
        }
        if value.index.is_some_and(|v| !v.is_finite()) {
            bail!("Numbers must be finite");
        }
    }
    if reference.approved
        && reference.authors.is_empty()
        && reference.identifiers.is_empty()
        && reference.book_uuids.is_empty()
    {
        bail!("Supply an author, identifier, or explicitly bound library book");
    }
    Ok(())
}

fn isbn(value: &str) -> Option<String> {
    let mut digits: String = value
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X')
        .collect();
    if digits.len() == 10 {
        let sum: u32 = digits
            .chars()
            .enumerate()
            .map(|(i, c)| {
                (10 - i as u32)
                    * if c == 'X' {
                        10
                    } else {
                        c.to_digit(10).unwrap_or(99)
                    }
            })
            .sum();
        if !sum.is_multiple_of(11) {
            return None;
        }
        digits = format!("978{}", &digits[..9]);
        let sum: u32 = digits
            .chars()
            .enumerate()
            .map(|(i, c)| c.to_digit(10).unwrap_or(0) * if i % 2 == 0 { 1 } else { 3 })
            .sum();
        digits.push(char::from_digit((10 - sum % 10) % 10, 10)?);
    }
    if digits.len() != 13 {
        return None;
    }
    let sum: u32 = digits
        .chars()
        .enumerate()
        .map(|(i, c)| c.to_digit(10).unwrap_or(99) * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    sum.is_multiple_of(10).then_some(digits)
}
fn strong_identifier(book: &Book, reference: &Reference) -> bool {
    for (kind, value) in &reference.identifiers {
        if kind.starts_with("isbn") {
            if let Some(wanted) = isbn(value) {
                if book
                    .identifiers
                    .iter()
                    .any(|(k, v)| k.starts_with("isbn") && isbn(v).as_ref() == Some(&wanted))
                {
                    return true;
                }
            }
        } else if matches!(
            kind.as_str(),
            "asin" | "amazon" | "google" | "goodreads" | "isfdb-title"
        ) && book
            .identifiers
            .get(kind)
            .is_some_and(|v| !v.is_empty() && v == value)
        {
            return true;
        }
    }
    false
}
fn score(book: &Book, reference: &Reference) -> (u16, bool) {
    let title = normalize(&book.title);
    let title_matches = normalize(&reference.title) == title
        || reference
            .aliases
            .iter()
            .any(|alias| normalize(alias) == title);
    let identifier = strong_identifier(book, reference);
    if reference.book_uuids.contains(&book.uuid) {
        let author_matches = reference.authors.is_empty()
            || reference.authors.iter().any(|a| {
                book.authors
                    .iter()
                    .any(|actual| normalize(actual) == normalize(a))
            });
        return (300, (!title_matches || !author_matches) && !identifier);
    }
    if identifier {
        return (200, false);
    }
    if title_matches
        && reference.authors.iter().any(|author| {
            book.authors
                .iter()
                .any(|actual| normalize(actual) == normalize(author))
        })
    {
        return (100, false);
    }
    (0, false)
}

fn work_key(reference: &Reference) -> String {
    format!(
        "{}::{}::{}",
        normalize(&reference.title),
        normalize(reference.authors.first().map(String::as_str).unwrap_or("")),
        reference.edition
    )
}

/// Order whole works, coalescing duplicate-edition rules. Never invent a tie-break.
pub fn chronology(references: &[Reference], universe: &str) -> Result<Vec<Vec<String>>> {
    let selected: Vec<_> = references
        .iter()
        .filter(|r| r.hierarchy[0].name.as_deref() == Some(universe))
        .collect();
    let mut groups: BTreeMap<String, Vec<&Reference>> = BTreeMap::new();
    let mut id_group = BTreeMap::new();
    for r in selected {
        let key = work_key(r);
        id_group.insert(r.id.clone(), key.clone());
        groups.entry(key).or_default().push(r);
    }
    let mut edges: BTreeMap<String, BTreeSet<String>> = groups
        .keys()
        .map(|key| (key.clone(), BTreeSet::new()))
        .collect();
    let mut anchors = Vec::new();
    for (key, members) in &groups {
        let fixed: Vec<f64> = members
            .iter()
            .filter(|r| r.fixed)
            .filter_map(|r| r.hierarchy[0].index)
            .collect();
        if fixed.windows(2).any(|pair| pair[0] != pair[1]) {
            bail!("Conflicting fixed positions for {}", members[0].title);
        }
        if let Some(position) = fixed.first() {
            if !position.is_finite() {
                bail!("Invalid chronological position");
            }
            anchors.push((*position, key.clone()));
        }
        for r in members {
            for target in &r.before {
                let target_key = id_group.get(target).ok_or_else(|| {
                    anyhow::anyhow!("Before relation must target a work in the same universe")
                })?;
                if key == target_key {
                    bail!("A work cannot precede itself");
                }
                edges
                    .get_mut(key)
                    .expect("known work")
                    .insert(target_key.clone());
            }
        }
    }
    anchors.sort_by(|a, b| a.0.total_cmp(&b.0));
    for pair in anchors.windows(2) {
        if pair[0].0 == pair[1].0 {
            bail!("Different works have the same fixed position");
        }
        edges
            .get_mut(&pair[0].1)
            .expect("known anchor")
            .insert(pair[1].1.clone());
    }
    let mut indegrees: BTreeMap<String, usize> = groups.keys().map(|k| (k.clone(), 0)).collect();
    for targets in edges.values() {
        for target in targets {
            *indegrees.get_mut(target).expect("known target") += 1;
        }
    }
    let mut result = Vec::new();
    while !indegrees.is_empty() {
        let ready: Vec<String> = indegrees
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(key, _)| key.clone())
            .collect();
        if ready.is_empty() {
            bail!("Chronology constraints contain a cycle or contradict fixed anchors");
        }
        if ready.len() != 1 {
            let titles: Vec<_> = ready.iter().map(|k| groups[k][0].title.as_str()).collect();
            bail!(
                "Order is incomplete: add a before relation or fixed position between {}",
                titles.join(" / ")
            );
        }
        let key = &ready[0];
        result.push(groups[key].iter().map(|r| r.id.clone()).collect());
        indegrees.remove(key);
        for target in &edges[key] {
            if let Some(degree) = indegrees.get_mut(target) {
                *degree -= 1;
            }
        }
    }
    Ok(result)
}

pub fn issues(references: &[Reference]) -> BTreeMap<String, String> {
    let approved: Vec<_> = references.iter().filter(|r| r.approved).cloned().collect();
    let universes: BTreeSet<_> = approved
        .iter()
        .filter_map(|r| r.hierarchy[0].name.clone())
        .collect();
    let mut issues = BTreeMap::new();
    for universe in universes {
        if let Err(error) = chronology(&approved, &universe) {
            for r in &approved {
                if r.hierarchy[0].name.as_deref() == Some(&universe) {
                    issues.insert(r.id.clone(), error.to_string());
                }
            }
        }
    }
    for r in &approved {
        for target in &r.before {
            if let Some(other) = approved.iter().find(|other| &other.id == target) {
                if let (Some(a), Some(b)) = (r.hierarchy[0].index, other.hierarchy[0].index) {
                    if a >= b {
                        issues.insert(
                            r.id.clone(),
                            "Numeric positions contradict an explicit before relation".into(),
                        );
                    }
                }
            }
        }
    }
    issues
}

pub fn propose(
    book: &Book,
    data: &AppData,
    library: &str,
    issues: &BTreeMap<String, String>,
) -> Proposal {
    let pinned = data
        .pins
        .get(&pin_key(library, &book.uuid))
        .copied()
        .unwrap_or([false; 4]);
    let mut result = Proposal {
        book: book.clone(),
        status: "unresolved".into(),
        reason: "No approved identity/chronology reference; populated metadata is not proof."
            .into(),
        reference_id: None,
        after: None,
        changed: Vec::new(),
        blocked: true,
        pinned,
    };
    let candidates: Vec<_> = data
        .references
        .iter()
        .filter(|r| r.approved)
        .map(|r| (score(book, r), r))
        .filter(|((score, _), _)| *score > 0)
        .collect();
    let best = candidates
        .iter()
        .map(|((score, _), _)| *score)
        .max()
        .unwrap_or(0);
    let best: Vec<_> = candidates
        .iter()
        .filter(|((score, _), _)| *score == best)
        .collect();
    if best.is_empty() {
        if pinned.iter().any(|v| *v) {
            result.status = "protected".into();
            result.reason =
                "User-protected numbering; not independently certified by pinning.".into();
        } else if let Some(alias) = data.aliases.iter().find(|alias| {
            book.hierarchy.iter().any(|v| {
                v.name
                    .as_ref()
                    .is_some_and(|name| normalize(name) == normalize(&alias.existing_series))
            })
        }) {
            result.reason = format!("Known candidate universe: {}. An approved chronological position is still required.", alias.universe);
        }
        return result;
    }
    let reference = best[0].1;
    if best.iter().any(|(_, r)| r.hierarchy != reference.hierarchy) {
        result.reason = "Conflicting approved references match this identity; review them.".into();
        return result;
    }
    if best[0].0 .1 {
        result.reason =
            "Previously bound UUID now has different identity metadata; review the reference."
                .into();
        return result;
    }
    result.reference_id = Some(reference.id.clone());
    if let Err(error) = validate_reference(reference) {
        result.reason = error.to_string();
        return result;
    }
    if let Some(error) = issues.get(&reference.id) {
        result.reason = error.clone();
        return result;
    }
    let title = book.title.to_lowercase();
    if reference.edition == "novel"
        && [
            "omnibus",
            "boxed set",
            "box set",
            "collected stories",
            "collection",
        ]
        .iter()
        .any(|v| title.contains(v))
    {
        result.reason =
            "Edition scope may be a collection rather than the referenced novel.".into();
        return result;
    }
    let mut target = reference.hierarchy.clone();
    if target[0].name.is_none() {
        target[0].index = Some(1.0);
    }
    result.changed = (0..4)
        .filter(|slot| book.hierarchy[*slot] != target[*slot])
        .collect();
    result.after = Some(target);
    result.blocked = !result.changed.is_empty() && result.changed.iter().all(|slot| pinned[*slot]);
    result.status = if result.changed.is_empty() {
        "correct"
    } else if result.changed.iter().any(|slot| pinned[*slot]) {
        "protected"
    } else if result
        .changed
        .iter()
        .any(|slot| book.hierarchy[*slot].name.is_none())
    {
        "missing"
    } else {
        "conflict"
    }
    .into();
    result.reason = format!(
        "{} match; {}. {}{}",
        match best[0].0 .0 {
            300 => "Bound UUID",
            200 => "Identifier",
            _ => "Approved title + author/alias",
        },
        reference.origin,
        reference.evidence,
        if reference.coverage == "partial" {
            " [Partial universe coverage; no unknown positions inferred.]"
        } else {
            ""
        }
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_accents_without_dropping_words() {
        assert_eq!(normalize("Melniboné"), normalize("MELNIBONE"));
        assert_ne!(normalize("The Book"), normalize("Book"));
    }
    #[test]
    fn validates_isbn_and_ignores_revision_numbers() {
        assert_eq!(isbn("0-553-29612-4"), isbn("9780553296129"));
        assert_eq!(isbn("1.1"), None);
    }
    #[test]
    fn detects_cycles_and_incomplete_order() {
        let mut a = Reference {
            title: "A".into(),
            fixed: false,
            ..Default::default()
        };
        let mut b = Reference {
            title: "B".into(),
            fixed: false,
            ..Default::default()
        };
        a.hierarchy[0].name = Some("Universe".into());
        b.hierarchy[0].name = Some("Universe".into());
        assert!(chronology(&[a.clone(), b.clone()], "Universe").is_err());
        a.before.push(b.id.clone());
        assert!(chronology(&[a.clone(), b.clone()], "Universe").is_ok());
        b.before.push(a.id.clone());
        assert!(chronology(&[a, b], "Universe").is_err());
    }
}
