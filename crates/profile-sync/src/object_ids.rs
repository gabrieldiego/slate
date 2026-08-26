use std::collections::BTreeSet;

pub(crate) fn push_unique_object_id(
    object_ids: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    object_id: &str,
) {
    let object_id = object_id.to_string();
    if seen.insert(object_id.clone()) {
        object_ids.push(object_id);
    }
}

pub(crate) fn extend_unique_object_ids(
    object_ids: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
    incoming: impl IntoIterator<Item = String>,
) {
    for object_id in incoming {
        push_unique_object_id(object_ids, seen, &object_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{extend_unique_object_ids, push_unique_object_id};
    use std::collections::BTreeSet;

    #[test]
    fn push_unique_object_id_keeps_first_seen_order() {
        let mut object_ids = Vec::new();
        let mut seen = BTreeSet::new();

        push_unique_object_id(&mut object_ids, &mut seen, "object-a");
        push_unique_object_id(&mut object_ids, &mut seen, "object-b");
        push_unique_object_id(&mut object_ids, &mut seen, "object-a");

        assert_eq!(object_ids, vec!["object-a", "object-b"]);
    }

    #[test]
    fn extend_unique_object_ids_skips_existing_entries() {
        let mut object_ids = vec!["object-a".to_string()];
        let mut seen = object_ids.iter().cloned().collect::<BTreeSet<_>>();

        extend_unique_object_ids(
            &mut object_ids,
            &mut seen,
            ["object-b".to_string(), "object-a".to_string()],
        );

        assert_eq!(object_ids, vec!["object-a", "object-b"]);
    }
}
