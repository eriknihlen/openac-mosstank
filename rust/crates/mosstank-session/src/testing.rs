//! Helpers the update check's tests share with the shell's
//! (`VtankGameInfoUpdaterTests.Download`).

use mosstank_formats::gameinfo;
use mosstank_formats::usd::{VtankCell, VtankDatabase, VtankRow};

use crate::game_info::VtankGameInfoAnswer;

/// A whole database as the source publishes one: every table, at the
/// built-in version, with its world-data time and the ammunition rows
/// named, changed however the test needs.
#[must_use]
pub fn download(
    time: i32,
    change: Option<&dyn Fn(&mut VtankDatabase)>,
    ammunition: &[&str],
) -> VtankGameInfoAnswer {
    let mut database = gameinfo::built_in();
    database
        .find_mut("DBLastUpdateTime")
        .expect("the built-in database has it")
        .rows[0]
        .cells[1] = VtankCell::int(time);
    for name in ammunition {
        let row = VtankRow {
            cells: vec![
                VtankCell::string(name),
                VtankCell::int(5),
                VtankCell::int(0),
                VtankCell::int(6),
                VtankCell::int(1),
                VtankCell::int(0),
                VtankCell::int(0),
                VtankCell::int(0),
            ],
        };
        database
            .find_mut("AmmunitionOptions")
            .expect("the built-in database has it")
            .rows
            .push(row);
    }
    if let Some(change) = change {
        change(&mut database);
    }
    VtankGameInfoAnswer::new(true, database.render())
}
