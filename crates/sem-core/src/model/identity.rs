use std::collections::{HashMap, HashSet};

use super::change::{ChangeType, SemanticChange};
use super::entity::SemanticEntity;

pub struct MatchResult {
    pub changes: Vec<SemanticChange>,
}

fn build_change(
    id: String,
    entity_id: String,
    change_type: ChangeType,
    entity_type: String,
    entity_name: String,
    file_path: String,
    old_file_path: Option<String>,
    before_entity: Option<&SemanticEntity>,
    after_entity: Option<&SemanticEntity>,
    commit_sha: Option<&str>,
    author: Option<&str>,
    structural_change: Option<bool>,
) -> SemanticChange {
    SemanticChange {
        id,
        entity_id,
        change_type,
        entity_type,
        entity_name,
        file_path,
        old_file_path,
        before_content: before_entity.map(|entity| entity.content.clone()),
        after_content: after_entity.map(|entity| entity.content.clone()),
        commit_sha: commit_sha.map(String::from),
        author: author.map(String::from),
        timestamp: None,
        structural_change,
        before_start_line: before_entity.map(|entity| entity.start_line),
        before_end_line: before_entity.map(|entity| entity.end_line),
        after_start_line: after_entity.map(|entity| entity.start_line),
        after_end_line: after_entity.map(|entity| entity.end_line),
    }
}

