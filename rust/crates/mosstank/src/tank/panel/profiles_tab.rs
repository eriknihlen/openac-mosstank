//! The Profiles page and the loot editor window: the four profile pickers,
//! their create, copy, clear and delete, the loot engine choice, and the
//! ordered loot rule editor.

use ac_plugin_abi::ui::UiValue;
use mosstank_formats::utl::VtankSalvageCombineSettings;
use mosstank_formats::utl::{LootAction, LootRule};
use mosstank_loot::expression::LootRuleExpression;
use openac_plugin_compat::number;

use super::{Arg, TankTab, b, i, t, tl};
use crate::tank::TankCx;
use crate::tank::profiles::report;

/// Every loot action, in number order.
fn loot_actions() -> Vec<LootAction> {
    (0..=11).filter_map(LootAction::from_i32).collect()
}

/// A loot action's member name.
fn loot_action_name(action: LootAction) -> String {
    format!("{action:?}")
}

/// `Enum.TryParse<LootAction>(value, ignoreCase: true)`: a member name,
/// any case, or its number.
fn parse_loot_action(value: &str) -> Option<LootAction> {
    let value = value.trim();
    if let Some(number) = openac_plugin_compat::number::parse_i32(
        value,
        openac_plugin_compat::number::NumberStyles::INTEGER,
    ) {
        return LootAction::from_i32(number);
    }
    loot_actions().into_iter().find(|action| {
        mosstank_dotnet::text::ordinal::equals_ignore_case(&loot_action_name(*action), value)
    })
}

/// `Enum.GetNames<LootAction>()`.
pub(crate) fn loot_action_names() -> Vec<String> {
    loot_actions().into_iter().map(loot_action_name).collect()
}

/// The name a classifier shows by (`FormatClassifier`).
fn format_classifier(display_name: &str, id: &str) -> String {
    format!("{display_name} [{id}]")
}

