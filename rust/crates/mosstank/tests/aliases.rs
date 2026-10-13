//! The aliases' interceptor as the plugin wires it (`InitializeAliases`,
//! `DisposeAliases`): installed when the plugin starts, asked about every
//! typed line against the switch and the list the UB settings hold, and
//! removed when the plugin stops.

mod common;

use ac_plugin_abi::callback::{Callback, CallbackAnswer, InterceptDecision};
use ac_plugin_abi::calls;
use common::World;
use mosstank::MossTank;
use openac_plugin::Plugin;

fn typed(world: &mut World, plugin: &mut MossTank, handle: u32, line: &str) -> CallbackAnswer {
    world
        .with_host(|host| plugin.callback(host, Callback::ChatIntercept((handle, line.to_owned()))))
}

/// The alias list gets `line` through the settings page's list editor.
fn add_alias(world: &mut World, plugin: &mut MossTank, line: &str) {
    world.act_text(plugin, "SetUbFilterText", "Aliases.DefinedAliases");
    world.act_int(plugin, "SelectUbSetting", 0);
    world.act(plugin, "ShowUbListEditor");
    world.act_text(plugin, "AddUbListEntry", line);
}

#[test]
fn the_interceptor_is_installed_at_start_and_removed_at_stop() {
    let mut world = World::with_character("Aliaser");
    world.set_reply::<calls::ChatAddInterceptor>(&Ok(7));
    let mut plugin = world.enable();
    assert_eq!(world.requests::<calls::ChatAddInterceptor>().len(), 1);

    world.disable(&mut plugin);

    assert_eq!(world.requests::<calls::ChatRemoveInterceptor>(), [7]);
}

#[test]
fn a_typed_line_is_held_against_the_alias_list() {
    let mut world = World::with_character("Aliaser");
    world.set_reply::<calls::ChatAddInterceptor>(&Ok(7));
    let mut plugin = world.enable();
    world.command(&mut plugin, "ub", "opt set Aliases.Enabled true");
    add_alias(&mut world, &mut plugin, "^hi$ = /say hello");

    assert_eq!(
        typed(&mut world, &mut plugin, 7, "hi"),
        CallbackAnswer::Intercept(InterceptDecision::Rewrite("/say hello".to_owned()))
    );
    assert_eq!(
        typed(&mut world, &mut plugin, 7, "bye"),
        CallbackAnswer::Intercept(InterceptDecision::Pass)
    );
    // Another plugin's interceptor is not this one.
    assert_eq!(
        typed(&mut world, &mut plugin, 8, "hi"),
        CallbackAnswer::Nothing
    );

    world.command(&mut plugin, "ub", "opt set Aliases.Enabled false");
    assert_eq!(
        typed(&mut world, &mut plugin, 7, "hi"),
        CallbackAnswer::Intercept(InterceptDecision::Pass)
    );
}
