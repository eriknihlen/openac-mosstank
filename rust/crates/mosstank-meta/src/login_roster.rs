//! The account's characters in the one order everything counts them in:
//! alphabetically by name (`LoginRoster`).

use ac_plugin_abi::social::LoginCharacter;
use mosstank_dotnet::text::ordinal;

/// Why a selector named nobody on the account's character list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginRosterFailure {
    /// It named somebody.
    None,
    /// The account's character list has not arrived yet.
    Empty,
    /// No character's name contains the text given.
    NoSuchName,
    /// An index counted from the current character needs that character on
    /// the list, and it is not.
    NoCurrentCharacter,
    /// The index landed off either end of the list and no wrap was asked for.
    OutOfRange,
    /// The character named is scheduled for deletion.
    PendingDelete,
}

/// The account's characters sorted by name: ordinal, ignoring case; ties
/// fall back on the account slot so the order is total.
#[must_use]
pub fn sorted(mut roster: Vec<LoginCharacter>) -> Vec<LoginCharacter> {
    if roster.len() < 2 {
        return roster;
    }
    // List<T>.Sort is not stable, but the comparison is total over distinct
    // slots, so any sort gives the same order.
    roster.sort_by(|left, right| {
        ordinal::compare_ignore_case(&left.name, &right.name)
            .then(left.active_index.cmp(&right.active_index))
    });
    roster
}

/// Where the first character whose name contains `name` (ignoring case)
/// sits in the sorted roster, or -1.
#[must_use]
pub fn index_of_name(roster: &[LoginCharacter], name: &str) -> i32 {
    roster
        .iter()
        .position(|character| ordinal::contains_ignore_case(&character.name, name))
        .map_or(-1, |index| i32::try_from(index).unwrap_or(i32::MAX))
}

/// Turns a selector into a position in the sorted roster. A selector that
/// parses as a whole number is an index; anything else is part of a name,
/// and a name is always absolute.
///
/// # Errors
/// Why nothing was resolved.
pub fn try_resolve(
    roster: &[LoginCharacter],
    selector: &str,
    current_name: &str,
    relative: bool,
    looping: bool,
) -> Result<usize, LoginRosterFailure> {
    if roster.is_empty() {
        return Err(LoginRosterFailure::Empty);
    }
    let count = i64::try_from(roster.len()).unwrap_or(i64::MAX);
    let text = selector.trim();
    let resolved = if let Some(requested) = mosstank_dotnet::try_parse_int(text) {
        let mut requested = i64::from(requested);
        if relative {
            let current = index_of_name(roster, current_name);
            if current < 0 {
                return Err(LoginRosterFailure::NoCurrentCharacter);
            }
            requested += i64::from(current);
        }
        if looping {
            requested = ((requested % count) + count) % count;
        }
        if requested < 0 || requested >= count {
            return Err(LoginRosterFailure::OutOfRange);
        }
        usize::try_from(requested).unwrap_or(0)
    } else {
        let index = index_of_name(roster, text);
        if index < 0 {
            return Err(LoginRosterFailure::NoSuchName);
        }
        usize::try_from(index).unwrap_or(0)
    };
    if roster[resolved].is_pending_delete || roster[resolved].object_id == 0 {
        return Err(LoginRosterFailure::PendingDelete);
    }
    Ok(resolved)
}
