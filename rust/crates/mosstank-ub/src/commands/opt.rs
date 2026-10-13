//! `/ub opt {list | get <option> | set <option> <newValue> | toggle
//! <option>}` and the `uboptget[]`/`uboptset[]` expression functions: the
//! UB settings, and only those (the macro's own options are `/vt opt`'s).

use mosstank_expr::value::ExpressionList;
use mosstank_expr::{ExpressionValue, ExpressionValueKind};
use mosstank_settings::ub::{UbSetting, UbSettingKind, UbSettingScope, UbSettingValue};
use openac_plugin_compat::number::{MidpointRounding, round_mode};
use openac_plugin_compat::text::casing;

use super::call::UbCall;
use super::split_head;
use crate::catalog::kind_name;

/// `/ub opt ...`.
pub fn handle_opt(call: &mut UbCall<'_>, arguments: &str) {
    let (operation, tail) = split_head(arguments.trim());
    let (name, raw_value) = split_head(&tail);
    match casing::to_lower_invariant(&operation).as_str() {
        "list" if tail.is_empty() => {
            // The profile settings before the character's own state.
            let mut rows: Vec<UbSetting> = call.settings.settings().to_vec();
            rows.sort_by_key(|row| row.scope() == UbSettingScope::Character);
            let lines: Vec<String> = rows
                .iter()
                .map(|row| call.settings.full_display_value(row))
                .collect();
            call.write_ub_message("All Settings:", lines);
        }
        "get" if !name.is_empty() && raw_value.is_empty() => {
            if !try_write_setting(call, &name) {
                write_invalid_option(call, &name);
            }
        }
        // A set with nothing after the name is not a set: the reference
        // answers it with its usage.
        "set" if !name.is_empty() && !raw_value.is_empty() => {
            if !try_set_setting(call, &name, &raw_value) {
                write_invalid_option(call, &name);
            }
        }
        "toggle" if !name.is_empty() && raw_value.is_empty() => toggle_setting(call, &name),
        _ => call.write_ub_bad_syntax("opt"),
    }
}

/// A get, set or toggle of a name no setting has: the name in lower case
/// as its error, then one error line per character setting, each "  - "
/// and the setting's name (`WriteUbInvalidOption`).
pub fn write_invalid_option(call: &mut UbCall<'_>, name: &str) {
    call.write_ub_error(&format!(
        "Invalid option: {}",
        casing::to_lower_invariant(name.trim())
    ));
    let names: Vec<String> = call
        .settings
        .settings()
        .iter()
        .filter(|row| row.scope() == UbSettingScope::Character)
        .map(|row| row.name().to_owned())
        .collect();
    for name in names {
        call.write_ub_error(&format!("  - {name}"));
    }
}

fn try_write_setting(call: &mut UbCall<'_>, name: &str) -> bool {
    let Some(row) = call.settings.try_get(name.trim()) else {
        return false;
    };
    let line = call.settings.full_display_value(&row);
    call.write_ub(&line);
    true
}

/// Writes one setting from typed text; false when there is no such row. A
/// row that exists but was given a value of the wrong shape says so and
/// still counts as handled.
fn try_set_setting(call: &mut UbCall<'_>, name: &str, raw_value: &str) -> bool {
    let Some(row) = call.settings.try_get(name.trim()) else {
        return false;
    };
    if row.kind() == UbSettingKind::Collection {
        edit_list(call, &row, raw_value.trim());
        return true;
    }
    let Some(value) = UbSettingValue::try_parse(row.kind(), raw_value.trim()) else {
        call.write_ub_error(&format!(
            "Option set: Invalid value specified. {} is a {}.",
            row.name(),
            kind_name(row.kind())
        ));
        return true;
    };
    call.settings.set(&row, value);
    echo_change(call, &row);
    true
}

/// The line a set or a toggle leaves: an ordinary line while debug is off,
/// a debug line with it on (`EchoUbSettingChange`).
fn echo_change(call: &mut UbCall<'_>, row: &UbSetting) {
    let text = call.settings.full_display_value(row);
    if call.ub_debug() {
        call.write_ub_debug(&text);
    } else {
        call.write_ub(&text);
    }
}

/// A list setting is edited, never replaced, from the command line: `add
/// a[,b]` appends each name it does not already hold, `remove a[,b]` takes
/// each out, and `clear` empties it. Names are compared exactly and taken
/// as written between the commas.
fn edit_list(call: &mut UbCall<'_>, row: &UbSetting, edit: &str) {
    let mut parts = edit.splitn(2, ' ').filter(|part| !part.is_empty());
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    let verb = casing::to_lower_invariant(first);
    let names = second.unwrap_or_default();
    let mut items = call.settings.get(row).items;
    match verb.as_str() {
        "add" => {
            if names.trim().is_empty() {
                call.write_ub_debug("Missing items to add");
                return;
            }
            for name in names.split(',') {
                if !items.iter().any(|item| item == name) {
                    items.push(name.to_owned());
                }
            }
        }
        "remove" => {
            if names.trim().is_empty() {
                call.write_ub_debug("Missing items to remove");
                return;
            }
            for name in names.split(',') {
                if let Some(index) = items.iter().position(|item| item == name) {
                    items.remove(index);
                }
            }
        }
        "clear" => items.clear(),
        _ => {
            // The reference names the word after the verb; a verb on its own
            // is named itself, so the line is never silently ignored.
            call.write_ub_error(&format!("Unknown verb: {}", second.unwrap_or(first)));
            return;
        }
    }
    call.settings
        .set(row, UbSettingValue::from_collection(&items));
    let line = call.settings.full_display_value(row);
    call.write_ub(&line);
}

