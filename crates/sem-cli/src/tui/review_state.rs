use chrono::Utc;
use git2::Repository;
use sem_core::model::change::{ChangeType, SemanticChange};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const REVIEW_STATE_VERSION: u32 = 1;
const MAX_REVIEW_RECORDS: usize = 20_000;
const REVIEW_STATE_FILE_NAME: &str = "tui-review-state.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewFilter {
    #[default]
    All,
    Unreviewed,
    Reviewed,
}

impl ReviewFilter {
    pub fn cycle(self) -> Self {
        match self {
            Self::All => Self::Unreviewed,
            Self::Unreviewed => Self::Reviewed,
            Self::Reviewed => Self::All,
        }
    }

    pub fn as_token(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unreviewed => "unreviewed",
            Self::Reviewed => "reviewed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ReviewIdentity {
    pub logical_entity_key: String,
    pub target_content_hash: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewStateData {
    pub filter: ReviewFilter,
    pub records: HashMap<ReviewIdentity, String>,
}

#[derive(Clone, Debug, Default)]
pub struct ReviewStateLoadResult {
    pub state: ReviewStateData,
    pub warning: Option<String>,
    pub compacted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewStateStore {
    repo_id: String,
    file_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReviewStateStoreInit {
    Available(ReviewStateStore),
    Unavailable(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedReviewState {
    version: u32,
    repo_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ui_prefs: Option<PersistedUiPrefs>,
    review_records: Vec<PersistedReviewRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUiPrefs {
    review_filter: ReviewFilter,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedReviewRecord {
    logical_entity_key: String,
    target_content_hash: String,
    updated_at: String,
}

impl ReviewStateStore {
    pub fn initialize(cwd: &str) -> ReviewStateStoreInit {
        let cwd_path = if cwd.is_empty() {
            match std::env::current_dir() {
                Ok(path) => path,
                Err(error) => {
                    return ReviewStateStoreInit::Unavailable(format!(
                        "review persistence unavailable: resolving current directory: {error}"
                    ))
                }
            }
        } else {
            PathBuf::from(cwd)
        };

        let repository = match Repository::discover(&cwd_path) {
            Ok(repository) => repository,
            Err(_) => {
                return ReviewStateStoreInit::Unavailable(
                    "review persistence unavailable: not inside a Git repository".to_string(),
                )
            }
        };

        let Some(workdir) = repository.workdir() else {
            return ReviewStateStoreInit::Unavailable(
                "review persistence unavailable: bare repositories are unsupported".to_string(),
            );
        };

        let canonical_root = fs::canonicalize(workdir).unwrap_or_else(|_| workdir.to_path_buf());
        let canonical_root_string = canonical_root.to_string_lossy().replace('\\', "/");
        let normalized_root = canonical_root_string.trim_end_matches('/').to_string();
        let repo_id = sha256_hex(normalized_root.as_bytes());
        let file_path = canonical_root.join(".sem").join(REVIEW_STATE_FILE_NAME);

        ReviewStateStoreInit::Available(Self { repo_id, file_path })
    }

    pub fn load(&self) -> ReviewStateLoadResult {
        let mut result = ReviewStateLoadResult::default();

        let raw = match fs::read_to_string(&self.file_path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return result;
            }
            Err(error) => {
                result.warning = Some(format!(
                    "review persistence read failed ({}): {error}",
                    self.file_path.display()
                ));
                return result;
            }
        };

        let persisted: PersistedReviewState = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(error) => {
                result.warning = Some(format!(
                    "review persistence ignored: invalid JSON at {} ({error})",
                    self.file_path.display()
                ));
                return result;
            }
        };

        if persisted.version != REVIEW_STATE_VERSION {
            result.warning = Some(format!(
                "review persistence ignored: unsupported schema version {}",
                persisted.version
            ));
            return result;
        }

        if persisted.repo_id != self.repo_id {
            result.warning = Some("review persistence ignored: repoId mismatch".to_string());
            return result;
        }

        let (records, compacted) = compact_records(persisted.review_records);
        let filter = persisted
            .ui_prefs
            .map(|prefs| prefs.review_filter)
            .unwrap_or_default();

        result.state = ReviewStateData { filter, records };
        result.compacted = compacted;
        result
    }

    pub fn save(&self, state: &ReviewStateData) -> Result<(), String> {
        let parent = self.file_path.parent().ok_or_else(|| {
            format!(
                "invalid review persistence path: {}",
                self.file_path.display()
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "creating review state directory {}: {error}",
                parent.display()
            )
        })?;

        let mut review_records: Vec<PersistedReviewRecord> = state
            .records
            .iter()
            .map(|(identity, updated_at)| PersistedReviewRecord {
                logical_entity_key: identity.logical_entity_key.clone(),
                target_content_hash: identity.target_content_hash.clone(),
                updated_at: updated_at.clone(),
            })
            .collect();
        review_records.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.logical_entity_key.cmp(&right.logical_entity_key))
                .then_with(|| left.target_content_hash.cmp(&right.target_content_hash))
        });

        let payload = PersistedReviewState {
            version: REVIEW_STATE_VERSION,
            repo_id: self.repo_id.clone(),
            ui_prefs: Some(PersistedUiPrefs {
                review_filter: state.filter,
            }),
            review_records,
        };
        let encoded = serde_json::to_vec_pretty(&payload)
            .map_err(|error| format!("encoding review persistence JSON: {error}"))?;

        let tmp_name = format!(
            ".{}.tmp-{}-{}",
            REVIEW_STATE_FILE_NAME,
            std::process::id(),
            monotonic_nanos()
        );
        let tmp_path = parent.join(tmp_name);

        let mut tmp_file = fs::File::create(&tmp_path).map_err(|error| {
            format!(
                "creating temp review state file {}: {error}",
                tmp_path.display()
            )
        })?;
        tmp_file
            .write_all(&encoded)
            .and_then(|_| tmp_file.flush())
            .map_err(|error| {
                format!(
                    "writing temp review state file {}: {error}",
                    tmp_path.display()
                )
            })?;

        fs::rename(&tmp_path, &self.file_path).map_err(|error| {
            let _ = fs::remove_file(&tmp_path);
            format!(
                "atomically replacing review state file {}: {error}",
                self.file_path.display()
            )
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

pub fn review_state_file_rel_path() -> &'static str {
    ".sem/tui-review-state.json"
}

pub fn endpoint_supports_review_hash(endpoint_id: Option<&str>) -> bool {
    let Some(endpoint_id) = endpoint_id else {
        return false;
    };

    if endpoint_id.eq_ignore_ascii_case("index") || endpoint_id.eq_ignore_ascii_case("working") {
        return true;
    }

    endpoint_id
        .strip_prefix("commit:")
        .map(|sha| !sha.is_empty())
        .unwrap_or(false)
}

pub fn current_updated_at() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn build_logical_entity_key(change: &SemanticChange, fallback_ordinal: usize) -> String {
    if !change.entity_id.trim().is_empty() {
        return format!("entityId::{}", change.entity_id);
    }

    format!(
        "fallback::{}::{}::{}::{}",
        change.file_path, change.entity_type, change.entity_name, fallback_ordinal
    )
}

pub fn hash_material_for_change(change: &SemanticChange) -> Option<&str> {
    match change.change_type {
        ChangeType::Deleted => change.before_content.as_deref(),
        _ => change
            .after_content
            .as_deref()
            .or(change.before_content.as_deref()),
    }
}

pub fn build_target_content_hash(change: &SemanticChange) -> Option<String> {
    hash_material_for_change(change).map(hash_normalized_text)
}

pub fn hash_normalized_text(text: &str) -> String {
    let normalized = normalize_hash_material(text);
    sha256_hex(normalized.as_bytes())
}

pub fn normalize_hash_material(text: &str) -> String {
    let mut normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    while normalized.ends_with("\n\n") {
        normalized.pop();
    }
    normalized
}

fn sha256_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    format!("sha256:{digest:x}")
}

fn monotonic_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn compact_records(input: Vec<PersistedReviewRecord>) -> (HashMap<ReviewIdentity, String>, bool) {
    let input_len = input.len();
    let mut deduped = HashMap::<ReviewIdentity, String>::new();

    for record in input {
        let identity = ReviewIdentity {
            logical_entity_key: record.logical_entity_key,
            target_content_hash: record.target_content_hash,
        };

        let replace = deduped
            .get(&identity)
            .map(|existing| record.updated_at > *existing)
            .unwrap_or(true);
        if replace {
            deduped.insert(identity, record.updated_at);
        }
    }

    let mut compacted = deduped.len() != input_len;

    if deduped.len() > MAX_REVIEW_RECORDS {
        let mut entries: Vec<(ReviewIdentity, String)> = deduped.into_iter().collect();
        entries.sort_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| left.0.logical_entity_key.cmp(&right.0.logical_entity_key))
                .then_with(|| left.0.target_content_hash.cmp(&right.0.target_content_hash))
        });

        let drain_count = entries.len() - MAX_REVIEW_RECORDS;
        entries.drain(0..drain_count);
        deduped = entries.into_iter().collect();
        compacted = true;
    }

    (deduped, compacted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sem_core::model::change::ChangeType;
    use std::process::Command;

    fn sample_change(change_type: ChangeType) -> SemanticChange {
        SemanticChange {
            id: "change::1".to_string(),
            entity_id: "src/app.rs::function::run".to_string(),
            change_type,
            entity_type: "function".to_string(),
            entity_name: "run".to_string(),
            file_path: "src/app.rs".to_string(),
            old_file_path: None,
            before_content: Some("fn run() {\n  old();\n}\n\n".to_string()),
            after_content: Some("fn run() {\n  new();\n}\n".to_string()),
            commit_sha: None,
            author: None,
            timestamp: None,
            structural_change: Some(true),
            before_start_line: Some(1),
            before_end_line: Some(3),
            after_start_line: Some(1),
            after_end_line: Some(3),
        }
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("{prefix}-{}", monotonic_nanos()));
        fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    fn init_repo(path: &Path) {
        run_git(path, &["init"]);
        run_git(path, &["config", "user.email", "sem@example.com"]);
        run_git(path, &["config", "user.name", "sem"]);
        fs::write(path.join("file.txt"), "first\n").expect("seed file should write");
        run_git(path, &["add", "."]);
        run_git(path, &["commit", "-m", "init"]);
    }

    fn run_git(path: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(path)
            .status()
            .expect("git should run");
        assert!(status.success(), "git {:?} should pass", args);
    }

    #[test]
    fn review_filter_cycles_all_states() {
        assert_eq!(ReviewFilter::All.cycle(), ReviewFilter::Unreviewed);
        assert_eq!(ReviewFilter::Unreviewed.cycle(), ReviewFilter::Reviewed);
        assert_eq!(ReviewFilter::Reviewed.cycle(), ReviewFilter::All);
    }

    #[test]
    fn endpoint_kind_supports_commit_index_and_working() {
        assert!(endpoint_supports_review_hash(Some("commit:abc123")));
        assert!(endpoint_supports_review_hash(Some("index")));
        assert!(endpoint_supports_review_hash(Some("WORKING")));
        assert!(!endpoint_supports_review_hash(Some("")));
        assert!(!endpoint_supports_review_hash(Some("blob:123")));
        assert!(!endpoint_supports_review_hash(None));
    }

    #[test]
    fn logical_entity_key_prefers_entity_id_and_has_fallback() {
        let with_id = sample_change(ChangeType::Modified);
        assert_eq!(
            build_logical_entity_key(&with_id, 3),
            "entityId::src/app.rs::function::run"
        );

        let mut without_id = sample_change(ChangeType::Modified);
        without_id.entity_id.clear();
        assert_eq!(
            build_logical_entity_key(&without_id, 2),
            "fallback::src/app.rs::function::run::2"
        );
    }

    #[test]
    fn normalize_hash_material_unifies_line_endings_and_trailing_newlines() {
        assert_eq!(normalize_hash_material("a\r\nb\r\n\r\n"), "a\nb\n");
        assert_eq!(normalize_hash_material("a\n\n\n"), "a\n");
        assert_eq!(normalize_hash_material("a\nb"), "a\nb");
    }

    #[test]
    fn deleted_changes_hash_from_before_content() {
        let mut deleted = sample_change(ChangeType::Deleted);
        deleted.after_content = None;

        let hash = build_target_content_hash(&deleted).expect("deleted should hash");
        let expected = hash_normalized_text("fn run() {\n  old();\n}\n\n");
        assert_eq!(hash, expected);
    }

    #[test]
    fn load_missing_review_state_file_is_empty_without_warning() {
        let repo_dir = temp_dir("sem-review-state-missing");
        init_repo(&repo_dir);

        let store = match ReviewStateStore::initialize(repo_dir.to_string_lossy().as_ref()) {
            ReviewStateStoreInit::Available(store) => store,
            ReviewStateStoreInit::Unavailable(reason) => {
                panic!("store should initialize: {reason}")
            }
        };

        let result = store.load();
        assert_eq!(result.state, ReviewStateData::default());
        assert_eq!(result.warning, None);
        assert!(!result.compacted);

        let _ = fs::remove_dir_all(repo_dir);
    }

    #[test]
    fn load_corrupt_review_state_file_returns_warning() {
        let repo_dir = temp_dir("sem-review-state-corrupt");
        init_repo(&repo_dir);

        let store = match ReviewStateStore::initialize(repo_dir.to_string_lossy().as_ref()) {
            ReviewStateStoreInit::Available(store) => store,
            ReviewStateStoreInit::Unavailable(reason) => {
                panic!("store should initialize: {reason}")
            }
        };
        fs::create_dir_all(store.file_path().parent().expect("has parent"))
            .expect("state dir should be created");
        fs::write(store.file_path(), "{not-json").expect("corrupt file should be written");

        let result = store.load();
        assert!(result.warning.is_some());
        assert_eq!(result.state, ReviewStateData::default());

        let _ = fs::remove_dir_all(repo_dir);
    }

    #[test]
    fn load_unsupported_version_is_ignored() {
        let repo_dir = temp_dir("sem-review-state-version");
        init_repo(&repo_dir);

        let store = match ReviewStateStore::initialize(repo_dir.to_string_lossy().as_ref()) {
            ReviewStateStoreInit::Available(store) => store,
            ReviewStateStoreInit::Unavailable(reason) => {
                panic!("store should initialize: {reason}")
            }
        };

        let payload = serde_json::json!({
            "version": 2,
            "repoId": "sha256:deadbeef",
            "reviewRecords": []
        });
        fs::create_dir_all(store.file_path().parent().expect("has parent"))
            .expect("state dir should be created");
        fs::write(
            store.file_path(),
            serde_json::to_vec_pretty(&payload).expect("json should encode"),
        )
        .expect("file should write");

        let result = store.load();
        assert!(result
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("unsupported schema version"));
        assert_eq!(result.state, ReviewStateData::default());

        let _ = fs::remove_dir_all(repo_dir);
    }

    #[test]
    fn load_repo_mismatch_is_ignored() {
        let repo_dir = temp_dir("sem-review-state-repo-mismatch");
        init_repo(&repo_dir);

        let store = match ReviewStateStore::initialize(repo_dir.to_string_lossy().as_ref()) {
            ReviewStateStoreInit::Available(store) => store,
            ReviewStateStoreInit::Unavailable(reason) => {
                panic!("store should initialize: {reason}")
            }
        };

        let payload = serde_json::json!({
            "version": 1,
            "repoId": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "uiPrefs": { "reviewFilter": "reviewed" },
            "reviewRecords": [
                {
                    "logicalEntityKey": "entityId::x",
                    "targetContentHash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "updatedAt": "2026-03-08T20:00:00Z"
                }
            ]
        });
        fs::create_dir_all(store.file_path().parent().expect("has parent"))
            .expect("state dir should be created");
        fs::write(
            store.file_path(),
            serde_json::to_vec_pretty(&payload).expect("json should encode"),
        )
        .expect("file should write");

        let result = store.load();
        assert!(result
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("repoId mismatch"));
        assert_eq!(result.state, ReviewStateData::default());

        let _ = fs::remove_dir_all(repo_dir);
    }

    #[test]
    fn compaction_dedupes_and_caps_record_count() {
        let repo_dir = temp_dir("sem-review-state-compact");
        init_repo(&repo_dir);

        let store = match ReviewStateStore::initialize(repo_dir.to_string_lossy().as_ref()) {
            ReviewStateStoreInit::Available(store) => store,
            ReviewStateStoreInit::Unavailable(reason) => {
                panic!("store should initialize: {reason}")
            }
        };

        let mut records = Vec::new();
        for i in 0..(MAX_REVIEW_RECORDS + 5) {
            records.push(serde_json::json!({
                "logicalEntityKey": format!("entityId::{i}"),
                "targetContentHash": format!("sha256:{:064x}", i),
                "updatedAt": format!("2026-03-08T20:{:02}:00Z", i % 60),
            }));
        }
        records.push(serde_json::json!({
            "logicalEntityKey": "entityId::dup",
            "targetContentHash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "updatedAt": "2026-03-08T20:00:00Z",
        }));
        records.push(serde_json::json!({
            "logicalEntityKey": "entityId::dup",
            "targetContentHash": "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "updatedAt": "2026-03-08T21:00:00Z",
        }));

        let payload = serde_json::json!({
            "version": 1,
            "repoId": store.repo_id.clone(),
            "uiPrefs": { "reviewFilter": "reviewed" },
            "reviewRecords": records,
        });

        fs::create_dir_all(store.file_path().parent().expect("has parent"))
            .expect("state dir should be created");
        fs::write(
            store.file_path(),
            serde_json::to_vec_pretty(&payload).expect("json should encode"),
        )
        .expect("file should write");

        let result = store.load();
        assert!(result.compacted);
        assert_eq!(result.state.filter, ReviewFilter::Reviewed);
        assert!(result.state.records.len() <= MAX_REVIEW_RECORDS);
        assert_eq!(
            result.state.records.get(&ReviewIdentity {
                logical_entity_key: "entityId::dup".to_string(),
                target_content_hash:
                    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
                        .to_string(),
            }),
            Some(&"2026-03-08T21:00:00Z".to_string())
        );

        let _ = fs::remove_dir_all(repo_dir);
    }

    #[test]
    fn save_then_load_round_trips_filter_and_records() {
        let repo_dir = temp_dir("sem-review-state-roundtrip");
        init_repo(&repo_dir);

        let store = match ReviewStateStore::initialize(repo_dir.to_string_lossy().as_ref()) {
            ReviewStateStoreInit::Available(store) => store,
            ReviewStateStoreInit::Unavailable(reason) => {
                panic!("store should initialize: {reason}")
            }
        };

        let mut state = ReviewStateData {
            filter: ReviewFilter::Unreviewed,
            records: HashMap::new(),
        };
        state.records.insert(
            ReviewIdentity {
                logical_entity_key: "entityId::src/app.rs::function::run".to_string(),
                target_content_hash:
                    "sha256:1111111111111111111111111111111111111111111111111111111111111111"
                        .to_string(),
            },
            "2026-03-08T20:10:00Z".to_string(),
        );

        store.save(&state).expect("save should succeed");
        assert!(store.file_path().exists());

        let loaded = store.load();
        assert_eq!(loaded.warning, None);
        assert_eq!(loaded.state, state);

        let _ = fs::remove_dir_all(repo_dir);
    }

    #[test]
    fn current_updated_at_uses_utc_second_precision_format() {
        let value = current_updated_at();
        assert_eq!(value.len(), 20);
        assert!(value.ends_with('Z'));
        assert_eq!(&value[4..5], "-");
        assert_eq!(&value[7..8], "-");
        assert_eq!(&value[10..11], "T");
        assert_eq!(&value[13..14], ":");
        assert_eq!(&value[16..17], ":");
    }
}
