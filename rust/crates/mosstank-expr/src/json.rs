//! The JSON shape expression values are saved in (persistent and global
//! variables), written byte for byte as System.Text.Json writes the C#'s
//! `StoredValue` (indented, named NaN/Infinity, every property present).

use mosstank_dotnet::compat::json::{JsonNumber, JsonValue, system_text_json};
use openac_plugin_compat::text::casing;

use crate::error::{ExpressionError, Fault};
use crate::value::{
    ChangeCounter, ExpressionCoordinates, ExpressionDictionary, ExpressionList, ExpressionValue,
    ObjectNames,
};

/// Refuses a value that cannot be saved, worded as the reference refuses
/// it: a stopwatch or a UI control, or a collection holding one.
///
/// # Errors
/// The value holds something that cannot be saved.
pub fn require_savable(value: &ExpressionValue) -> Result<(), ExpressionError> {
    store(value).map(|_| ())
}

fn stored(
    kind: &str,
    number: f64,
    text: &str,
    list: JsonValue,
    dictionary: JsonValue,
    coordinates: JsonValue,
) -> JsonValue {
    JsonValue::object([
        ("Kind", JsonValue::String(kind.to_owned())),
        ("Number", JsonValue::Number(JsonNumber::F64(number))),
        ("Text", JsonValue::String(text.to_owned())),
        ("List", list),
        ("Dictionary", dictionary),
        ("Coordinates", coordinates),
    ])
}

/// The saved form of a value.
///
/// # Errors
/// A stopwatch or a UI control (also inside a collection).
pub fn store(value: &ExpressionValue) -> Result<JsonValue, ExpressionError> {
    let null = JsonValue::Null;
    Ok(match value {
        ExpressionValue::Number(number) => {
            stored("number", *number, "", null.clone(), null.clone(), null)
        }
        ExpressionValue::Boolean(truth) => stored(
            "boolean",
            if *truth { 1.0 } else { 0.0 },
            "",
            null.clone(),
            null.clone(),
            null,
        ),
        ExpressionValue::String(text) => {
            stored("string", 0.0, text, null.clone(), null.clone(), null)
        }
        ExpressionValue::List(list) => {
            let items = list
                .items()
                .iter()
                .map(store)
                .collect::<Result<Vec<_>, _>>()?;
            stored("list", 0.0, "", JsonValue::Array(items), null.clone(), null)
        }
        ExpressionValue::Dictionary(dictionary) => {
            let mut members = Vec::new();
            for (key, item) in dictionary.entries() {
                members.push((key, store(&item)?));
            }
            stored(
                "dictionary",
                0.0,
                "",
                null.clone(),
                JsonValue::Object(members),
                null,
            )
        }
        ExpressionValue::Coordinates(coordinates) => stored(
            "coordinates",
            0.0,
            "",
            null.clone(),
            null,
            JsonValue::array([
                JsonValue::Number(JsonNumber::F64(coordinates.east_west)),
                JsonValue::Number(JsonNumber::F64(coordinates.north_south)),
                JsonValue::Number(JsonNumber::F64(coordinates.elevation)),
            ]),
        ),
        ExpressionValue::WorldObject(object) => stored(
            "worldobject",
            f64::from(object.id),
            "",
            null.clone(),
            null.clone(),
            null,
        ),
        ExpressionValue::Stopwatch(_) | ExpressionValue::UiControl(_) => {
            let name = if matches!(value, ExpressionValue::UiControl(_)) {
                "UIControl"
            } else {
                "Stopwatch"
            };
            return Err(ExpressionError::evaluation(format!(
                "There is currently no support for serializing {name} types"
            )));
        }
    })
}

/// The JSON writer settings the C# store uses.
pub const OPTIONS: system_text_json::Options = system_text_json::Options {
    write_indented: true,
    named_floating_point_literals: true,
    new_line: "\r\n",
};

/// A value as saved text.
///
/// # Errors
/// A value that cannot be saved.
pub fn serialize(value: &ExpressionValue) -> Result<String, ExpressionError> {
    let json = store(value)?;
    system_text_json::to_string(&json, &OPTIONS)
        .map_err(|error| ExpressionError::evaluation(error.to_string()))
}