/// `/ub opt toggle <option>`: flips one of the UB switches.
fn toggle_setting(call: &mut UbCall<'_>, name: &str) {
    let requested = name.trim();
    let Some(row) = call.settings.try_get(requested) else {
        write_invalid_option(call, requested);
        return;
    };
    if row.kind() != UbSettingKind::Bool {
        // The reference's toggle reads the value as a true/false and prints
        // the runtime's own message for the failed read.
        call.write_ub_error(&format!(
            "Unable to toggle setting {}: Specified cast is not valid.",
            row.name()
        ));
        return;
    }
    let flipped = !call.settings.get(&row).boolean;
    call.settings.set(&row, UbSettingValue::from_bool(flipped));
    echo_change(call, &row);
}

/// `uboptget[name]`: a UB setting's value, a list setting as a list of its
/// lines. A name that is not a UB setting reads the macro's own option of
/// that name; a name that is neither is the reference's invalid option: an
/// error and 0.
pub fn ub_option_get(call: &mut UbCall<'_>, name: &str) -> ExpressionValue {
    let Some(row) = call.settings.try_get(name.trim()) else {
        if call.services.is_vtank_option(name) {
            return call.services.get_meta_option(call.host, name);
        }
        call.write_ub_error(&format!(
            "Invalid option: {}",
            casing::to_lower_invariant(name)
        ));
        return ExpressionValue::zero();
    };
    let value = call.settings.get(&row);
    match value.kind {
        UbSettingKind::Bool => ExpressionValue::Boolean(value.boolean),
        UbSettingKind::String => ExpressionValue::string(&value.text),
        UbSettingKind::Collection => {
            let counter = call.expressions.state().changes().clone();
            ExpressionValue::List(ExpressionList::new(
                value
                    .items
                    .iter()
                    .map(|item| ExpressionValue::string(item))
                    .collect(),
                &counter,
            ))
        }
        _ => ExpressionValue::Number(value.number),
    }
}

/// `uboptset[name,value]`: writes a UB setting and says the new value as a
/// typed `opt set` does. A list setting takes a list and is replaced by it.
/// A name that is not a UB setting writes the macro's own option; a name
/// that is neither is the reference's invalid option: an error and false.
pub fn ub_option_set(call: &mut UbCall<'_>, name: &str, value: &ExpressionValue) -> bool {
    let Some(row) = call.settings.try_get(name.trim()) else {
        if call.services.is_vtank_option(name) {
            return call.services.set_meta_option(call.host, name, value);
        }
        call.write_ub_error(&format!(
            "Invalid option: {}",
            casing::to_lower_invariant(name)
        ));
        return false;
    };
    if row.kind() == UbSettingKind::Collection {
        let ExpressionValue::List(list) = value else {
            call.write_ub_error(&format!("{} expects a value of type list", row.name()));
            return false;
        };
        let items: Vec<String> = list
            .items()
            .iter()
            .map(ExpressionValue::to_display_string)
            .collect();
        call.settings
            .set(&row, UbSettingValue::from_collection(&items));
        return true;
    }
    let Some(converted) = convert(row.kind(), value) else {
        call.write_ub_error(&format!(
            "{} is a {}; {} is not.",
            row.name(),
            kind_name(row.kind()),
            value.to_display_string()
        ));
        return false;
    };
    call.settings.set(&row, converted);
    echo_change(call, &row);
    true
}

#[allow(clippy::cast_possible_truncation)]
fn convert(kind: UbSettingKind, value: &ExpressionValue) -> Option<UbSettingValue> {
    if let ExpressionValue::String(text) = value {
        return UbSettingValue::try_parse(kind, text);
    }
    if kind == UbSettingKind::String {
        return Some(UbSettingValue::from_text(&value.to_display_string()));
    }
    let number = match value {
        ExpressionValue::Number(number) => *number,
        ExpressionValue::Boolean(flag) => {
            if *flag {
                1.0
            } else {
                0.0
            }
        }
        _ => return None,
    };
    let whole =
        || mosstank_dotnet::double_to_int(round_mode(number, MidpointRounding::AwayFromZero));
    match kind {
        UbSettingKind::Bool => Some(UbSettingValue::from_bool(value.is_truthy())),
        UbSettingKind::Int => Some(UbSettingValue::from_int(whole())),
        UbSettingKind::Enum => Some(UbSettingValue::from_choice(whole())),
        UbSettingKind::Single => Some(UbSettingValue::from_single(number as f32)),
        UbSettingKind::Double => Some(UbSettingValue::from_double(number)),
        // `unchecked((uint)(long)number)`: the low 32 bits of the whole part.
        #[allow(clippy::cast_sign_loss)]
        UbSettingKind::Color => Some(UbSettingValue::from_color((number as i64) as u32)),
        UbSettingKind::String | UbSettingKind::Collection => None,
    }
}

/// The kind the value is, for a caller that wants to say so.
#[must_use]
pub fn is_list(value: &ExpressionValue) -> bool {
    value.kind() == ExpressionValueKind::List
}
