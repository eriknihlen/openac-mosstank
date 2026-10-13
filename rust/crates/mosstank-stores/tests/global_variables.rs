//! The shared global variables at the store: where each variable's file is,
//! the old case-blind key, damage, storage failures and the lock. The
//! values and inputs are those of the C# host-function tests that reach
//! this store (`HostExpressionFunctionsTests` and its `ReferenceParity`
//! part); those tests themselves run through the expression runtime and are
//! ported with the host functions.

use mosstank_expr::{ChangeCounter, ExpressionValue, GlobalVariableStore};
use mosstank_stores::global_variables::{SharedGlobalVariables, StorageGlobalVariableStore, hash};
use mosstank_stores::storage::{MemoryStorage, PluginStorage};

fn shared(storage: MemoryStorage) -> SharedGlobalVariables<MemoryStorage> {
    SharedGlobalVariables::new("Coldeve", storage, ChangeCounter::new(), None)
}

fn number(value: f64) -> ExpressionValue {
    ExpressionValue::Number(value)
}

fn as_number(value: Option<ExpressionValue>) -> Option<f64> {
    value.map(|value| match value {
        ExpressionValue::Number(number) => number,
        other => panic!("not a number: {other:?}"),
    })
}

/// `Sha("Coldeve")` in the C# test: twelve bytes of SHA-256, lower-case hex.
#[test]
fn a_server_and_a_name_hash_as_the_csharp_keys_them() {
    let store = StorageGlobalVariableStore::new("Coldeve");
    assert_eq!(hash("Coldeve").len(), 24);
    assert_eq!(
        store.folder(),
        format!("expressions/global/{}", hash("Coldeve"))
    );
    assert_eq!(
        store.key_for("Rare"),
        format!(
            "expressions/global/{}/{}.json",
            hash("Coldeve"),
            hash("name:Rare")
        )
    );
    assert_eq!(
        store.old_key_for("Carried"),
        format!(
            "expressions/global/{}/{}.json",
            hash("Coldeve"),
            hash("CARRIED")
        )
    );
}

/// From `GlobalVariableNamesAreCaseSensitive`: `Rare` and `rare` are two
/// variables.
#[test]
fn names_are_case_sensitive() {
    let mut globals = shared(MemoryStorage::ordinal());
    globals.set("Rare", &number(1.0)).unwrap();

    assert!(!globals.contains("rare").unwrap());
    assert_eq!(as_number(globals.try_get("rare").unwrap()), None);
    assert!(!globals.remove("RARE").unwrap());
    globals.set("rare", &number(2.0)).unwrap();
    assert_eq!(as_number(globals.try_get("Rare").unwrap()), Some(1.0));
    assert_eq!(as_number(globals.try_get("rare").unwrap()), Some(2.0));
}

/// From `AGlobalVariableSavedBeforeCaseSensitivityIsFoundByItsExactName`.
#[test]
fn a_variable_saved_under_the_old_key_is_found_by_its_exact_name_and_moves_on_set() {
    let mut storage = MemoryStorage::ordinal();
    let old_key = format!(
        "expressions/global/{}/{}.json",
        hash("Coldeve"),
        hash("CARRIED")
    );
    storage.insert(
        &old_key,
        "{\"Name\":\"Carried\",\"Value\":{\"Kind\":\"number\",\"Number\":7}}",
    );
    let mut globals = shared(storage);

    assert_eq!(as_number(globals.try_get("Carried").unwrap()), Some(7.0));
    assert!(globals.contains("Carried").unwrap());
    assert!(!globals.contains("carried").unwrap());
    assert_eq!(as_number(globals.try_get("CARRIED").unwrap()), None);

    globals.set("Carried", &number(8.0)).unwrap();
    assert!(!globals.storage().contains(&old_key));
    assert_eq!(as_number(globals.try_get("Carried").unwrap()), Some(8.0));
    assert!(globals.remove("Carried").unwrap());
    assert!(!globals.contains("Carried").unwrap());
}

