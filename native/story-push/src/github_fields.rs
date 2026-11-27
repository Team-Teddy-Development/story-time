//! GitHub organization issue fields for named epics and features: provisioning the `Epic` and
//! `Feature` single-select fields (`story group --sync-fields`) and mirroring a story's values
//! onto its issue during `story push`. GitHub only.
//!
//! REST, per github/rest-api-description:
//! - `GET/POST /orgs/{org}/issue-fields`, `PATCH /orgs/{org}/issue-fields/{id}` (options
//!   array replaces the set; existing options are kept by sending their `id`).
//! - `GET/POST /repos/{o}/{r}/issues/{n}/issue-field-values` (POST adds without clearing
//!   others) and `DELETE …/issue-field-values/{field_id}`.

use crate::gateway::{ErrorKind, TrackerError};
use serde_json::{json, Value};

pub(crate) const EPIC_FIELD: &str = "Epic";
pub(crate) const FEATURE_FIELD: &str = "Feature";

/// The REST transport the field logic needs; `gateway::Cli` is the real adapter.
pub(crate) trait GithubApi {
    fn rest(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, TrackerError>;
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldOption {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) color: String,
    pub(crate) priority: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Field {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) options: Vec<FieldOption>,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

fn number(v: &Value) -> Option<u64> {
    v.as_u64()
}

fn parse_field(v: &Value) -> Option<Field> {
    let options = match &v["options"] {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(i, o)| {
                Some(FieldOption {
                    id: number(&o["id"])?,
                    name: o["name"].as_str()?.to_owned(),
                    color: o["color"].as_str().unwrap_or("gray").to_owned(),
                    priority: number(&o["priority"]).unwrap_or(i as u64 + 1),
                })
            })
            .collect::<Option<Vec<_>>>()?,
        _ => Vec::new(),
    };
    Some(Field {
        id: number(&v["id"])?,
        name: v["name"].as_str()?.to_owned(),
        data_type: v["data_type"].as_str()?.to_owned(),
        options,
    })
}

/// The organization's issue fields, or `None` when the owner has none to offer (404: a user
/// account, or issue fields not enabled). Any other failure is an error, never "no fields".
pub(crate) fn org_fields(
    api: &dyn GithubApi,
    org: &str,
) -> Result<Option<Vec<Field>>, TrackerError> {
    match api.rest("GET", &format!("orgs/{org}/issue-fields"), None) {
        Ok(Value::Array(items)) => items
            .iter()
            .map(parse_field)
            .collect::<Option<Vec<_>>>()
            .map(Some)
            .ok_or_else(|| TrackerError::permanent("GitHub returned an invalid issue field.")),
        Ok(_) => Err(TrackerError::permanent(
            "GitHub returned an invalid issue field list.",
        )),
        Err(e) if e.kind == ErrorKind::Permanent => Ok(None),
        Err(e) => Err(e),
    }
}

fn named<'a>(fields: &'a [Field], name: &str) -> Option<&'a Field> {
    fields.iter().find(|f| same(&f.name, name))
}

/// The value to send for a story name: a single-select option's own spelling, or the text.
fn value_for(field: &Field, name: &str) -> Result<String, TrackerError> {
    match field.data_type.as_str() {
        "single_select" => field
            .options
            .iter()
            .find(|o| same(&o.name, name))
            .map(|o| o.name.clone())
            .ok_or_else(|| {
                TrackerError::permanent(format!(
                    "GitHub issue field \"{}\" has no option \"{name}\"; run story group --sync-fields (organization admin) to add it.",
                    field.name
                ))
            }),
        "text" => Ok(name.to_owned()),
        other => Err(TrackerError::permanent(format!(
            "GitHub issue field \"{}\" is {other}; story-time writes single_select or text.",
            field.name
        ))),
    }
}

fn current_value(item: &Value) -> Option<String> {
    item["single_select_option"]["name"]
        .as_str()
        .or_else(|| item["value"].as_str())
        .map(str::to_owned)
}

