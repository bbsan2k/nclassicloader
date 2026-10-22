use serde_json::Value;
use std::collections::HashSet;

pub const MAX_JSON_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub enum DecodeError {
    TooLarge,
    InvalidJson(serde_json::Error),
    Catalog(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub sort_title: String,
    pub publisher: String,
    pub release: String,
    pub copyright: String,
    pub rom: String,
    pub import_error: String,
    pub players: u32,
    pub simultaneous: bool,
    pub firmware_required: Vec<String>,
}

pub fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() < 100
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

pub fn decode(catalog: &Value) -> Result<Vec<Game>, String> {
    if catalog.get("schema").and_then(Value::as_u64) != Some(1) {
        return Err("Unsupported catalog schema".into());
    }
    let entries = catalog
        .get("games")
        .and_then(Value::as_array)
        .ok_or("Catalog has no games")?;
    let mut ids = HashSet::new();
    let mut games = Vec::new();
    for item in entries {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Catalog game has no id")?
            .to_owned();
        if !safe_id(&id) || !ids.insert(id.clone()) {
            return Err("Invalid or duplicate catalog game ID".into());
        }
        let string = |key: &str, fallback: &str| {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or(fallback)
                .to_owned()
        };
        games.push(Game {
            title: string("title", &id),
            sort_title: string("sortTitle", &string("title", &id)),
            publisher: string("publisher", ""),
            release: string("releaseDate", ""),
            copyright: string("copyright", ""),
            rom: string("rom", ""),
            import_error: string("importError", ""),
            players: item.get("players").and_then(Value::as_u64).unwrap_or(1) as u32,
            simultaneous: item
                .get("simultaneous")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            firmware_required: item
                .get("firmwareRequired")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            id,
        });
    }
    if games.is_empty() {
        return Err("Empty game catalog".into());
    }
    Ok(games)
}

pub fn decode_bytes(bytes: &[u8]) -> Result<Vec<Game>, DecodeError> {
    if bytes.len() > MAX_JSON_BYTES {
        return Err(DecodeError::TooLarge);
    }
    let catalog = serde_json::from_slice(bytes).map_err(DecodeError::InvalidJson)?;
    decode(&catalog).map_err(DecodeError::Catalog)
}

#[cfg(test)]
mod tests {
    use super::{DecodeError, Game, MAX_JSON_BYTES, decode, decode_bytes, safe_id};
    use serde_json::{Value, json};

    #[test]
    fn decodes_all_fields_in_order_and_preserves_cast_and_firmware_semantics() {
        let value = json!({"schema":1,"games":[
            {"id":"second","title":"Title","sortTitle":"Sort","publisher":"Pub","releaseDate":"Date","copyright":"Copy","rom":"rom.sfc","importError":"bad","players":4294967297_u64,"simultaneous":true,"firmwareRequired":["one","ignored",3,"two",null]},
            {"id":"first","players":0,"firmwareRequired":"not-an-array"}
        ]});
        let decoded = decode(&value).unwrap();
        assert_eq!(
            decoded
                .iter()
                .map(|game| game.id.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert_eq!(
            decoded[0],
            Game {
                id: "second".into(),
                title: "Title".into(),
                sort_title: "Sort".into(),
                publisher: "Pub".into(),
                release: "Date".into(),
                copyright: "Copy".into(),
                rom: "rom.sfc".into(),
                import_error: "bad".into(),
                players: 1,
                simultaneous: true,
                firmware_required: vec!["one".into(), "ignored".into(), "two".into()],
            }
        );
        assert_eq!(
            decoded[1],
            Game {
                id: "first".into(),
                title: "first".into(),
                sort_title: "first".into(),
                publisher: "".into(),
                release: "".into(),
                copyright: "".into(),
                rom: "".into(),
                import_error: "".into(),
                players: 0,
                simultaneous: false,
                firmware_required: vec![],
            }
        );
    }

    #[test]
    fn title_and_sort_title_fallbacks_require_strings() {
        let games = decode(&json!({"schema":1,"games":[
            {"id":"fallback","title":false,"sortTitle":2},
            {"id":"title","title":"Hello"},
            {"id":"sort","title":"Hello","sortTitle":" A "}
        ]}))
        .unwrap();
        assert_eq!(
            (games[0].title.as_str(), games[0].sort_title.as_str()),
            ("fallback", "fallback")
        );
        assert_eq!(
            (games[1].title.as_str(), games[1].sort_title.as_str()),
            ("Hello", "Hello")
        );
        assert_eq!(games[2].sort_title, " A ");
    }

    #[test]
    fn reports_exact_catalog_errors() {
        let cases: [(Value, &str); 9] = [
            (json!(null), "Unsupported catalog schema"),
            (json!({"schema":2,"games":[]}), "Unsupported catalog schema"),
            (json!({"schema":1}), "Catalog has no games"),
            (json!({"schema":1,"games":{}}), "Catalog has no games"),
            (json!({"schema":1,"games":[]}), "Empty game catalog"),
            (json!({"schema":1,"games":[{}]}), "Catalog game has no id"),
            (
                json!({"schema":1,"games":[{"id":3}]}),
                "Catalog game has no id",
            ),
            (
                json!({"schema":1,"games":[{"id":"bad_id"}]}),
                "Invalid or duplicate catalog game ID",
            ),
            (
                json!({"schema":1,"games":[{"id":"same"},{"id":"same"}]}),
                "Invalid or duplicate catalog game ID",
            ),
        ];
        for (value, error) in cases {
            assert_eq!(decode(&value).unwrap_err(), error);
        }
    }

    #[test]
    fn safe_id_accepts_only_nonempty_ascii_alphanumeric_and_dash_below_100_bytes() {
        assert!(safe_id(&"a".repeat(99)));
        assert!(!safe_id(&"a".repeat(100)));
        for invalid in ["", "é", "a_b", "a.b", "a/b", "a b", "a\\b"] {
            assert!(!safe_id(invalid), "unexpectedly accepted {invalid:?}");
        }
        assert!(safe_id("A0-z"));
    }

    #[test]
    fn decode_bytes_matches_value_decoder() {
        let value = json!({"schema":1,"games":[{"id":"byte-game","title":"Byte Game"}]});
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(decode_bytes(&bytes).unwrap(), decode(&value).unwrap());
        assert!(matches!(
            decode_bytes(br#"{"schema":1,"games":[{"id":"bad_id"}]}"#),
            Err(DecodeError::Catalog(error)) if error == "Invalid or duplicate catalog game ID"
        ));
    }

    #[test]
    fn decode_bytes_rejects_invalid_utf8_json_and_deep_nesting() {
        assert!(matches!(
            decode_bytes(b"\xff"),
            Err(DecodeError::InvalidJson(_))
        ));
        assert!(matches!(
            decode_bytes(b"{not-json}"),
            Err(DecodeError::InvalidJson(_))
        ));
        let nested = format!("{}0{}", "[".repeat(256), "]".repeat(256));
        assert!(matches!(
            decode_bytes(nested.as_bytes()),
            Err(DecodeError::InvalidJson(_))
        ));
    }

    #[test]
    fn decode_bytes_enforces_inclusive_size_limit_before_parsing() {
        let valid = br#"{"schema":1,"games":[{"id":"limit"}]}"#;
        let mut exact = valid.to_vec();
        exact.resize(MAX_JSON_BYTES, b' ');
        assert_eq!(decode_bytes(&exact).unwrap()[0].id, "limit");

        let mut oversized_whitespace = valid.to_vec();
        oversized_whitespace.resize(MAX_JSON_BYTES + 1, b' ');
        assert!(matches!(
            decode_bytes(&oversized_whitespace),
            Err(DecodeError::TooLarge)
        ));
    }
}