/// From `ADamagedGlobalVariableIsAnErrorOfTheExpressionThatReadsIt`: every
/// damaged file is an error of the read, still exists, and can be cleared
/// or replaced; the other variables read as before.
#[test]
fn a_damaged_variable_is_an_error_of_its_read_and_still_exists() {
    for damaged in [
        "{ not json",
        "   ",
        "null",
        "{\"Name\":\"r\",\"Value\":null}",
        "{\"Name\":\"r\",\"Value\":{\"Kind\":null}}",
        "{\"Name\":\"r\",\"Value\":{\"Kind\":\"list\",\"List\":[null]}}",
        "{\"Name\":\"r\",\"Value\":{\"Kind\":\"worldobject\",\"Number\":-1}}",
    ] {
        let mut globals = shared(MemoryStorage::ordinal());
        globals.set("r", &number(1.0)).unwrap();
        let key = globals.store().key_for("r");
        globals.set("ok", &number(2.0)).unwrap();
        globals.storage().insert(&key, damaged);

        let error = globals.try_get("r").unwrap_err();
        assert!(
            error
                .message()
                .starts_with("Global variable r could not be read: "),
            "{damaged}: {}",
            error.message()
        );
        assert_eq!(as_number(globals.try_get("ok").unwrap()), Some(2.0));
        assert!(globals.contains("r").unwrap(), "{damaged}");
        assert!(globals.remove("r").unwrap(), "{damaged}");
        assert!(!globals.contains("r").unwrap(), "{damaged}");

        globals.storage().insert(&key, damaged);
        globals.set("r", &number(3.0)).unwrap();
        assert_eq!(as_number(globals.try_get("r").unwrap()), Some(3.0));
    }
}

/// From `AStorageFailureIsAnErrorOfTheExpression`: the failure's own text is
/// in the error.
#[test]
fn a_storage_failure_is_an_error_naming_the_failure() {
    let mut globals = shared(MemoryStorage::ordinal());
    globals.set("r", &number(1.0)).unwrap();
    let key = globals.store().key_for("r");

    globals.storage().unreadable.push(key.clone());
    let error = globals.try_get("r").unwrap_err();
    assert!(error.message().contains("is in use by another process"));
    assert!(
        error
            .message()
            .starts_with("The global variables could not be read or written: ")
    );

    globals.storage().unreadable.clear();
    globals.storage().unwritable.push(key);
    let error = globals.set("r", &number(2.0)).unwrap_err();
    assert!(error.message().contains("is denied"));

    globals.storage().unlistable = true;
    assert!(globals.clear().is_err());
}

/// Every read and write holds the lock named after the folder, and lets it
/// go; storage not backed by a folder is not locked.
#[test]
fn every_call_takes_and_releases_the_folder_lock() {
    let mut globals = shared(MemoryStorage::ordinal());
    globals.set("r", &number(1.0)).unwrap();
    globals.try_get("r").unwrap();
    assert!(globals.storage().locks.is_empty());

    let mut storage = MemoryStorage::ordinal();
    storage.root = None;
    let store = StorageGlobalVariableStore::new("Coldeve");
    store
        .set(&mut storage, &ChangeCounter::new(), "r", &number(1.0))
        .unwrap();
    assert!(storage.lock("x").is_ok());
}

/// The whole set, each variable once; an old copy under the case-blind key
/// loses to the variable's own file.
#[test]
fn the_whole_set_reads_each_variable_once() {
    let mut storage = MemoryStorage::ordinal();
    let store = StorageGlobalVariableStore::new("Coldeve");
    let counter = ChangeCounter::new();
    store
        .set(&mut storage, &counter, "a", &number(1.0))
        .unwrap();
    store
        .set(&mut storage, &counter, "b", &ExpressionValue::string("x"))
        .unwrap();
    storage.insert(
        &store.old_key_for("a"),
        "{\"Name\":\"a\",\"Value\":{\"Kind\":\"number\",\"Number\":7}}",
    );

    let mut captured = store.capture(&mut storage, None, &counter).unwrap();
    captured.sort_by(|x, y| x.0.cmp(&y.0));

    assert_eq!(captured.len(), 2);
    assert_eq!(captured[0].0, "a");
    assert_eq!(as_number(Some(captured[0].1.clone())), Some(1.0));
    assert_eq!(captured[1].1.to_display_string(), "x");

    store.clear(&mut storage).unwrap();
    assert!(
        store
            .capture(&mut storage, None, &counter)
            .unwrap()
            .is_empty()
    );
}

/// The file is the C#'s: System.Text.Json, indented, `Name` then `Value`.
#[test]
fn a_variable_file_is_written_as_the_csharp_writes_it() {
    let mut storage = MemoryStorage::ordinal();
    let store = StorageGlobalVariableStore::new("Coldeve");
    store
        .set(&mut storage, &ChangeCounter::new(), "count", &number(42.0))
        .unwrap();

    assert_eq!(
        storage.get(&store.key_for("count")),
        Some(
            "{\r\n  \"Name\": \"count\",\r\n  \"Value\": {\r\n    \"Kind\": \"number\",\r\n    \"Number\": 42,\r\n    \"Text\": \"\",\r\n    \"List\": null,\r\n    \"Dictionary\": null,\r\n    \"Coordinates\": null\r\n  }\r\n}"
        )
    );
}