/// Sets the issue's `Epic` and `Feature` values to the story's names and clears a value the
/// story no longer has. Other fields are never touched. No-op when neither field exists.
pub(crate) fn mirror(
    api: &dyn GithubApi,
    repo: &str,
    issue: &str,
    fields: &[Field],
    epic: Option<&str>,
    feature: Option<&str>,
) -> Result<(), TrackerError> {
    let targets: Vec<(&Field, Option<String>)> = [(EPIC_FIELD, epic), (FEATURE_FIELD, feature)]
        .into_iter()
        .filter_map(|(name, wanted)| named(fields, name).map(|f| (f, wanted)))
        .map(|(f, wanted)| Ok((f, wanted.map(|w| value_for(f, w)).transpose()?)))
        .collect::<Result<_, TrackerError>>()?;
    if targets.is_empty() {
        return Ok(());
    }
    let path = format!("repos/{repo}/issues/{issue}/issue-field-values");
    let current = api.rest("GET", &path, None)?;
    let current = current
        .as_array()
        .ok_or_else(|| TrackerError::permanent("GitHub returned invalid issue field values."))?;
    let now = |id: u64| {
        current
            .iter()
            .find(|v| number(&v["issue_field_id"]) == Some(id))
            .and_then(current_value)
    };
    let mut set = Vec::new();
    for (field, wanted) in &targets {
        match (wanted, now(field.id)) {
            (Some(w), Some(c)) if same(w, &c) => {}
            (Some(w), _) => set.push(json!({"field_id": field.id, "value": w})),
            (None, Some(_)) => {
                api.rest("DELETE", &format!("{path}/{}", field.id), None)?;
            }
            (None, None) => {}
        }
    }
    if !set.is_empty() {
        api.rest("POST", &path, Some(json!({"issue_field_values": set})))?;
    }
    Ok(())
}

/// What `sync` changed, for the report.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Change {
    Created { field: String, options: Vec<String> },
    Extended { field: String, added: Vec<String> },
    Unchanged { field: String },
}

