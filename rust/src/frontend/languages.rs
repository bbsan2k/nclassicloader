use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn valid_locale(locale: &str) -> bool {
    !locale.is_empty()
        && locale.len() <= 24
        && locale
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

pub fn default_table(document: &Value) -> (String, Value) {
    let strings = document
        .get("strings")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let locale = document
        .get("locale")
        .and_then(Value::as_str)
        .unwrap_or("usa_en")
        .to_owned();
    (locale, strings)
}

pub fn decode_index(document: &Value) -> Result<Vec<String>, String> {
    let locales = document
        .get("locales")
        .and_then(Value::as_array)
        .ok_or("Invalid language index")?;
    if locales.len() > 64 {
        return Err("Too many language tables".into());
    }
    locales
        .iter()
        .map(|locale| {
            locale
                .as_str()
                .filter(|locale| valid_locale(locale))
                .map(str::to_owned)
                .ok_or_else(|| "Invalid language identifier".into())
        })
        .collect()
}

pub fn decode_table(document: &Value) -> Result<Value, String> {
    let strings = document
        .get("strings")
        .and_then(Value::as_object)
        .ok_or("Invalid language table")?;
    if strings.len() >= 5000 || !strings.values().all(Value::is_string) {
        return Err("Invalid language strings".into());
    }
    Ok(Value::Object(strings.clone()))
}

pub fn register_default(
    languages: &mut BTreeMap<String, Value>,
    locale: String,
    default_strings: &Value,
) {
    languages
        .entry(locale)
        .or_insert_with(|| default_strings.clone());
}

pub fn translate(selected: &Value, default: &Value, key: &str, fallback: &str) -> String {
    selected
        .get(key)
        .and_then(Value::as_str)
        .or_else(|| default.get(key).and_then(Value::as_str))
        .unwrap_or(fallback)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        decode_index, decode_table, default_table, register_default, translate, valid_locale,
    };
    use serde_json::{Map, Value, json};
    use std::collections::BTreeMap;

    #[test]
    fn legacy_default_table_and_locale_fallbacks_are_preserved() {
        assert_eq!(default_table(&json!({})), ("usa_en".into(), json!({})));
        assert_eq!(
            default_table(&json!({"strings":[1],"locale":false})),
            ("usa_en".into(), json!([1]))
        );
        assert_eq!(
            default_table(&json!({"strings":{"x":"y"},"locale":"LEGACY"})),
            ("LEGACY".into(), json!({"x":"y"}))
        );
    }

    #[test]
    fn locale_validation_is_ascii_lowercase_or_underscore_and_limited_by_bytes() {
        assert!(valid_locale("a"));
        assert!(valid_locale(&"a".repeat(24)));
        for invalid in ["", &"a".repeat(25), "é", "a-b", "a.b", "a/b", "A"] {
            assert!(!valid_locale(invalid), "unexpectedly accepted {invalid:?}");
        }
    }

    #[test]
    fn language_index_validates_shape_count_and_items_without_deduplicating() {
        for invalid in [json!(null), json!({}), json!({"locales":"usa_en"})] {
            assert_eq!(
                decode_index(&invalid).unwrap_err(),
                "Invalid language index"
            );
        }
        assert_eq!(
            decode_index(&json!({"locales":["usa_en",3]})).unwrap_err(),
            "Invalid language identifier"
        );
        assert_eq!(
            decode_index(&json!({"locales":["unsafe-locale"]})).unwrap_err(),
            "Invalid language identifier"
        );

        for count in [8, 9] {
            let locales =
                (0..count).map(|index| format!("locale_{}", (b'a' + index as u8) as char));
            let document = json!({"locales":locales.collect::<Vec<_>>()});
            assert_eq!(decode_index(&document).unwrap().len(), count);
        }
        let mut exact = vec![json!("locale"); 64];
        exact[1] = json!("locale");
        assert_eq!(decode_index(&json!({"locales":exact})).unwrap().len(), 64);
        assert_eq!(
            decode_index(&json!({"locales":vec!["a"; 65]})).unwrap_err(),
            "Too many language tables"
        );
        assert_eq!(
            decode_index(&json!({"locales":["b","a","b"]})).unwrap(),
            ["b", "a", "b"]
        );
    }

    #[test]
    fn table_validation_preserves_exact_limits_and_unicode_strings() {
        for invalid in [json!(null), json!({}), json!({"strings":[]} )] {
            assert_eq!(
                decode_table(&invalid).unwrap_err(),
                "Invalid language table"
            );
        }
        assert_eq!(
            decode_table(&json!({"strings":{"x":1}})).unwrap_err(),
            "Invalid language strings"
        );
        let accepted: serde_json::Map<String, Value> = (0..4999)
            .map(|index| (format!("k{index}"), json!("値")))
            .collect();
        let decoded = decode_table(&json!({"strings":accepted})).unwrap();
        assert_eq!(decoded.get("k0").and_then(Value::as_str), Some("値"));
        let rejected: Map<String, Value> = (0..5000)
            .map(|index| (format!("k{index}"), json!("value")))
            .collect();
        assert_eq!(
            decode_table(&json!({"strings":rejected})).unwrap_err(),
            "Invalid language strings"
        );
    }

    #[test]
    fn translation_falls_back_per_key_and_keeps_empty_strings() {
        let selected = json!({"translated":"選択","empty":"","wrong":false});
        let default = json!({"missing-selection":"標準","wrong":"標準型"});
        assert_eq!(
            translate(&selected, &default, "translated", "caller"),
            "選択"
        );
        assert_eq!(
            translate(&selected, &default, "missing-selection", "caller"),
            "標準"
        );
        assert_eq!(translate(&selected, &default, "wrong", "caller"), "標準型");
        assert_eq!(translate(&selected, &default, "empty", "caller"), "");
        assert_eq!(translate(&selected, &default, "absent", "caller"), "caller");
    }

    #[test]
    fn default_registration_does_not_replace_a_loaded_default_table() {
        let mut languages = BTreeMap::from([("usa_en".into(), json!({"shipped":"table"}))]);
        register_default(
            &mut languages,
            "usa_en".into(),
            &json!({"fallback":"table"}),
        );
        assert_eq!(languages["usa_en"], json!({"shipped":"table"}));
        register_default(
            &mut languages,
            "legacy".into(),
            &json!({"fallback":"table"}),
        );
        assert_eq!(languages["legacy"], json!({"fallback":"table"}));
    }
}
