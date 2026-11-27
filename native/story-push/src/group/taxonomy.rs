//! The team's taxonomy, `.story-time/taxonomy.json`: named epics, each with named features. Pure.

use serde_json::Value;
use std::collections::HashSet;

pub(crate) const MAX_EPICS: usize = 15;
pub(crate) const MAX_FEATURES: usize = 15;
pub(crate) const MAX_NAME: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Feature {
    pub(crate) name: String,
    pub(crate) description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Epic {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) features: Vec<Feature>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Taxonomy {
    pub(crate) epics: Vec<Epic>,
}

/// The option key Jev answers with when no listed epic or feature fits.
pub(crate) const NONE_FIT: &str = "none_fit";

fn fold(name: &str) -> String {
    name.trim().to_lowercase()
}

/// `name` and `description` of one taxonomy entry, checked.
fn entry(value: &Value, kind: &str, position: usize) -> Result<(String, String), String> {
    let name = value["name"].as_str().map(str::trim).unwrap_or("");
    if name.is_empty() {
        return Err(format!("taxonomy: {kind} {position} has an empty name"));
    }
    if fold(name) == NONE_FIT {
        return Err(format!("taxonomy: \"{NONE_FIT}\" is reserved"));
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!(
            "taxonomy: {kind} \"{name}\" is longer than {MAX_NAME} characters"
        ));
    }
    let description = value["description"].as_str().map(str::trim).unwrap_or("");
    if description.is_empty() {
        return Err(format!("taxonomy: {kind} \"{name}\" needs a description"));
    }
    Ok((name.to_owned(), description.to_owned()))
}

impl Taxonomy {
    /// Parses and validates the taxonomy file. Epic names and feature names are each unique
    /// (case-folded) across the whole file, because each family shares one GitHub field.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let raw: Value =
            serde_json::from_str(text).map_err(|e| format!("taxonomy: invalid JSON ({e})"))?;
        let list = raw["epics"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        if list.is_empty() {
            return Err(format!("taxonomy: needs 1 to {MAX_EPICS} epics"));
        }
        if list.len() > MAX_EPICS {
            return Err(format!(
                "taxonomy: {} epics; at most {MAX_EPICS} fit one Jev request",
                list.len()
            ));
        }
        let (mut epic_names, mut feature_names) = (HashSet::new(), HashSet::new());
        let mut epics = Vec::with_capacity(list.len());
        for (i, value) in list.iter().enumerate() {
            let (name, description) = entry(value, "epic", i + 1)?;
            if !epic_names.insert(fold(&name)) {
                return Err(format!(
                    "taxonomy: epic \"{}\" is listed twice",
                    fold(&name)
                ));
            }
            let raw_features = value["features"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if raw_features.is_empty() || raw_features.len() > MAX_FEATURES {
                return Err(format!(
                    "taxonomy: epic \"{name}\" needs 1 to {MAX_FEATURES} features"
                ));
            }
            let mut features = Vec::with_capacity(raw_features.len());
            for (j, value) in raw_features.iter().enumerate() {
                let (fname, fdescription) = entry(value, "feature", j + 1)?;
                if !feature_names.insert(fold(&fname)) {
                    return Err(format!("taxonomy: feature \"{}\" is listed twice; feature names share one GitHub field, so they must be unique", fold(&fname)));
                }
                features.push(Feature {
                    name: fname,
                    description: fdescription,
                });
            }
            epics.push(Epic {
                name,
                description,
                features,
            });
        }
        Ok(Self { epics })
    }

    /// The epic with this name, compared case-folded.
    pub(crate) fn epic(&self, name: &str) -> Option<&Epic> {
        self.epics.iter().find(|e| fold(&e.name) == fold(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"{"epics":[
      {"name":"Core cutover","description":"Move engine callers onto Core","features":[
        {"name":"Engine call cutovers","description":"Route one engine module through Core"},
        {"name":"Bypass retirement","description":"Remove legacy credentials and paths"}]},
      {"name":"Recruit portal","description":"Coaches manage prospects","features":[
        {"name":"Prospects & private notes","description":"Program-scoped prospects"}]}]}"#;

    #[test]
    fn a_valid_taxonomy_keeps_names_descriptions_and_order() {
        let t = Taxonomy::parse(VALID).unwrap();
        assert_eq!(t.epics.len(), 2);
        assert_eq!(t.epics[0].name, "Core cutover");
        assert_eq!(t.epics[0].features[1].name, "Bypass retirement");
        assert_eq!(
            t.epics[1].features[0].description,
            "Program-scoped prospects"
        );
        assert_eq!(t.epic("recruit PORTAL").unwrap().name, "Recruit portal");
        assert!(t.epic("Behind the scenes").is_none());
    }

    fn refused(text: &str) -> String {
        Taxonomy::parse(text).unwrap_err()
    }

    #[test]
    fn duplicate_epic_names_are_refused_case_folded() {
        let e = refused(
            r#"{"epics":[{"name":"Infra","description":"a","features":[{"name":"A","description":"a"}]},{"name":" infra ","description":"b","features":[{"name":"B","description":"b"}]}]}"#,
        );
        assert_eq!(e, "taxonomy: epic \"infra\" is listed twice");
    }

    #[test]
    fn a_feature_name_shared_by_two_epics_is_refused() {
        let e = refused(
            r#"{"epics":[{"name":"A","description":"a","features":[{"name":"Same","description":"a"}]},{"name":"B","description":"b","features":[{"name":"same","description":"b"}]}]}"#,
        );
        assert_eq!(e, "taxonomy: feature \"same\" is listed twice; feature names share one GitHub field, so they must be unique");
    }

    #[test]
    fn too_many_epics_are_refused() {
        let epics: Vec<String> = (0..16)
            .map(|i| format!(r#"{{"name":"E{i}","description":"d","features":[{{"name":"F{i}","description":"d"}}]}}"#))
            .collect();
        let e = refused(&format!(r#"{{"epics":[{}]}}"#, epics.join(",")));
        assert_eq!(e, "taxonomy: 16 epics; at most 15 fit one Jev request");
    }

    #[test]
    fn empty_long_and_missing_names_are_refused() {
        assert_eq!(
            refused(
                r#"{"epics":[{"name":"  ","description":"d","features":[{"name":"F","description":"d"}]}]}"#
            ),
            "taxonomy: epic 1 has an empty name"
        );
        let long = "x".repeat(51);
        assert_eq!(
            refused(&format!(
                r#"{{"epics":[{{"name":"{long}","description":"d","features":[{{"name":"F","description":"d"}}]}}]}}"#
            )),
            format!("taxonomy: epic \"{long}\" is longer than 50 characters")
        );
        assert_eq!(
            refused(r#"{"epics":[{"name":"E","description":"d","features":[]}]}"#),
            "taxonomy: epic \"E\" needs 1 to 15 features"
        );
        assert_eq!(refused(r#"{"epics":[]}"#), "taxonomy: needs 1 to 15 epics");
        assert_eq!(refused("nope").split(':').next(), Some("taxonomy"));
        assert_eq!(
            refused(r#"{"epics":[{"name":"E","features":[{"name":"F","description":"d"}]}]}"#),
            "taxonomy: epic \"E\" needs a description"
        );
        assert_eq!(
            refused(
                r#"{"epics":[{"name":"none_fit","description":"d","features":[{"name":"F","description":"d"}]}]}"#
            ),
            "taxonomy: \"none_fit\" is reserved"
        );
    }
}