/// 3-phase entity matching algorithm:
/// 1. Exact ID match — same entity ID in before/after → modified or unchanged
/// 2. Content hash match — same hash, different ID → renamed or moved
/// 3. Fuzzy similarity — >80% content similarity → probable rename
pub fn match_entities(
    before: &[SemanticEntity],
    after: &[SemanticEntity],
    _file_path: &str,
    similarity_fn: Option<&dyn Fn(&SemanticEntity, &SemanticEntity) -> f64>,
    commit_sha: Option<&str>,
    author: Option<&str>,
) -> MatchResult {
    let mut changes: Vec<SemanticChange> = Vec::new();
    let mut matched_before: HashSet<&str> = HashSet::new();
    let mut matched_after: HashSet<&str> = HashSet::new();

    let before_by_id: HashMap<&str, &SemanticEntity> =
        before.iter().map(|e| (e.id.as_str(), e)).collect();
    let after_by_id: HashMap<&str, &SemanticEntity> =
        after.iter().map(|e| (e.id.as_str(), e)).collect();

    // Phase 1: Exact ID match
    for (&id, after_entity) in &after_by_id {
        if let Some(before_entity) = before_by_id.get(id) {
            matched_before.insert(id);
            matched_after.insert(id);

            if before_entity.content_hash != after_entity.content_hash {
                let structural_change = match (&before_entity.structural_hash, &after_entity.structural_hash) {
                    (Some(before_sh), Some(after_sh)) => Some(before_sh != after_sh),
                    _ => None,
                };
                changes.push(build_change(
                    format!("change::{id}"),
                    id.to_string(),
                    ChangeType::Modified,
                    after_entity.entity_type.clone(),
                    after_entity.name.clone(),
                    after_entity.file_path.clone(),
                    None,
                    Some(before_entity),
                    Some(after_entity),
                    commit_sha,
                    author,
                    structural_change,
                ));
            }
        }
    }

    // Collect unmatched
    let unmatched_before: Vec<&SemanticEntity> = before
        .iter()
        .filter(|e| !matched_before.contains(e.id.as_str()))
        .collect();
    let unmatched_after: Vec<&SemanticEntity> = after
        .iter()
        .filter(|e| !matched_after.contains(e.id.as_str()))
        .collect();

    // Phase 2: Content hash match (rename/move detection)
    let mut before_by_hash: HashMap<&str, Vec<&SemanticEntity>> = HashMap::new();
    let mut before_by_structural: HashMap<&str, Vec<&SemanticEntity>> = HashMap::new();
    for entity in &unmatched_before {
        before_by_hash
            .entry(entity.content_hash.as_str())
            .or_default()
            .push(entity);
        if let Some(ref sh) = entity.structural_hash {
            before_by_structural
                .entry(sh.as_str())
                .or_default()
                .push(entity);
        }
    }

    for after_entity in &unmatched_after {
        if matched_after.contains(after_entity.id.as_str()) {
            continue;
        }
        // Try exact content_hash first
        let found = before_by_hash
            .get_mut(after_entity.content_hash.as_str())
            .and_then(|c| c.pop());
        // Fall back to structural_hash (formatting/comment changes don't matter)
        let found = found.or_else(|| {
            after_entity.structural_hash.as_ref().and_then(|sh| {
                before_by_structural.get_mut(sh.as_str()).and_then(|c| {
                    c.iter()
                        .position(|e| !matched_before.contains(e.id.as_str()))
                        .map(|i| c.remove(i))
                })
            })
        });

        if let Some(before_entity) = found {
            matched_before.insert(&before_entity.id);
            matched_after.insert(&after_entity.id);

            let change_type = if before_entity.file_path != after_entity.file_path {
                ChangeType::Moved
            } else {
                ChangeType::Renamed
            };

            let old_file_path = if before_entity.file_path != after_entity.file_path {
                Some(before_entity.file_path.clone())
            } else {
                None
            };

            changes.push(build_change(
                format!("change::{}", after_entity.id),
                after_entity.id.clone(),
                change_type,
                after_entity.entity_type.clone(),
                after_entity.name.clone(),
                after_entity.file_path.clone(),
                old_file_path,
                Some(before_entity),
                Some(after_entity),
                commit_sha,
                author,
                None,
            ));
        }
    }

    // Phase 3: Fuzzy similarity (>80% threshold)
    let still_unmatched_before: Vec<&SemanticEntity> = unmatched_before
        .iter()
        .filter(|e| !matched_before.contains(e.id.as_str()))
        .copied()
        .collect();
    let still_unmatched_after: Vec<&SemanticEntity> = unmatched_after
        .iter()
        .filter(|e| !matched_after.contains(e.id.as_str()))
        .copied()
        .collect();

    if let Some(sim_fn) = similarity_fn {
        if !still_unmatched_before.is_empty() && !still_unmatched_after.is_empty() {
            const THRESHOLD: f64 = 0.8;
            // Size ratio filter: pairs with very different content lengths can't reach 0.8 Jaccard
            const SIZE_RATIO_CUTOFF: f64 = 0.5;

            // Pre-compute content lengths for O(1) size filtering
            let before_lens: Vec<usize> = still_unmatched_before
                .iter()
                .map(|e| e.content.split_whitespace().count())
                .collect();
            let after_lens: Vec<usize> = still_unmatched_after
                .iter()
                .map(|e| e.content.split_whitespace().count())
                .collect();

            for (ai, after_entity) in still_unmatched_after.iter().enumerate() {
                let mut best_match: Option<&SemanticEntity> = None;
                let mut best_score: f64 = 0.0;
                let a_len = after_lens[ai];

                for (bi, before_entity) in still_unmatched_before.iter().enumerate() {
                    if matched_before.contains(before_entity.id.as_str()) {
                        continue;
                    }
                    if before_entity.entity_type != after_entity.entity_type {
                        continue;
                    }

                    // Early exit: skip pairs where token count ratio is too different
                    let b_len = before_lens[bi];
                    let (min_l, max_l) = if a_len < b_len { (a_len, b_len) } else { (b_len, a_len) };
                    if max_l > 0 && (min_l as f64 / max_l as f64) < SIZE_RATIO_CUTOFF {
                        continue;
                    }

                    let score = sim_fn(before_entity, after_entity);
                    if score > best_score && score >= THRESHOLD {
                        best_score = score;
                        best_match = Some(before_entity);
                    }
                }

                if let Some(matched) = best_match {
                    matched_before.insert(&matched.id);
                    matched_after.insert(&after_entity.id);

                    let change_type = if matched.file_path != after_entity.file_path {
                        ChangeType::Moved
                    } else {
                        ChangeType::Renamed
                    };

                    let old_file_path = if matched.file_path != after_entity.file_path {
                        Some(matched.file_path.clone())
                    } else {
                        None
                    };

                    changes.push(build_change(
                        format!("change::{}", after_entity.id),
                        after_entity.id.clone(),
                        change_type,
                        after_entity.entity_type.clone(),
                        after_entity.name.clone(),
                        after_entity.file_path.clone(),
                        old_file_path,
                        Some(matched),
                        Some(after_entity),
                        commit_sha,
                        author,
                        None,
                    ));
                }
            }
        }
    }

    // Remaining unmatched before = deleted
    for entity in before.iter().filter(|e| !matched_before.contains(e.id.as_str())) {
        changes.push(build_change(
            format!("change::deleted::{}", entity.id),
            entity.id.clone(),
            ChangeType::Deleted,
            entity.entity_type.clone(),
            entity.name.clone(),
            entity.file_path.clone(),
            None,
            Some(entity),
            None,
            commit_sha,
            author,
            None,
        ));
    }

    // Remaining unmatched after = added
    for entity in after.iter().filter(|e| !matched_after.contains(e.id.as_str())) {
        changes.push(build_change(
            format!("change::added::{}", entity.id),
            entity.id.clone(),
            ChangeType::Added,
            entity.entity_type.clone(),
            entity.name.clone(),
            entity.file_path.clone(),
            None,
            None,
            Some(entity),
            commit_sha,
            author,
            None,
        ));
    }

    MatchResult { changes }
}

