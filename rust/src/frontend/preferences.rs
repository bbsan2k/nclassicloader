//! Pure preference document policy; persistence remains in the host UI.
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSettings {
    pub selected: Option<String>,
    pub sort: usize,
    pub slot: usize,
    pub sound: bool,
    pub motion: bool,
    pub language: Option<String>,
    pub firmware: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuSnapshot {
    pub selected: String,
    pub sort: usize,
    pub slot: usize,
    pub sound: bool,
    pub motion: bool,
    pub language: String,
    pub firmware: String,
}

impl Default for MenuSettings {
    fn default() -> Self {
        Self {
            selected: None,
            sort: 0,
            slot: 0,
            sound: false,
            motion: true,
            language: None,
            firmware: None,
        }
    }
}

pub fn decode(value: &Value) -> Result<MenuSettings, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "Preferences must be a JSON object".to_owned())?;
    Ok(MenuSettings {
        selected: object
            .get("selected")
            .and_then(Value::as_str)
            .map(str::to_owned),
        sort: object
            .get("sort")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(5) as usize,
        slot: object
            .get("slot")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(3) as usize,
        sound: object
            .get("sound")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        motion: object
            .get("motion")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        language: object
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned),
        firmware: object
            .get("firmware")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

pub fn merge_snapshot(value: &Value, settings: &MenuSnapshot) -> Result<Value, String> {
    let mut map: Map<String, Value> = value
        .as_object()
        .cloned()
        .ok_or_else(|| "Preferences must be a JSON object".to_owned())?;
    map.insert("schema".into(), json!(1));
    map.insert("selected".into(), json!(settings.selected));
    map.insert("sort".into(), json!(settings.sort));
    map.insert("slot".into(), json!(settings.slot));
    map.insert("sound".into(), json!(settings.sound));
    map.insert("motion".into(), json!(settings.motion));
    map.insert("language".into(), json!(settings.language));
    map.insert("firmware".into(), json!(settings.firmware));
    Ok(Value::Object(map))
}

pub fn decode_filter(value: Option<&str>) -> crate::frontend::library::LibraryFilter {
    crate::frontend::library::LibraryFilter::from_label(value)
}
pub fn encode_filter(filter: crate::frontend::library::LibraryFilter) -> &'static str {
    filter.label()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_bad_roots() {
        assert_eq!(decode(&json!({})).unwrap(), MenuSettings::default());
        assert!(decode(&json!([])).is_err());
        assert!(decode(&json!(false)).is_err());
        for root in [json!([]), json!(false), json!(null)] {
            assert!(decode(&root).is_err());
            assert!(merge_snapshot(&root, &snapshot()).is_err());
        }
    }

    fn snapshot() -> MenuSnapshot {
        MenuSnapshot {
            selected: "id".into(),
            sort: 5,
            slot: 3,
            sound: true,
            motion: false,
            language: "en".into(),
            firmware: "/fw".into(),
        }
    }
    #[test]
    fn bounds_and_preservation() {
        let original = json!({"history":[1],"suspendLocks":{},"roms":{},"displayMode":"wide","displayFrame":2,"extension":true,"sort":-1});
        let _settings = decode(&original).unwrap();
        let merged = merge_snapshot(&original, &snapshot()).unwrap();
        for key in [
            "history",
            "suspendLocks",
            "roms",
            "displayMode",
            "displayFrame",
            "extension",
        ] {
            assert_eq!(merged[key], original[key]);
        }
        assert_eq!(merged["schema"], 1);
        assert_eq!(
            decode(&merged).unwrap(),
            MenuSettings {
                selected: Some("id".into()),
                sort: 5,
                slot: 3,
                sound: true,
                motion: false,
                language: Some("en".into()),
                firmware: Some("/fw".into())
            }
        );
        for value in [json!(-1), json!(1.5), json!("2")] {
            assert_eq!(decode(&json!({"sort":value,"slot":value})).unwrap().sort, 0);
            assert_eq!(decode(&json!({"sort":value,"slot":value})).unwrap().slot, 0);
        }
        assert_eq!(
            decode(&json!({"sort":u64::MAX,"slot":u64::MAX}))
                .unwrap()
                .sort,
            5
        );
        assert_eq!(decode(&json!({"sort":99,"slot":3})).unwrap().sort, 5);
        assert!(!decode(&json!({"sound":"yes","motion":0})).unwrap().sound);
        assert!(decode(&json!({"sound":"yes","motion":0})).unwrap().motion);
        assert_eq!(
            decode(&json!({"selected":"","language":"","firmware":""}))
                .unwrap()
                .selected,
            Some(String::new())
        );
        assert_eq!(decode(&json!({})).unwrap().language, None);
        let first = merge_snapshot(&json!({"history":[1]}), &snapshot()).unwrap();
        let second = merge_snapshot(&json!({"extension":"second"}), &snapshot()).unwrap();
        assert_eq!(first["history"], json!([1]));
        assert_eq!(second["extension"], "second");
        use crate::frontend::library::LibraryFilter::*;
        for (label, filter) in [("ALL", All), ("NES", Nes), ("SNES", Snes)] {
            assert_eq!(encode_filter(decode_filter(Some(label))), label);
            assert_eq!(decode_filter(Some(label)), filter);
        }
        for unknown in ["nes", "UNKNOWN", "", "wat"] {
            assert_eq!(decode_filter(Some(unknown)), All);
        }
    }
}