pub(super) fn value(cx: &mut TankCx<'_>, name: &str) -> Option<UiValue> {
    match name {
        "MacroProfileNames" => {
            let names = report!(
                cx.client,
                cx.core.profiles.available_names(cx.client.stores())
            );
            tl(names.unwrap_or_default())
        }
        "SelectedMacroProfile" => t(cx.core.profiles.selected()),
        "MineOnlyEnabled" => b(cx.core.profiles.mine_only()),
        "ProfileNameDraft" => t(cx.core.panel.profile_name_draft.clone()),
        "ProfileLifecycleNotice" => t(cx.core.profile_lifecycle_notice_text()),
        "RouteProfileNames" => {
            let names = report!(
                cx.client,
                cx.core.route_profiles.available_names(cx.client.stores())
            )
            .unwrap_or_default();
            match cx.core.embedded_route_label.clone() {
                Some(label) => tl(std::iter::once(label).chain(names).collect()),
                None => tl(names),
            }
        }
        "SelectedRouteProfile" => {
            let selected = match cx.core.embedded_route_label.clone() {
                Some(label) => label,
                None => report!(
                    cx.client,
                    cx.core.route_profiles.selected(cx.client.stores())
                )
                .unwrap_or_default(),
            };
            t(selected)
        }
        "RouteProfileNameDraft" => t(cx.core.panel.route_profile_name_draft.clone()),
        "LootProfileNames" => {
            let names = report!(
                cx.client,
                cx.core.loot_profiles.available_names(cx.client.stores())
            );
            tl(names.unwrap_or_default())
        }
        "LootProfileName" => t(cx.core.loot_profiles.selected()),
        "LootProfileNameDraft" => t(cx.core.panel.loot_profile_name_draft.clone()),
        "MetaProfileNames" => {
            let names = report!(
                cx.client,
                cx.core.meta_profiles.available_names(cx.client.stores())
            );
            tl(names.unwrap_or_default())
        }
        "SelectedMetaProfile" => {
            let selected = report!(
                cx.client,
                cx.core.meta_profiles.selected(cx.client.stores())
            );
            t(selected.unwrap_or_default())
        }
        "MetaProfileNameDraft" => t(cx.core.panel.meta_profile_name_draft.clone()),
        "LootClassifierNames" => tl(cx.loot_classifier_names()),
        "SelectedLootClassifier" => t(cx.selected_loot_classifier()),

        // The loot editor.
        "LootEditorVisible" => b(cx.core.panel.loot_editor_visible),
        "LootAllCorpsesEnabled" => b(cx.core.settings.inventory.loot.loot_all_corpses),
        "LootFellowCorpsesEnabled" => b(cx.core.settings.inventory.loot.loot_fellow_corpses),
        "ReadUnknownScrollsEnabled" => b(cx.core.settings.inventory.loot.read_unknown_scrolls),
        "LootRangeText" => t(format!(
            "Corpse range {}m",
            number::format_f64(cx.core.settings.inventory.loot.corpse_approach_range, "0")
                .unwrap_or_default()
        )),
        "LootRuleRows" => tl(cx.core.panel.loot_rule_rows.clone()),
        "SelectedLootRuleIndex" => i(cx.core.panel.selected_loot_rule),
        "LootExpressionDraft" => t(cx.core.panel.loot_expression_draft.clone()),
        "LootEditorNotice" => t(cx.core.loot_editor_notice.clone()),
        "LootActionNames" => tl(loot_action_names()),
        "SelectedLootAction" => t(loot_action_name(
            cx.selected_loot_rule()
                .map_or(LootAction::Keep, |rule| rule.action),
        )),
        "LootPriorityText" => t(format!(
            "Priority {}",
            cx.selected_loot_rule().map_or(0, |rule| rule.priority)
        )),
        "LootKeepCountText" => t(format!(
            "Keep up to {}",
            cx.selected_loot_rule().map_or(1, |rule| rule.keep_count)
        )),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn act(cx: &mut TankCx<'_>, name: &str, arg: &Arg) -> bool {
    match name {
        "SelectMacroProfile" => {
            cx.select_profile(arg.text());
            cx.after_profile_change();
        }
        "SetProfileNameDraft" => arg.text().clone_into(&mut cx.core.panel.profile_name_draft),
        "CreateNamedProfile" => {
            arg.text().clone_into(&mut cx.core.panel.profile_name_draft);
            cx.create_profile(false);
        }
        "CreateProfile" => cx.create_profile(false),
        "CopyProfile" => {
            let command = if cx.core.profiles.mine_only() {
                "/vt settings savechar "
            } else {
                "/vt settings save "
            };
            cx.compose_profile_command(command);
        }
        "ClearProfile" => {
            let core = &mut *cx.core;
            let cleared = core.profiles.clear_current(
                cx.client.stores(),
                &mut core.settings,
                &mut core.no_buff_item_names,
                &mut core.command_log_types,
            );
            report!(cx.client, cleared);
            let selected = cx.core.profiles.selected();
            cx.core.profile_lifecycle_notice = format!("Cleared {selected} to VTank defaults.");
            cx.reset_profile_consumers();
            cx.after_profile_change();
        }
        "DeleteProfile" => {
            let deleted = report!(cx.client, cx.core.profiles.delete(cx.client.stores()));
            let Some((done, notice)) = deleted else {
                return true;
            };
            if done {
                cx.load_selected_profile();
                cx.after_profile_change();
            }
            cx.core.profile_lifecycle_notice = notice;
        }
        "ToggleMineOnly" => {
            let before = cx.core.profiles.selected();
            let mine_only = !cx.core.profiles.mine_only();
            report!(
                cx.client,
                cx.core
                    .profiles
                    .set_mine_only(cx.client.stores(), mine_only)
            );
            if !mosstank_dotnet::text::ordinal::equals_ignore_case(
                &before,
                &cx.core.profiles.selected(),
            ) {
                cx.load_selected_profile();
                cx.after_profile_change();
            }
            cx.core.profile_lifecycle_notice = if cx.core.profiles.mine_only() {
                "Showing profiles owned by this character.".to_owned()
            } else {
                "Showing profiles from all characters.".to_owned()
            };
        }

        "SelectRouteProfile" => {
            cx.select_route_profile_from_panel(arg.text());
        }
        "SetRouteProfileNameDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.route_profile_name_draft);
        }
        "CreateNamedRouteProfile" => {
            arg.text()
                .clone_into(&mut cx.core.panel.route_profile_name_draft);
            cx.create_route_profile(false);
        }
        "CreateRouteProfile" => cx.create_route_profile(false),
        "CopyRouteProfile" => cx.compose_profile_command("/vt nav save "),
        "CopyRouteProfileToAf" => cx.compose_profile_command("/vt navaf save "),
        "ClearRouteProfile" => {
            cx.core.embedded_route_label = None;
            let core = &mut *cx.core;
            core.route_profiles
                .clear_current(cx.client.stores(), &mut core.settings.navigation);
            cx.reset_navigation();
            let selected = report!(
                cx.client,
                cx.core.route_profiles.selected(cx.client.stores())
            )
            .unwrap_or_default();
            cx.core.route_notice = format!("Cleared {selected}.");
        }
        "DeleteRouteProfile" => {
            let deleted = report!(cx.client, cx.core.route_profiles.delete(cx.client.stores()));
            let Some((done, notice)) = deleted else {
                return true;
            };
            if done {
                cx.load_route_profile();
            }
            cx.core.route_notice = notice;
        }

        "SelectLootProfile" => cx.select_loot_profile(arg.text()),
        "SelectLootClassifier" => cx.select_loot_classifier(arg.text()),
        "SetLootProfileNameDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.loot_profile_name_draft);
        }
        "CreateNamedLootProfile" => {
            arg.text()
                .clone_into(&mut cx.core.panel.loot_profile_name_draft);
            cx.create_loot_profile(false);
        }
        "CreateLootProfile" => cx.compose_profile_command("/vt loot new "),
        "CopyLootProfile" => cx.create_loot_profile(true),
        "ClearLootProfile" => {
            let core = &mut *cx.core;
            let loot = &mut core.settings.inventory.loot;
            let mut rules = std::mem::take(&mut loot.rules);
            let cleared =
                core.loot_profiles
                    .clear_current(cx.client.stores(), &mut rules, Some(loot));
            core.settings.inventory.loot.rules = rules;
            report!(cx.client, cleared);
            cx.core.reset_loot(cx.client);
            cx.refresh_loot_editor(false);
            let selected = cx.core.loot_profiles.selected();
            cx.core.loot_editor_notice = format!("Cleared {selected}.");
        }
        "DeleteLootProfile" => {
            let deleted = report!(cx.client, cx.core.loot_profiles.delete(cx.client.stores()));
            let Some((done, notice)) = deleted else {
                return true;
            };
            if !done {
                cx.core.loot_editor_notice = notice;
                return true;
            }
            cx.core.settings.inventory.loot.rules.clear();
            cx.core.settings.inventory.loot.salvage_combine =
                VtankSalvageCombineSettings::default();
            if !cx.load_loot_profile_from_panel() {
                return true;
            }
            cx.core.loot_editor_notice = notice;
        }
        "ShowLootEditor" => {
            cx.core.panel.active_tab = TankTab::Profiles;
            cx.core.panel.loot_editor_visible = true;
            cx.refresh_loot_editor(false);
        }
        "ToggleLootEditorVisible" => {
            if cx.core.panel.loot_editor_visible {
                cx.core.panel.loot_editor_visible = false;
            } else {
                cx.core.panel.active_tab = TankTab::Profiles;
                cx.core.panel.loot_editor_visible = true;
                cx.refresh_loot_editor(false);
            }
        }
        "CloseLootEditor" => cx.core.panel.loot_editor_visible = false,
        "SelectLootRule" => cx.select_loot_rule(arg.int()),
        "SetLootExpressionDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.loot_expression_draft);
        }
        "ApplyLootExpression" => {
            arg.text()
                .clone_into(&mut cx.core.panel.loot_expression_draft);
            cx.apply_loot_expression();
        }
        "ApplyLootRule" => cx.apply_loot_expression(),
        "AddLootRule" => cx.add_loot_rule(),
        "RemoveLootRule" => cx.remove_loot_rule(),
        "MoveLootRuleUp" => cx.move_loot_rule(-1),
        "MoveLootRuleDown" => cx.move_loot_rule(1),
        "SelectLootAction" => cx.select_loot_action(arg.text()),
        "LootPriorityDown" => {
            cx.update_selected_loot_rule(|rule| rule.priority = (rule.priority - 1).max(-1000));
        }
        "LootPriorityUp" => {
            cx.update_selected_loot_rule(|rule| rule.priority = (rule.priority + 1).min(1000));
        }
        "LootKeepCountDown" => {
            cx.update_selected_loot_rule(|rule| rule.keep_count = (rule.keep_count - 1).max(0));
        }
        "LootKeepCountUp" => {
            cx.update_selected_loot_rule(|rule| {
                rule.keep_count = (rule.keep_count + 1).min(100_000);
            });
        }
        "ToggleLootAllCorpses" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.loot_all_corpses = !loot.loot_all_corpses;
            cx.core.save_profile(cx.client);
        }
        "ToggleLootFellowCorpses" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.loot_fellow_corpses = !loot.loot_fellow_corpses;
            cx.core.save_profile(cx.client);
        }
        "ToggleReadUnknownScrolls" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.read_unknown_scrolls = !loot.read_unknown_scrolls;
            cx.core.save_profile(cx.client);
        }
        "LootRangeDown" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.corpse_approach_range = (loot.corpse_approach_range - 2.0).max(0.0);
            cx.core.save_profile(cx.client);
        }
        "LootRangeUp" => {
            let loot = &mut cx.core.settings.inventory.loot;
            loot.corpse_approach_range = (loot.corpse_approach_range + 2.0).min(100.0);
            cx.core.save_profile(cx.client);
        }

        "SelectMetaProfile" => {
            cx.select_meta_profile(arg.text());
            cx.after_meta_profile_change();
        }
        "SetMetaProfileNameDraft" => {
            arg.text()
                .clone_into(&mut cx.core.panel.meta_profile_name_draft);
        }
        "CreateNamedMetaProfile" => {
            arg.text()
                .clone_into(&mut cx.core.panel.meta_profile_name_draft);
            cx.create_meta_profile(false);
        }
        "CreateMetaProfile" => cx.create_meta_profile(false),
        "CopyMetaProfile" => cx.compose_profile_command("/vt meta save "),
        "CopyMetaProfileToAf" => cx.compose_profile_command("/vt metaaf save "),
        "ClearMetaProfile" => {
            let profile = cx.core.meta_profiles.clear_current(cx.client.stores());
            cx.with_engine(|engine, host, services| {
                engine.replace_profile(host, services, profile)
            });
            cx.core.panel.meta.selected = 0;
            let selected = report!(
                cx.client,
                cx.core.meta_profiles.selected(cx.client.stores())
            )
            .unwrap_or_default();
            cx.core.meta_notice = format!("Cleared Meta profile {selected}.");
        }
        "DeleteMetaProfile" => {
            let deleted = report!(cx.client, cx.core.meta_profiles.delete(cx.client.stores()));
            let Some((done, notice)) = deleted else {
                return true;
            };
            if done {
                cx.load_meta_profile();
                cx.after_meta_profile_change();
            }
            cx.core.meta_notice = notice;
        }
        _ => return false,
    }
    true
}