/// Default content similarity using Jaccard index on whitespace-split tokens
pub fn default_similarity(a: &SemanticEntity, b: &SemanticEntity) -> f64 {
    let tokens_a: Vec<&str> = a.content.split_whitespace().collect();
    let tokens_b: Vec<&str> = b.content.split_whitespace().collect();

    // Early rejection: if token counts differ too much, Jaccard can't reach 0.8
    let (min_c, max_c) = if tokens_a.len() < tokens_b.len() {
        (tokens_a.len(), tokens_b.len())
    } else {
        (tokens_b.len(), tokens_a.len())
    };
    if max_c > 0 && (min_c as f64 / max_c as f64) < 0.6 {
        return 0.0;
    }

    let set_a: HashSet<&str> = tokens_a.into_iter().collect();
    let set_b: HashSet<&str> = tokens_b.into_iter().collect();

    let intersection_size = set_a.intersection(&set_b).count();
    let union_size = set_a.union(&set_b).count();

    if union_size == 0 {
        return 0.0;
    }

    intersection_size as f64 / union_size as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::hash::content_hash;

    fn make_entity_with_lines(
        id: &str,
        name: &str,
        content: &str,
        file_path: &str,
        start_line: usize,
        end_line: usize,
    ) -> SemanticEntity {
        SemanticEntity {
            id: id.to_string(),
            file_path: file_path.to_string(),
            entity_type: "function".to_string(),
            name: name.to_string(),
            parent_id: None,
            content: content.to_string(),
            content_hash: content_hash(content),
            structural_hash: None,
            start_line,
            end_line,
            metadata: None,
        }
    }

    fn make_entity(id: &str, name: &str, content: &str, file_path: &str) -> SemanticEntity {
        make_entity_with_lines(id, name, content, file_path, 1, 1)
    }

    #[test]
    fn test_exact_match_modified() {
        let before = vec![make_entity_with_lines(
            "a::f::foo",
            "foo",
            "old content",
            "a.ts",
            4,
            8,
        )];
        let after = vec![make_entity_with_lines(
            "a::f::foo",
            "foo",
            "new content",
            "a.ts",
            5,
            11,
        )];
        let result = match_entities(&before, &after, "a.ts", None, None, None);
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].change_type, ChangeType::Modified);
        assert_eq!(result.changes[0].before_start_line, Some(4));
        assert_eq!(result.changes[0].before_end_line, Some(8));
        assert_eq!(result.changes[0].after_start_line, Some(5));
        assert_eq!(result.changes[0].after_end_line, Some(11));
    }

    #[test]
    fn test_exact_match_unchanged() {
        let before = vec![make_entity("a::f::foo", "foo", "same", "a.ts")];
        let after = vec![make_entity("a::f::foo", "foo", "same", "a.ts")];
        let result = match_entities(&before, &after, "a.ts", None, None, None);
        assert_eq!(result.changes.len(), 0);
    }

    #[test]
    fn test_added_deleted() {
        let before = vec![make_entity_with_lines(
            "a::f::old",
            "old",
            "content",
            "a.ts",
            10,
            12,
        )];
        let after = vec![make_entity_with_lines(
            "a::f::new",
            "new",
            "different",
            "a.ts",
            20,
            22,
        )];
        let result = match_entities(&before, &after, "a.ts", None, None, None);
        assert_eq!(result.changes.len(), 2);
        let deleted = result
            .changes
            .iter()
            .find(|change| change.change_type == ChangeType::Deleted)
            .expect("expected deleted change");
        assert_eq!(deleted.before_start_line, Some(10));
        assert_eq!(deleted.before_end_line, Some(12));
        assert_eq!(deleted.after_start_line, None);
        assert_eq!(deleted.after_end_line, None);

        let added = result
            .changes
            .iter()
            .find(|change| change.change_type == ChangeType::Added)
            .expect("expected added change");
        assert_eq!(added.before_start_line, None);
        assert_eq!(added.before_end_line, None);
        assert_eq!(added.after_start_line, Some(20));
        assert_eq!(added.after_end_line, Some(22));
    }

    #[test]
    fn test_content_hash_rename() {
        let before = vec![make_entity_with_lines(
            "a::f::old",
            "old",
            "same content",
            "a.ts",
            30,
            35,
        )];
        let after = vec![make_entity_with_lines(
            "a::f::new",
            "new",
            "same content",
            "a.ts",
            40,
            44,
        )];
        let result = match_entities(&before, &after, "a.ts", None, None, None);
        assert_eq!(result.changes.len(), 1);
        assert_eq!(result.changes[0].change_type, ChangeType::Renamed);
        assert_eq!(result.changes[0].before_start_line, Some(30));
        assert_eq!(result.changes[0].before_end_line, Some(35));
        assert_eq!(result.changes[0].after_start_line, Some(40));
        assert_eq!(result.changes[0].after_end_line, Some(44));
    }

    #[test]
    fn test_content_hash_moved_cross_file_line_ranges() {
        let before = vec![make_entity_with_lines(
            "src/a.ts::f::old",
            "old",
            "same content",
            "src/a.ts",
            7,
            13,
        )];
        let after = vec![make_entity_with_lines(
            "src/b.ts::f::new",
            "new",
            "same content",
            "src/b.ts",
            17,
            24,
        )];
        let result = match_entities(&before, &after, "src/b.ts", None, None, None);
        assert_eq!(result.changes.len(), 1);
        let change = &result.changes[0];
        assert_eq!(change.change_type, ChangeType::Moved);
        assert_eq!(change.old_file_path.as_deref(), Some("src/a.ts"));
        assert_eq!(change.before_start_line, Some(7));
        assert_eq!(change.before_end_line, Some(13));
        assert_eq!(change.after_start_line, Some(17));
        assert_eq!(change.after_end_line, Some(24));
    }

    #[test]
    fn test_similarity_moved_cross_file_line_ranges() {
        let before = vec![make_entity_with_lines(
            "src/a.ts::f::old",
            "old",
            "alpha beta gamma delta",
            "src/a.ts",
            50,
            55,
        )];
        let after = vec![make_entity_with_lines(
            "src/b.ts::f::new",
            "new",
            "alpha beta gamma epsilon",
            "src/b.ts",
            60,
            66,
        )];
        let similarity = |_: &SemanticEntity, _: &SemanticEntity| 0.95;
        let result = match_entities(&before, &after, "src/b.ts", Some(&similarity), None, None);

        assert_eq!(result.changes.len(), 1);
        let change = &result.changes[0];
        assert_eq!(change.change_type, ChangeType::Moved);
        assert_eq!(change.old_file_path.as_deref(), Some("src/a.ts"));
        assert_eq!(change.before_start_line, Some(50));
        assert_eq!(change.before_end_line, Some(55));
        assert_eq!(change.after_start_line, Some(60));
        assert_eq!(change.after_end_line, Some(66));
    }

    #[test]
    fn test_default_similarity() {
        let a = make_entity("a", "a", "the quick brown fox", "a.ts");
        let b = make_entity("b", "b", "the quick brown dog", "a.ts");
        let score = default_similarity(&a, &b);
        assert!(score > 0.5);
        assert!(score < 1.0);
    }
}
