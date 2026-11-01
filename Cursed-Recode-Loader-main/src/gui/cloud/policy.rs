//! Three-way synchronisation: a downloaded config may contain newer settings this build
//! cannot represent. Keep the remote baseline and the locally applied baseline separately.
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Baseline {
    pub id: i64,
    pub version: i64,
    pub remote: String,
    pub local: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Synced,
    Upload,
    Download,
    Conflict,
    SharedLocal,
}

pub(super) fn decide(local: &str, remote: &str, baseline: &Baseline, own: bool) -> Decision {
    if local == remote {
        return Decision::Synced;
    }
    if baseline.remote.is_empty() || baseline.local.is_empty() {
        return Decision::Conflict;
    }
    let here = local != baseline.local;
    let there = remote != baseline.remote;
    match (here, there) {
        (false, false) => Decision::Synced,
        (true, false) if own => Decision::Upload,
        (true, false) => Decision::SharedLocal,
        (false, true) => Decision::Download,
        (true, true) => Decision::Conflict,
    }
}

/// This build overwrites only keys it actually knows; settings from a newer build survive
/// the round trip even when an older client changes one of its supported settings.
pub(super) fn merge_menu(local: &str, remote: &str, known_before: &str) -> String {
    let known: std::collections::HashSet<_> = known_before.lines().filter_map(|l| l.split_once('=')).map(|(k, _)| k).collect();
    let mut values: std::collections::BTreeMap<&str, &str> = remote.lines().filter_map(|l| l.split_once('=')).collect();
    values.retain(|k, _| !k.starts_with("text.") && !known.contains(k));
    values.extend(local.lines().filter_map(|l| l.split_once('=')));
    values.into_iter().map(|(k, v)| format!("{k}={v}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_changes_wait_for_a_choice_and_never_silently_win() {
        let baseline = Baseline {
            id: 3,
            version: 1,
            remote: "old".into(),
            local: "old".into(),
        };
        assert_eq!(decide("mine", "theirs", &baseline, true), Decision::Conflict);
        assert_eq!(decide("mine", "old", &baseline, true), Decision::Upload);
        assert_eq!(decide("old", "theirs", &baseline, true), Decision::Download);
        assert_eq!(decide("same", "same", &baseline, true), Decision::Synced);
        assert_eq!(decide("mine", "theirs", &Baseline::default(), true), Decision::Conflict);
        assert_eq!(decide("mine", "old", &baseline, false), Decision::SharedLocal);
    }

    #[test]
    fn an_older_client_does_not_upload_again_just_because_it_cannot_render_new_keys() {
        let baseline = Baseline {
            id: 3,
            version: 1,
            remote: "new-format".into(),
            local: "normalised".into(),
        };
        assert_eq!(decide("normalised", "new-format", &baseline, true), Decision::Synced);
        let joined = merge_menu("#parts=31\nknown=changed\n", "#parts=31\nknown=before\nfuture.setting=kept\n", "");
        assert!(joined.contains("known=changed\n"));
        assert!(joined.contains("future.setting=kept\n"));
        assert!(!joined.contains("known=before\n"));
        let deleted = merge_menu(
            "known=kept\n",
            "known=kept\ntext.Search=Custom\nsetting#binds=Alt+K\nfuture.setting=kept\n",
            "known=kept\ntext.Search=Custom\nsetting#binds=Alt+K\n",
        );
        assert!(!deleted.contains("text.Search="));
        assert!(!deleted.contains("setting#binds="));
        assert!(deleted.contains("future.setting=kept\n"));
    }
}