impl crate::tank::Core {
    /// `ProfileLifecycleNotice`: a recovery notice from any store first.
    pub(crate) fn profile_lifecycle_notice_text(&self) -> String {
        self.profiles
            .recovery_notice()
            .or_else(|| self.loot_profiles.recovery_notice())
            .or_else(|| self.route_profiles.recovery_notice())
            .or_else(|| self.meta_profiles.recovery_notice())
            .map_or_else(|| self.profile_lifecycle_notice.clone(), str::to_owned)
    }

    /// The looter put back to its start (`_loot.Reset()`).
    pub(crate) fn reset_loot(&mut self, client: &mut dyn crate::client::TankClient) {
        let lent = crate::tank::rules::lent_loot!(self);
        lent.loot.reset(client.looter());
    }
}

impl TankCx<'_> {
    /// `ComposeProfileCommand`: the command waits in the chat entry for a
    /// name; with the entry busy the player is told to finish first.
    pub(crate) fn compose_profile_command(&mut self, command: &str) {
        if !self.client.meta().chat_compose(command) {
            crate::tank::post_chat(
                self.client.rules(),
                &format!("Finish typing in chat, then try again. Command: {command}"),
            );
        }
    }

    /// What a settings change leaves for the windows to rebuild.
    pub(crate) fn after_profile_change(&mut self) {
        self.core.panel.items_dirty = true;
        self.refresh_loot_editor(false);
        self.refresh_advanced_options();
    }

    /// What a meta load leaves for the windows to rebuild.
    pub(crate) fn after_meta_profile_change(&mut self) {
        self.core.panel.meta.selected = 0;
    }

    /// The route's controller put back to its start (`_navigation.Reset()`).
    pub(crate) fn reset_navigation(&mut self) {
        let core = &mut *self.core;
        let mut turn = crate::tank::rules::route_turn!(self.client, core);
        core.navigation.reset(&mut turn);
    }

    /// `CreateProfileCore`.
    fn create_profile(&mut self, copy_current: bool) {
        let core = &mut *self.core;
        let draft = core.panel.profile_name_draft.clone();
        let created = core.profiles.create(
            self.client.stores(),
            Some(&draft),
            copy_current,
            &mut core.settings,
            &mut core.no_buff_item_names,
            &mut core.command_log_types,
        );
        let Some((made, notice)) = report!(self.client, created) else {
            return;
        };
        if !made {
            self.core.profile_lifecycle_notice = notice;
            return;
        }
        self.core.panel.profile_name_draft.clear();
        self.core.profile_lifecycle_notice = notice;
        self.reset_profile_consumers();
        self.after_profile_change();
    }

    /// `SelectRouteProfileCore`, with the embedded route's reload.
    fn select_route_profile_from_panel(&mut self, name: &str) {
        if self.core.embedded_route_label.as_deref() == Some(name) {
            self.reload_embedded_route(name);
            return;
        }
        self.select_route_profile(name);
    }

    /// `CreateRouteProfileCore`.
    fn create_route_profile(&mut self, copy_current: bool) {
        let core = &mut *self.core;
        let draft = core.panel.route_profile_name_draft.clone();
        let created = core.route_profiles.create(
            self.client.stores(),
            Some(&draft),
            copy_current,
            &core.settings.navigation,
        );
        let Some((made, notice)) = report!(self.client, created) else {
            return;
        };
        if !made {
            self.core.route_notice = notice;
            return;
        }
        self.core.panel.route_profile_name_draft.clear();
        self.load_route_profile();
        self.core.route_notice = notice;
    }

    /// `CreateLootProfileCore`.
    fn create_loot_profile(&mut self, copy_current: bool) {
        let core = &mut *self.core;
        let draft = core.panel.loot_profile_name_draft.clone();
        let loot = &mut core.settings.inventory.loot;
        let rules = loot.rules.clone();
        let created = core.loot_profiles.create(
            self.client.stores(),
            Some(&draft),
            copy_current,
            &rules,
            Some(loot),
        );
        let Some((made, notice)) = report!(self.client, created) else {
            return;
        };
        if !made {
            self.core.loot_editor_notice = notice;
            return;
        }
        self.core.panel.loot_profile_name_draft.clear();
        self.load_loot_profile_from_panel();
        self.core.loot_editor_notice = notice;
    }

    /// `LoadLootProfile`, with the editor rebuilt.
    pub(crate) fn load_loot_profile_from_panel(&mut self) -> bool {
        let loaded = self.core.load_loot_profile(self.client);
        self.refresh_loot_editor(false);
        loaded
    }

    /// `CreateMetaProfileCore`.
    fn create_meta_profile(&mut self, copy_current: bool) {
        let draft = self.core.panel.meta_profile_name_draft.clone();
        let current = self.meta.profile().clone();
        let created = self.core.meta_profiles.create(
            self.client.stores(),
            Some(&draft),
            copy_current,
            &current,
        );
        let Some((made, notice)) = report!(self.client, created) else {
            return;
        };
        if !made {
            self.core.meta_notice = notice;
            return;
        }
        self.core.panel.meta_profile_name_draft.clear();
        self.load_meta_profile();
        self.after_meta_profile_change();
        self.core.meta_notice = notice;
    }

    /// The classifiers the loot engine picker offers
    /// (`LootClassifierNames`).
    fn loot_classifier_names(&mut self) -> Vec<String> {
        let mut names = vec!["VTClassic".to_owned()];
        for info in self.client.loot_classifiers() {
            names.push(format_classifier(&info.display_name, &info.id));
        }
        let selected = self.selected_loot_classifier();
        if !names.contains(&selected) {
            names.push(selected);
        }
        names
    }

    /// `SelectedLootClassifier`.
    pub(crate) fn selected_loot_classifier(&mut self) -> String {
        let id = self
            .core
            .settings
            .inventory
            .loot
            .external_classifier_id
            .clone();
        if mosstank_dotnet::text::casing::is_white_space_only(&id) {
            return "VTClassic".to_owned();
        }
        for info in self.client.loot_classifiers() {
            if mosstank_dotnet::text::ordinal::equals_ignore_case(&info.id, &id) {
                return format_classifier(&info.display_name, &info.id);
            }
        }
        format!("Unavailable [{id}]")
    }

    /// `SelectLootClassifier`.
    pub(crate) fn select_loot_classifier(&mut self, value: &str) {
        let Some(id) = self.resolve_classifier_id(value) else {
            self.core.profile_lifecycle_notice = format!("Loot engine '{value}' is unavailable.");
            return;
        };
        self.core.settings.inventory.loot.external_classifier_id = id.clone();
        self.core.reset_loot(self.client);
        self.core.save_profile(self.client);
        self.core.profile_lifecycle_notice = if id.is_empty() {
            "Loot engine set to VTClassic.".to_owned()
        } else {
            format!("Loot engine set to {}.", self.selected_loot_classifier())
        };
    }

    fn resolve_classifier_id(&mut self, value: &str) -> Option<String> {
        let value = value.trim();
        if mosstank_dotnet::text::ordinal::equals_ignore_case(value, "VTClassic") {
            return Some(String::new());
        }
        self.client
            .loot_classifiers()
            .into_iter()
            .find(|info| {
                value == format_classifier(&info.display_name, &info.id)
                    || mosstank_dotnet::text::ordinal::equals_ignore_case(value, &info.id)
            })
            .map(|info| info.id)
    }

    /// The rule the loot editor edits (`SelectedLootRule`).
    pub(crate) fn selected_loot_rule(&self) -> Option<&LootRule> {
        let rules = &self.core.settings.inventory.loot.rules;
        if rules.is_empty() {
            return None;
        }
        let index = usize::try_from(self.core.panel.selected_loot_rule.max(0))
            .unwrap_or(0)
            .min(rules.len() - 1);
        rules.get(index)
    }

    fn selected_loot_rule_index(&self) -> Option<usize> {
        let count = self.core.settings.inventory.loot.rules.len();
        (count != 0).then(|| {
            usize::try_from(self.core.panel.selected_loot_rule.max(0))
                .unwrap_or(0)
                .min(count - 1)
        })
    }

    /// `RefreshLootEditor`.
    pub(crate) fn refresh_loot_editor(&mut self, retain_draft: bool) {
        self.core.refresh_loot_editor(retain_draft);
    }
}