fn number_of(value: Option<&JsonValue>) -> Result<f64, String> {
    match value {
        None | Some(JsonValue::Null) => Ok(0.0),
        Some(JsonValue::Number(number)) => Ok(number.to_f64()),
        Some(JsonValue::String(text)) => match text.as_str() {
            "NaN" => Ok(f64::NAN),
            "Infinity" => Ok(f64::INFINITY),
            "-Infinity" => Ok(f64::NEG_INFINITY),
            _ => Err("The JSON value could not be converted to System.Double.".to_owned()),
        },
        Some(_) => Err("The JSON value could not be converted to System.Double.".to_owned()),
    }
}

/// Rebuilds a saved value. A part a saved value always has that the JSON
/// left null is damage; so is a world object id out of range.
///
/// # Errors
/// What is damaged.
pub fn restore(
    value: &JsonValue,
    names: Option<&ObjectNames>,
    counter: &ChangeCounter,
) -> Result<ExpressionValue, String> {
    match value {
        JsonValue::Object(_) => {}
        // A list item or dictionary entry saved as null has no value.
        JsonValue::Null => return Err("a saved value has no kind".to_owned()),
        _ => {
            return Err(
                "The JSON value could not be converted to ExpressionValueJson+StoredValue."
                    .to_owned(),
            );
        }
    }
    let kind = match value.get_ignore_case("Kind") {
        None => "number".to_owned(),
        Some(JsonValue::String(kind)) => kind.clone(),
        Some(_) => return Err("a saved value has no kind".to_owned()),
    };
    let number = number_of(value.get_ignore_case("Number"))?;
    Ok(match casing::to_lower_invariant(&kind).as_str() {
        "number" => ExpressionValue::Number(number),
        "boolean" => ExpressionValue::Boolean(number != 0.0),
        "string" => match value.get_ignore_case("Text") {
            None => ExpressionValue::string(""),
            Some(JsonValue::String(text)) => ExpressionValue::string(text),
            Some(_) => return Err("a saved string has no text".to_owned()),
        },
        "list" => {
            let mut items = Vec::new();
            if let Some(JsonValue::Array(list)) = value.get_ignore_case("List") {
                for item in list {
                    items.push(restore(item, names, counter)?);
                }
            }
            ExpressionValue::List(ExpressionList::new(items, counter))
        }
        "dictionary" => {
            let dictionary = ExpressionDictionary::new(counter);
            if let Some(JsonValue::Object(members)) = value.get_ignore_case("Dictionary") {
                for (key, item) in members {
                    let item = restore(item, names, counter)?;
                    dictionary.with_items(|items| items.insert(key, item));
                }
            }
            ExpressionValue::Dictionary(dictionary)
        }
        "coordinates" => match value.get_ignore_case("Coordinates") {
            Some(JsonValue::Array(values)) if values.len() >= 2 => {
                let at = |index: usize| number_of(values.get(index));
                ExpressionValue::coordinates(ExpressionCoordinates::new(
                    at(0)?,
                    at(1)?,
                    if values.len() >= 3 { at(2)? } else { 0.0 },
                ))
            }
            _ => ExpressionValue::zero(),
        },
        "worldobject" => {
            let truncated = number.trunc();
            if !(0.0..=f64::from(u32::MAX)).contains(&truncated) {
                return Err(Fault::overflow().message());
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            ExpressionValue::world_object(truncated as u32, names.cloned())
        }
        _ => ExpressionValue::zero(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_saved_with_every_property() {
        assert_eq!(
            serialize(&ExpressionValue::Number(5.0)).unwrap(),
            "{\r\n  \"Kind\": \"number\",\r\n  \"Number\": 5,\r\n  \"Text\": \"\",\r\n  \"List\": null,\r\n  \"Dictionary\": null,\r\n  \"Coordinates\": null\r\n}"
        );
    }

    #[test]
    fn a_stopwatch_cannot_be_saved() {
        let stopwatch = crate::value::ExpressionStopwatch::new(std::rc::Rc::new(|| 0));
        let error = serialize(&ExpressionValue::Stopwatch(stopwatch)).unwrap_err();
        assert_eq!(
            error.message(),
            "There is currently no support for serializing Stopwatch types"
        );
    }

    #[test]
    fn values_come_back_as_saved() {
        let counter = ChangeCounter::new();
        let list = ExpressionValue::List(ExpressionList::new(
            vec![
                ExpressionValue::string("a"),
                ExpressionValue::Number(f64::NAN),
            ],
            &counter,
        ));
        let text = serialize(&list).unwrap();
        let json = mosstank_dotnet::compat::json::parse(&text).unwrap();
        let back = restore(&json, None, &counter).unwrap();
        assert_eq!(back.to_display_string(), "[a,NaN]");
    }
}