/// Makes sure `Epic` holds every epic name and `Feature` every feature name as single-select
/// options, creating a field that is missing. Existing options are kept (by id) and never
/// deleted, so a team's own options and every issue's value survive. A text field is left
/// as text: it already takes any name.
pub(crate) fn sync(
    api: &dyn GithubApi,
    org: &str,
    epics: &[String],
    features: &[String],
) -> Result<Vec<Change>, TrackerError> {
    let fields = org_fields(api, org)?.ok_or_else(|| {
        TrackerError::permanent(format!(
            "{org} has no organization issue fields (HTTP 404); issue fields need an organization-owned repository."
        ))
    })?;
    let plan = [
        (
            EPIC_FIELD,
            epics,
            "blue",
            "The epic this issue belongs to, from the story-time taxonomy",
        ),
        (
            FEATURE_FIELD,
            features,
            "purple",
            "The feature this issue builds, from the story-time taxonomy",
        ),
    ];
    let mut changes = Vec::new();
    for (name, wanted, color, description) in plan {
        let Some(field) = named(&fields, name) else {
            let options: Vec<Value> = wanted
                .iter()
                .enumerate()
                .map(|(i, n)| json!({"name": n, "color": color, "priority": i + 1}))
                .collect();
            api.rest(
                "POST",
                &format!("orgs/{org}/issue-fields"),
                Some(json!({"name": name, "data_type": "single_select", "description": description, "options": options})),
            )?;
            changes.push(Change::Created {
                field: name.into(),
                options: wanted.to_vec(),
            });
            continue;
        };
        match field.data_type.as_str() {
            "text" => {
                changes.push(Change::Unchanged {
                    field: field.name.clone(),
                });
                continue;
            }
            "single_select" => {}
            other => {
                return Err(TrackerError::permanent(format!(
                    "GitHub issue field \"{}\" is {other}; story-time needs single_select or text.",
                    field.name
                )))
            }
        }
        let added: Vec<String> = wanted
            .iter()
            .filter(|w| !field.options.iter().any(|o| same(&o.name, w)))
            .cloned()
            .collect();
        if added.is_empty() {
            changes.push(Change::Unchanged {
                field: field.name.clone(),
            });
            continue;
        }
        let top = field.options.iter().map(|o| o.priority).max().unwrap_or(0);
        let options: Vec<Value> = field
            .options
            .iter()
            .map(|o| json!({"id": o.id, "name": o.name, "color": o.color, "priority": o.priority}))
            .chain(
                added.iter().enumerate().map(
                    |(i, n)| json!({"name": n, "color": color, "priority": top + i as u64 + 1}),
                ),
            )
            .collect();
        api.rest(
            "PATCH",
            &format!("orgs/{org}/issue-fields/{}", field.id),
            Some(json!({"options": options})),
        )?;
        changes.push(Change::Extended {
            field: field.name.clone(),
            added,
        });
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Records each call and answers from a script keyed by "METHOD path".
    struct Recorder {
        calls: RefCell<Vec<(String, String, Option<Value>)>>,
        replies: Vec<(&'static str, Result<Value, ErrorKind>)>,
    }
    impl GithubApi for Recorder {
        fn rest(
            &self,
            method: &str,
            path: &str,
            body: Option<Value>,
        ) -> Result<Value, TrackerError> {
            self.calls
                .borrow_mut()
                .push((method.into(), path.into(), body));
            let key = format!("{method} {path}");
            match self.replies.iter().find(|(k, _)| *k == key) {
                Some((_, Ok(v))) => Ok(v.clone()),
                Some((_, Err(kind))) => Err(TrackerError {
                    kind: *kind,
                    message: "Tracker CLI failed".into(),
                }),
                None => Ok(json!({})),
            }
        }
    }
    impl Recorder {
        fn new(replies: Vec<(&'static str, Result<Value, ErrorKind>)>) -> Self {
            Self {
                calls: RefCell::new(vec![]),
                replies,
            }
        }
        fn writes(&self) -> Vec<(String, String, Option<Value>)> {
            self.calls
                .borrow()
                .iter()
                .filter(|(m, _, _)| m != "GET")
                .cloned()
                .collect()
        }
    }

    fn org_listing() -> Value {
        json!([
          {"id": 41863314, "name": "Priority", "data_type": "single_select", "options": [{"id": 1, "name": "High", "color": "red", "priority": 1}]},
          {"id": 900, "name": "Epic", "data_type": "single_select", "options": [
             {"id": 11, "name": "Core cutover", "color": "blue", "priority": 1},
             {"id": 12, "name": "Recruit portal", "color": "blue", "priority": 2}]},
          {"id": 901, "name": "feature", "data_type": "text", "options": null}
        ])
    }

    #[test]
    fn org_fields_reads_ids_types_and_options_and_404_means_none() {
        let api = Recorder::new(vec![(
            "GET orgs/Varsity-Engine/issue-fields",
            Ok(org_listing()),
        )]);
        let fields = org_fields(&api, "Varsity-Engine").unwrap().unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1].id, 900);
        assert_eq!(
            fields[1].options[1],
            FieldOption {
                id: 12,
                name: "Recruit portal".into(),
                color: "blue".into(),
                priority: 2
            }
        );
        assert_eq!(fields[2].data_type, "text");
        let api = Recorder::new(vec![(
            "GET orgs/someone/issue-fields",
            Err(ErrorKind::Permanent),
        )]);
        assert_eq!(org_fields(&api, "someone").unwrap(), None);
        let api = Recorder::new(vec![("GET orgs/x/issue-fields", Err(ErrorKind::Retryable))]);
        assert!(
            org_fields(&api, "x").is_err(),
            "only a permanent failure (404/410) means no fields"
        );
    }

    fn fields() -> Vec<Field> {
        let api = Recorder::new(vec![("GET orgs/o/issue-fields", Ok(org_listing()))]);
        org_fields(&api, "o").unwrap().unwrap()
    }

    #[test]
    fn mirror_sets_epic_by_option_name_and_feature_as_text_leaving_other_fields_alone() {
        let api = Recorder::new(vec![(
            "GET repos/o/r/issues/7/issue-field-values",
            Ok(
                json!([{"issue_field_id": 41863314, "issue_field_name": "Priority", "data_type": "single_select", "value": "High"}]),
            ),
        )]);
        mirror(
            &api,
            "o/r",
            "7",
            &fields(),
            Some("core CUTOVER"),
            Some("Engine call cutovers"),
        )
        .unwrap();
        assert_eq!(
            api.writes(),
            [(
                "POST".to_string(),
                "repos/o/r/issues/7/issue-field-values".to_string(),
                Some(
                    json!({"issue_field_values": [{"field_id": 900, "value": "Core cutover"}, {"field_id": 901, "value": "Engine call cutovers"}]})
                )
            )]
        );
    }

    #[test]
    fn mirror_writes_nothing_when_values_already_match_and_clears_a_removed_feature() {
        let api = Recorder::new(vec![(
            "GET repos/o/r/issues/7/issue-field-values",
            Ok(json!([
              {"issue_field_id": 900, "issue_field_name": "Epic", "data_type": "single_select", "value": "Core cutover", "single_select_option": {"id": 11, "name": "Core cutover", "color": "blue"}},
              {"issue_field_id": 901, "issue_field_name": "feature", "data_type": "text", "value": "Old feature"}])),
        )]);
        mirror(&api, "o/r", "7", &fields(), Some("Core cutover"), None).unwrap();
        assert_eq!(
            api.writes(),
            [(
                "DELETE".to_string(),
                "repos/o/r/issues/7/issue-field-values/901".to_string(),
                None
            )]
        );
    }

    #[test]
    fn a_missing_single_select_option_refuses_naming_the_fix() {
        let api = Recorder::new(vec![]);
        let e = mirror(
            &api,
            "o/r",
            "7",
            &fields(),
            Some("Platform & infrastructure"),
            None,
        )
        .unwrap_err();
        assert_eq!(e.message, "GitHub issue field \"Epic\" has no option \"Platform & infrastructure\"; run story group --sync-fields (organization admin) to add it.");
        assert!(api.writes().is_empty());
    }

    #[test]
    fn mirror_is_a_no_op_without_epic_or_feature_fields() {
        let api = Recorder::new(vec![]);
        let only_priority = vec![fields().remove(0)];
        mirror(
            &api,
            "o/r",
            "7",
            &only_priority,
            Some("Core cutover"),
            Some("X"),
        )
        .unwrap();
        assert!(api.calls.borrow().is_empty());
    }

    #[test]
    fn sync_creates_missing_fields_and_appends_only_missing_options_keeping_ids() {
        let api = Recorder::new(vec![(
            "GET orgs/o/issue-fields",
            Ok(json!([
          {"id": 900, "name": "Epic", "data_type": "single_select", "options": [
             {"id": 11, "name": "Core cutover", "color": "green", "priority": 1},
             {"id": 99, "name": "Team's own", "color": "gray", "priority": 2}]}])),
        )]);
        let epics = vec!["Core cutover".to_string(), "Recruit portal".to_string()];
        let features = vec![
            "Engine call cutovers".to_string(),
            "Prospects & private notes".to_string(),
        ];
        let changes = sync(&api, "o", &epics, &features).unwrap();
        assert_eq!(
            changes,
            [
                Change::Extended {
                    field: "Epic".into(),
                    added: vec!["Recruit portal".into()]
                },
                Change::Created {
                    field: "Feature".into(),
                    options: features.clone()
                },
            ]
        );
        let writes = api.writes();
        assert_eq!(writes[0].0, "PATCH");
        assert_eq!(writes[0].1, "orgs/o/issue-fields/900");
        assert_eq!(
            writes[0].2,
            Some(json!({"options": [
            {"id": 11, "name": "Core cutover", "color": "green", "priority": 1},
            {"id": 99, "name": "Team's own", "color": "gray", "priority": 2},
            {"name": "Recruit portal", "color": "blue", "priority": 3}]}))
        );
        assert_eq!(writes[1].0, "POST");
        assert_eq!(writes[1].1, "orgs/o/issue-fields");
        assert_eq!(
            writes[1].2,
            Some(json!({
            "name": "Feature", "data_type": "single_select",
            "description": "The feature this issue builds, from the story-time taxonomy",
            "options": [
              {"name": "Engine call cutovers", "color": "purple", "priority": 1},
              {"name": "Prospects & private notes", "color": "purple", "priority": 2}]}))
        );
    }

    #[test]
    fn sync_twice_changes_nothing_and_a_text_feature_field_is_left_as_text() {
        let api = Recorder::new(vec![("GET orgs/o/issue-fields", Ok(org_listing()))]);
        let changes = sync(
            &api,
            "o",
            &["Recruit portal".into(), "core cutover".into()],
            &["Anything".into()],
        )
        .unwrap();
        assert_eq!(
            changes,
            [
                Change::Unchanged {
                    field: "Epic".into()
                },
                Change::Unchanged {
                    field: "feature".into()
                }
            ]
        );
        assert!(api.writes().is_empty());
    }
}