impl crate::tank::Core {
    /// `RefreshLootEditor`: the rows and the draft from the live rules.
    pub(crate) fn refresh_loot_editor(&mut self, retain_draft: bool) {
        let rules = &self.settings.inventory.loot.rules;
        let panel = &mut self.panel;
        panel.selected_loot_rule = super::clamp_row(panel.selected_loot_rule, rules.len());
        if !retain_draft {
            panel.loot_expression_draft = rules
                .get(usize::try_from(panel.selected_loot_rule).unwrap_or(0))
                .map_or_else(|| "*".to_owned(), |rule| rule.expression().to_owned());
        }
        panel.loot_rule_rows = rules
            .iter()
            .enumerate()
            .map(|(index, rule)| {
                format!(
                    "{}. {}   P{}   {}",
                    index + 1,
                    loot_action_name(rule.action),
                    rule.priority,
                    if rule.vtank_requirements.is_empty() {
                        rule.expression().to_owned()
                    } else {
                        format!(
                            "[VTClassic: {} requirements]",
                            rule.vtank_requirements.len()
                        )
                    }
                )
            })
            .collect();
    }
}

impl TankCx<'_> {
    /// `SelectLootRuleCore`.
    fn select_loot_rule(&mut self, index: i32) {
        let rules = &self.core.settings.inventory.loot.rules;
        let Some(rule) = super::row(index, rules.len()).map(|row| &rules[row]) else {
            return;
        };
        let expression = rule.expression().to_owned();
        self.core.panel.selected_loot_rule = index;
        self.core.panel.loot_expression_draft = expression;
        self.core.loot_editor_notice = format!("Editing rule {}.", index + 1);
        self.refresh_loot_editor(true);
    }

    /// `ApplyLootExpressionCore`.
    fn apply_loot_expression(&mut self) {
        let Some(index) = self.selected_loot_rule_index() else {
            self.core.loot_editor_notice = "Add a loot rule first.".to_owned();
            return;
        };
        let draft = self.core.panel.loot_expression_draft.clone();
        if let Err(error) = LootRuleExpression::compile(&draft) {
            self.core.loot_editor_notice = error;
            return;
        }
        let rule = &mut self.core.settings.inventory.loot.rules[index];
        rule.set_expression(&draft);
        rule.custom_expression = String::new();
        rule.vtank_requirements.clear();
        rule.has_imported_requirements = false;
        self.core.loot_editor_notice = format!("Updated {}.", rule.name);
        self.core.reset_loot(self.client);
        self.refresh_loot_editor(false);
        self.core.save_profile(self.client);
    }

    /// `AddLootRuleCore`.
    fn add_loot_rule(&mut self) {
        let rules = &mut self.core.settings.inventory.loot.rules;
        let mut rule = LootRule::named(&format!("Rule {}", rules.len() + 1), "*");
        rule.action = LootAction::Keep;
        let name = rule.name.clone();
        rules.push(rule);
        self.core.panel.selected_loot_rule = i32::try_from(rules.len() - 1).unwrap_or(0);
        "*".clone_into(&mut self.core.panel.loot_expression_draft);
        self.core.loot_editor_notice = format!("Added {name}.");
        self.core.reset_loot(self.client);
        self.refresh_loot_editor(false);
        self.core.save_profile(self.client);
    }

    /// `RemoveLootRuleCore`.
    fn remove_loot_rule(&mut self) {
        let Some(index) = self.selected_loot_rule_index() else {
            self.core.loot_editor_notice = "The loot profile is empty.".to_owned();
            return;
        };
        let rules = &mut self.core.settings.inventory.loot.rules;
        let removed = rules.remove(index);
        let last = i32::try_from(rules.len().saturating_sub(1)).unwrap_or(0);
        self.core.panel.selected_loot_rule = self.core.panel.selected_loot_rule.min(last.max(0));
        self.core.loot_editor_notice = format!("Removed {}.", removed.name);
        self.core.reset_loot(self.client);
        self.refresh_loot_editor(false);
        self.core.save_profile(self.client);
    }

    /// `MoveLootRule`.
    fn move_loot_rule(&mut self, direction: i32) {
        let Some(index) = self.selected_loot_rule_index() else {
            return;
        };
        let rules = &mut self.core.settings.inventory.loot.rules;
        let destination = i64::try_from(index).unwrap_or(0) + i64::from(direction.signum());
        let Ok(destination) = usize::try_from(destination) else {
            return;
        };
        if destination >= rules.len() {
            return;
        }
        let rule = rules.remove(index);
        let name = rule.name.clone();
        rules.insert(destination, rule);
        self.core.panel.selected_loot_rule = i32::try_from(destination).unwrap_or(0);
        self.core.loot_editor_notice = format!("Moved {name}.");
        self.core.reset_loot(self.client);
        self.refresh_loot_editor(false);
        self.core.save_profile(self.client);
    }

    /// `SelectLootActionCore`.
    fn select_loot_action(&mut self, value: &str) {
        let Some(index) = self.selected_loot_rule_index() else {
            return;
        };
        let Some(action) = parse_loot_action(value) else {
            return;
        };
        let rule = &mut self.core.settings.inventory.loot.rules[index];
        rule.action = action;
        self.core.loot_editor_notice = format!("{}: {}.", rule.name, loot_action_name(action));
        self.core.reset_loot(self.client);
        self.refresh_loot_editor(false);
        self.core.save_profile(self.client);
    }

    /// `UpdateSelectedLootRule`.
    fn update_selected_loot_rule(&mut self, update: impl FnOnce(&mut LootRule)) {
        let Some(index) = self.selected_loot_rule_index() else {
            return;
        };
        let rule = &mut self.core.settings.inventory.loot.rules[index];
        update(rule);
        self.core.loot_editor_notice = format!("Updated {}.", rule.name);
        self.core.reset_loot(self.client);
        self.refresh_loot_editor(false);
        self.core.save_profile(self.client);
    }
}
