//! The C# panel tests' `FakeAutomation` (`MossTankPanelTests.FakeAutomation`)
//! as a client the plugin talks to through the SDK: the character and its
//! spells, the casts it is asked for and what each one does to the chat,
//! the cast receipt and the enchantments, the components, the inventory,
//! the appraisals, the selection, the combat mode and the movement intents.
//!
//! The same struct plays the C#'s `CombatCapableFakeAutomation` too
//! ([`FakeAutomation::combat_capable`]): equipment that equips, a stance,
//! the monsters around and the swings at them.
//!
//! The golden values of the panel tests depend on how this fake answers, so
//! it follows the C# one call for call. Put one in a [`World`] with
//! [`World::with_automation`]; the calls it does not answer go on to the
//! SDK's fake host.

use std::collections::{BTreeMap, BTreeSet};

use ac_plugin_abi::character::{ActiveEnchantment, Attribute, Skill, Training};
use ac_plugin_abi::combat::{CombatMode, CombatResult, CombatSnapshot, CombatStatus, CombatTarget};
use ac_plugin_abi::items::{
    EquipmentItem, EquipmentObservation, EquipmentPlacement, EquipmentResult, EquipmentStatus,
    InventoryItem, ItemResult, ItemState, ItemStatus, ItemUseCompletion,
};
use ac_plugin_abi::magic::{
    CastCompletion, CastGate, CastRequest, CastResult, ComponentInfo, SpellInfo, SpellList,
    TrackedEnchantment,
};
use ac_plugin_abi::navigation::{MovementIntent, NavigationSnapshot, NavigationStatus};
use ac_plugin_abi::social::SelectionState;
use ac_plugin_abi::types::ChatLine;
use ac_plugin_abi::world::{ItemProperties, ObjectClass, WorldObject};
use ac_plugin_abi::{CharacterInfo, Import, calls};

use super::World;

/// The log text type of a spell's result line (`logTextType: 0x07`).
pub const SPELL_RESULT_TEXT_TYPE: u32 = 0x07;

/// The log text type of a caster's spoken words
/// (`CombatLogTextType.Spellcasting`).
pub const SPELLCASTING_TEXT_TYPE: u32 = 0x11;

/// The chat kind of local speech (`SpellCastTracker.LocalSpeechChatKind`).
pub const LOCAL_SPEECH_CHAT_KIND: u32 = 0;

/// The aura school, Item Enchantment (`ItemEnchantmentSchoolId`).
pub const ITEM_ENCHANTMENT_SCHOOL: u32 = 32;

/// The C# `FakeAutomation`.
#[derive(Debug, Clone)]
pub struct FakeAutomation {
    /// In the world (`IsAvailable`).
    pub is_available: bool,
    /// `Name`.
    pub name: String,
    /// `WorldName`.
    pub world_name: String,
    /// `Level`.
    pub level: i32,
    /// `ObjectId`.
    pub object_id: u32,
    /// `CurrentHealth`, `MaxHealth`.
    pub health: (u32, u32),
    /// `CurrentStamina`, `MaxStamina`.
    pub stamina: (u32, u32),
    /// `CurrentMana`, `MaxMana`.
    pub mana: (u32, u32),
    /// `MainPackFreeSlots`.
    pub main_pack_free_slots: i32,
    /// `Skills`.
    pub skills: Vec<Skill>,
    /// `Attributes`.
    pub attributes: Vec<Attribute>,
    /// `ActiveEnchantments` (the timed list reads the same).
    pub active_enchantments: Vec<ActiveEnchantment>,
    /// `ItemEnchantments`: what the tracker holds per object.
    pub item_enchantments: BTreeMap<u32, Vec<TrackedEnchantment>>,
    /// `KnownSelfBuffs`, each with its words (set through
    /// [`FakeAutomation::set_known_self_buffs`]).
    known_self_buffs: Vec<SpellInfo>,
    /// How many times the self buffs were read (`KnownSelfBuffReads`).
    pub known_self_buff_reads: usize,
    /// `CatalogSpells`: spells the table has that are not self buffs.
    pub catalog_spells: Vec<SpellInfo>,

    /// `IsCasting`.
    pub is_casting: bool,
    /// `NextCastWeenieError`: the error every cast is answered with.
    pub next_cast_weenie_error: u32,
    /// `SuppressCastCompletion`: no receipt, words, result or enchantment.
    pub suppress_cast_completion: bool,
    /// `SuppressCastReceipt`: no receipt.
    pub suppress_cast_receipt: bool,
    /// `SuppressCastResultText`: no result line.
    pub suppress_cast_result_text: bool,
    /// `CastResultText`: the result line in place of the landed one (and
    /// then nothing lands).
    pub cast_result_text: Option<String>,
    /// `CastSpellIds`: the casts that went out, in order.
    pub cast_spell_ids: Vec<u32>,
    /// `CastSelectionIds`: what was selected for each.
    pub cast_selection_ids: Vec<u32>,
    /// `CastRequestCount`: every cast asked for at the selection, sent or
    /// not. The C# counts only `RequestCast`; here the cast tracker's
    /// re-sends of an unanswered cast, which come through the same client
    /// call, are counted too.
    pub cast_request_count: usize,
    /// `CastGate`.
    pub cast_gate: CastGate,
    /// `RefusedCastSpellIds`: casts the client will not send.
    pub refused_cast_spell_ids: BTreeSet<u32>,
    /// `MissingComponentSpellIds`: `HasComponents` answers false.
    pub missing_component_spell_ids: BTreeSet<u32>,
    /// `CastRefusals`: a request for the spell is answered with this.
    pub cast_refusals: BTreeMap<u32, CastResult>,
    /// `RaiseSkillOnCast`: (spell, skill, new level).
    pub raise_skill_on_cast: Option<(u32, u32, u32)>,
    /// `EnchantmentDurationSeconds`: how long a landed buff runs.
    pub enchantment_duration_seconds: f64,
    /// `Components`.
    pub components: BTreeMap<u32, ComponentInfo>,
    last_completion: CastCompletion,

    /// The selection (`FakeHost.Selection`).
    pub selected: Option<u32>,
    /// The selection before the last change.
    pub previous_selected: Option<u32>,
    /// `CurrentSelection` is wired to the selection: a cast records it and
    /// an aura lands on it. Off, as in the C#, a cast sees nothing selected.
    pub casts_see_selection: bool,

    /// The combat state (`CombatSnapshot`, magic mode by default).
    pub combat: CombatSnapshot,
    /// `HostileTargets`.
    pub hostile_targets: Vec<CombatTarget>,

    /// `ItemEntries`.
    pub item_entries: Vec<InventoryItem>,
    /// How many times the inventory was read (`CaptureOwnedItemsCallCount`).
    pub capture_owned_items_call_count: usize,
    /// `Properties`: an object's appraised properties.
    pub properties: BTreeMap<u32, ItemProperties>,
    /// `WorldObjects`.
    pub world_objects: Vec<WorldObject>,
    /// `ItemsBusy`.
    pub items_busy: bool,
    /// `Unassessed`: items not appraised yet.
    pub unassessed: BTreeSet<u32>,
    /// `Identified`: every appraisal asked for.
    pub identified: Vec<u32>,
    /// `IdentifyRefusals`.
    pub identify_refusals: BTreeSet<u32>,
    /// `IdentifyBusy`.
    pub identify_busy: BTreeSet<u32>,
    /// `IdentifyStatus`.
    pub identify_status: ItemStatus,
    /// `AppraisalReplies`: what an appraisal adds, once asked.
    pub appraisal_replies: BTreeMap<u32, ItemProperties>,
    /// `UsedItemIds`.
    pub used_item_ids: Vec<u32>,
    /// `RoomAhead`: what the room check answers.
    pub room_ahead: ac_plugin_abi::navigation::RoomAheadStatus,
    /// `RoomAheadAsked`: every distance the room check was asked about.
    pub room_ahead_asked: Vec<f32>,
    /// `Applied`: (tool, target).
    pub applied: Vec<(u32, u32)>,
    /// `Gives`: (item, target, amount).
    pub gives: Vec<(u32, u32, u32)>,
    /// `ItemCompletion`.
    pub item_completion: ItemUseCompletion,

    /// `NavigationSnapshot`.
    pub navigation: NavigationSnapshot,
    /// `MovementIntents`.
    pub movement_intents: Vec<MovementIntent>,
    /// `ClearMovementCount`.
    pub clear_movement_count: usize,

    /// What the character can wear or wield. The C# fake has no equipment
    /// at all, so its combat gate readies a stance without asking what is
    /// wielded; a Rust client always has equipment in the world, so the
    /// fake holds [`stand_in_caster`] instead, which readies magic the same
    /// way (see the divergence register, TEST-1).
    pub equipment: Vec<EquipmentItem>,

    /// Which of the C# fakes this one plays.
    pub kind: FakeKind,
    /// `CallLog` (combat-capable): every cast, stance, swing, use and
    /// equip, in order.
    pub call_log: Vec<String>,
    /// `CastTargets` (combat-capable): the target of each targeted cast.
    pub cast_targets: Vec<u32>,
    /// `KnownSpellIds` (combat-capable): what `IsKnown` answers true for.
    pub known_spell_ids: BTreeSet<u32>,
    /// `BeginCount`, `LastBeginTarget`, `ReleaseCount` (combat-capable).
    pub begin_count: usize,
    /// The target of the last swing begun.
    pub last_begin_target: u32,
    /// How many swings were released.
    pub release_count: usize,
    /// `ApplyCount`, `LastAppliedItem` (combat-capable).
    pub apply_count: usize,
    /// The last (item, target) used.
    pub last_applied_item: (u32, u32),
    /// `ModeChangeRequests` (combat-capable).
    pub mode_change_requests: usize,
    /// The placements an equip raised (`PlacementObserved`), handed to the
    /// plugin as events before its next frame ([`World::frame`]).
    pub pending_observations: Vec<EquipmentObservation>,
}

impl Default for FakeAutomation {
    fn default() -> Self {
        Self {
            is_available: true,
            name: "Test Character".to_owned(),
            world_name: String::new(),
            level: 0,
            object_id: 1,
            health: (0, 0),
            stamina: (0, 0),
            mana: (0, 0),
            main_pack_free_slots: 10,
            skills: Vec::new(),
            attributes: Vec::new(),
            active_enchantments: Vec::new(),
            item_enchantments: BTreeMap::new(),
            known_self_buffs: Vec::new(),
            known_self_buff_reads: 0,
            catalog_spells: Vec::new(),
            is_casting: false,
            next_cast_weenie_error: 0,
            suppress_cast_completion: false,
            suppress_cast_receipt: false,
            suppress_cast_result_text: false,
            cast_result_text: None,
            cast_spell_ids: Vec::new(),
            cast_selection_ids: Vec::new(),
            cast_request_count: 0,
            cast_gate: CastGate::Ready,
            refused_cast_spell_ids: BTreeSet::new(),
            missing_component_spell_ids: BTreeSet::new(),
            cast_refusals: BTreeMap::new(),
            raise_skill_on_cast: None,
            enchantment_duration_seconds: 1800.0,
            components: BTreeMap::new(),
            last_completion: CastCompletion::default(),
            selected: None,
            previous_selected: None,
            casts_see_selection: false,
            combat: CombatSnapshot {
                mode: CombatMode::Magic,
                ..CombatSnapshot::default()
            },
            hostile_targets: Vec::new(),
            item_entries: Vec::new(),
            capture_owned_items_call_count: 0,
            properties: BTreeMap::new(),
            world_objects: Vec::new(),
            items_busy: false,
            unassessed: BTreeSet::new(),
            identified: Vec::new(),
            identify_refusals: BTreeSet::new(),
            identify_busy: BTreeSet::new(),
            identify_status: ItemStatus::Started,
            appraisal_replies: BTreeMap::new(),
            used_item_ids: Vec::new(),
            room_ahead: ac_plugin_abi::navigation::RoomAheadStatus::Unknown,
            room_ahead_asked: Vec::new(),
            applied: Vec::new(),
            gives: Vec::new(),
            item_completion: ItemUseCompletion::default(),
            navigation: NavigationSnapshot::default(),
            movement_intents: Vec::new(),
            clear_movement_count: 0,
            equipment: vec![stand_in_caster()],
            kind: FakeKind::Plain,
            call_log: Vec::new(),
            cast_targets: Vec::new(),
            known_spell_ids: BTreeSet::new(),
            begin_count: 0,
            last_begin_target: 0,
            release_count: 0,
            apply_count: 0,
            last_applied_item: (0, 0),
            mode_change_requests: 0,
            pending_observations: Vec::new(),
        }
    }
}

/// The object id of [`stand_in_caster`].
pub const STAND_IN_CASTER_ID: u32 = 0x7FFF_FF01;

/// A caster in the character's hand, known only to the equipment calls
/// (not an owned item, not on any profile page): with it the combat gate
/// readies magic as the C# fake's missing equipment does.
#[must_use]
pub fn stand_in_caster() -> EquipmentItem {
    EquipmentItem {
        object_id: STAND_IN_CASTER_ID,
        name: "Stand-in Caster".to_owned(),
        item_type: 0x8000,
        valid_locations: 0x0100_0000,
        equipped_location: 0x0100_0000,
        container_id: 0,
        wielder_id: 1,
        object_class: ObjectClass::WandStaffOrb,
        ..EquipmentItem::default()
    }
}

/// A trained skill at `current` (`new PluginSkillInfo(id, name, Trained, current)`).
#[must_use]
pub fn skill(skill_id: u32, name: &str, current: u32) -> Skill {
    Skill {
        skill_id,
        name: name.to_owned(),
        training: Training::Trained,
        current,
        base: current,
        ..Skill::default()
    }
}

/// An attribute (`new PluginAttributeInfo(kind, name, current)`).
#[must_use]
pub fn attribute(kind: i32, name: &str, current: u32) -> Attribute {
    Attribute {
        kind,
        name: name.to_owned(),
        current,
        base: current,
        ..Attribute::default()
    }
}

/// An enchantment in force (`new PluginActiveEnchantment(...)`).
#[must_use]
pub fn active(spell_id: u32, family: u32, tier: i32, seconds_remaining: f64) -> ActiveEnchantment {
    ActiveEnchantment {
        spell_id,
        family,
        tier,
        seconds_remaining,
    }
}

/// A scarab (`MossTankPanelTests.Component`).
#[must_use]
pub fn component(component_id: u32, name: &str) -> ComponentInfo {
    ComponentInfo {
        component_id,
        weenie_class_id: component_id,
        name: name.to_owned(),
        burn_rate: 1.0,
        gesture_id: 0,
        gesture_speed: 1.0,
        icon_id: 0,
        sort_key: component_id,
        component_type: "Scarab".to_owned(),
        word: String::new(),
    }
}

/// A spell with a name in a school (`MossTankPanelTests.NamedSpell`):
/// targeted, half an hour long.
#[must_use]
pub fn named_spell(spell_id: u32, family: u32, name: &str, school: u32) -> SpellInfo {
    SpellInfo {
        spell_id,
        name: name.to_owned(),
        family,
        tier: 1,
        difficulty: 10,
        mana_cost: 5,
        duration_seconds: 1800.0,
        school,
        description: "Increases a weapon's damage value by 2 points.".to_owned(),
        is_self_targeted: false,
        is_beneficial: true,
        ..SpellInfo::default()
    }
}

impl FakeAutomation {
    /// Full vitals (the C# tests' `100` for every current and maximum).
    #[must_use]
    pub fn with_full_vitals() -> Self {
        Self {
            health: (100, 100),
            stamina: (100, 100),
            mana: (100, 100),
            ..Self::default()
        }
    }

    /// `KnownSelfBuffs = ...`: a spell without words is given
    /// `testwords<id>`.
    pub fn set_known_self_buffs(&mut self, spells: Vec<SpellInfo>) {
        self.known_self_buffs = spells
            .into_iter()
            .map(|mut spell| {
                if spell.saying.is_empty() {
                    spell.saying = format!("testwords{}", spell.spell_id);
                }
                spell
            })
            .collect();
    }

    /// The self buffs as set.
    #[must_use]
    pub fn known_self_buffs(&self) -> &[SpellInfo] {
        &self.known_self_buffs
    }

    /// `TryGet`: the self buffs, then the catalog.
    #[must_use]
    pub fn try_get(&self, spell_id: u32) -> Option<SpellInfo> {
        self.known_self_buffs
            .iter()
            .chain(&self.catalog_spells)
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
    }

    /// `LastCompletion`.
    #[must_use]
    pub fn last_completion(&self) -> CastCompletion {
        self.last_completion.clone()
    }

    /// `PostCastReceipt`: a receipt for the character with no cast behind
    /// it.
    pub fn post_cast_receipt(&mut self, spell_id: u32, error: u32) {
        self.last_completion = CastCompletion {
            revision: self.last_completion.revision + 1,
            spell_id,
            target_id: self.object_id,
            weenie_error: error,
        };
    }

    fn character(&self) -> CharacterInfo {
        CharacterInfo {
            id: self.object_id,
            name: self.name.clone(),
            world: self.world_name.clone(),
            level: self.level,
            main_pack_free_slots: self.main_pack_free_slots,
            health: (self.health.0, self.health.1, self.health.1),
            stamina: (self.stamina.0, self.stamina.1, self.stamina.1),
            mana: (self.mana.0, self.mana.1, self.mana.1),
            ..CharacterInfo::default()
        }
    }

    fn current_selection(&self) -> u32 {
        if self.casts_see_selection {
            self.selected.unwrap_or(0)
        } else {
            0
        }
    }

    fn select(&mut self, object_id: u32) {
        self.previous_selected = self.selected;
        self.selected = Some(object_id);
    }

    fn clear_selection(&mut self) {
        self.previous_selected = self.selected;
        self.selected = None;
    }

    /// `CaptureOwnedItems`.
    /// The host folds the shared cooldown an appraisal carries into the
    /// item it reports, as it does the one sent with the item itself
    /// (`WithAppraisedSharedCooldown`).
    fn capture_owned_items(&mut self) -> Vec<InventoryItem> {
        self.capture_owned_items_call_count += 1;
        let mut items = self.item_entries.clone();
        for item in &mut items {
            if item.shared_cooldown_id == 0
                && let Some(cooldown) = self
                    .properties
                    .get(&item.object_id)
                    .and_then(|properties| properties.int(280))
            {
                item.shared_cooldown_id = u32::try_from(cooldown).unwrap();
            }
        }
        items
    }

    /// `TryCaptureProperties`.
    fn try_capture_properties(&self, object_id: u32) -> Option<ItemProperties> {
        if let Some(properties) = self.properties.get(&object_id) {
            return Some(properties.clone());
        }
        if let Some(item) = self
            .item_entries
            .iter()
            .find(|item| item.object_id == object_id)
        {
            let mut ints = Vec::new();
            if item.item_maximum_mana > 0 {
                ints.push((107, item.item_current_mana));
                ints.push((108, item.item_maximum_mana));
            }
            return Some(ItemProperties {
                ints,
                ..ItemProperties::default()
            });
        }
        self.try_get_object(object_id)
            .map(|_| ItemProperties::default())
    }

    /// `IWorldObjectAutomation.TryGet`: the world's objects, then the owned
    /// items as the object table holds them (appraised unless unassessed).
    fn try_get_object(&self, object_id: u32) -> Option<WorldObject> {
        if let Some(object) = self
            .world_objects
            .iter()
            .find(|object| object.object_id == object_id)
        {
            return Some(object.clone());
        }
        self.item_entries
            .iter()
            .find(|item| item.object_id == object_id)
            .map(|item| WorldObject {
                object_id: item.object_id,
                weenie_class_id: item.weenie_class_id,
                name: item.name.clone(),
                object_class: ObjectClass::Unknown,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                last_id_time: i32::from(!self.unassessed.contains(&item.object_id)),
                ..WorldObject::default()
            })
    }

    /// `Identify`.
    fn identify(&mut self, object_id: u32) -> ItemResult {
        self.identified.push(object_id);
        if self.identify_busy.contains(&object_id) {
            return ItemResult::of(ItemStatus::Busy);
        }
        if self.identify_refusals.contains(&object_id) {
            return ItemResult::of(ItemStatus::Refused);
        }
        if self.identify_status == ItemStatus::Started {
            self.unassessed.remove(&object_id);
            if let Some(reply) = self.appraisal_replies.remove(&object_id) {
                self.properties.insert(object_id, reply);
            }
        }
        ItemResult::of(self.identify_status)
    }

    /// `RaiseSkill`.
    fn raise_skill(&mut self, spell_id: u32) {
        let Some((raise_spell, skill_id, level)) = self.raise_skill_on_cast else {
            return;
        };
        if raise_spell != spell_id {
            return;
        }
        for skill in &mut self.skills {
            if skill.skill_id == skill_id {
                skill.current = level;
            }
        }
    }

    /// `LandEnchantment`: folds a landed buff into the tracked tables.
    fn land_enchantment(&mut self, spell_id: u32) {
        let Some(spell) = self
            .known_self_buffs
            .iter()
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
        else {
            return;
        };
        let target = self.current_selection();
        let aura = spell.school == ITEM_ENCHANTMENT_SCHOOL && !spell.is_self_targeted;
        if aura && target == self.object_id {
            return;
        }
        if aura && target != 0 && target != self.object_id {
            let held = self.item_enchantments.entry(target).or_default();
            held.retain(|enchantment| enchantment.spell_id != spell_id);
            held.push(TrackedEnchantment {
                target_id: target,
                spell_id,
                family: spell.family,
                quality: spell.tier,
                is_untargeted: spell.is_untargeted,
                seconds_remaining: self.enchantment_duration_seconds,
            });
            return;
        }
        self.active_enchantments
            .retain(|held| held.spell_id != spell_id);
        self.active_enchantments.push(ActiveEnchantment {
            spell_id,
            family: spell.family,
            tier: spell.tier,
            seconds_remaining: self.enchantment_duration_seconds,
        });
    }
}

impl World {
    /// The C# panel tests' client: `automation` in the world, its chat
    /// lines captured.
    #[must_use]
    pub fn with_automation(automation: FakeAutomation) -> Self {
        let mut world = Self::new();
        world.fake.character = None;
        world.automation = Some(automation);
        world.chat_log = Some(Vec::new());
        world
    }

    /// The fake automation.
    ///
    /// # Panics
    /// The world has none.
    pub fn automation(&mut self) -> &mut FakeAutomation {
        self.automation
            .as_mut()
            .expect("a world with a FakeAutomation")
    }

    /// The casts that went out (`CastSpellIds`).
    #[must_use]
    pub fn cast_spell_ids(&self) -> Vec<u32> {
        self.automation
            .as_ref()
            .map(|automation| automation.cast_spell_ids.clone())
            .unwrap_or_default()
    }

    /// `PostChatFrom`: a line the client logged from a speaker.
    pub fn post_chat_from(&mut self, sender_id: u32, kind: u32, text: &str, log_text_type: u32) {
        let log = self.chat_log.get_or_insert_with(Vec::new);
        let sequence = log.last().map_or(1, |line| line.sequence + 1);
        log.push(ChatLine {
            sequence,
            sender_id,
            kind,
            text: text.to_owned(),
            log_text_type,
            ..ChatLine::default()
        });
    }

    /// `PostChat` without the event: a line only the captured-chat read
    /// sees.
    pub fn post_captured(&mut self, text: &str, log_text_type: u32) {
        self.post_chat_from(0, 0, text, log_text_type);
    }

    /// `Cast`.
    fn fake_cast(&mut self, request: &CastRequest) -> bool {
        let automation = self.automation();
        let spell_id = request.spell_id;
        if automation.cast_gate == CastGate::Busy
            || automation.refused_cast_spell_ids.contains(&spell_id)
        {
            return false;
        }
        automation.cast_spell_ids.push(spell_id);
        let selection = automation.current_selection();
        automation.cast_selection_ids.push(selection);
        let error = automation.next_cast_weenie_error;
        let completes = !automation.suppress_cast_completion;
        if completes && !automation.suppress_cast_receipt {
            automation.last_completion = CastCompletion {
                revision: automation.last_completion.revision + 1,
                spell_id,
                target_id: 0,
                weenie_error: error,
            };
        }
        let spell = automation.try_get(spell_id);
        let object_id = automation.object_id;
        let show_result = error == 0 && completes && !automation.suppress_cast_result_text;
        let result_text = automation.cast_result_text.clone();
        let lands = error == 0 && completes && result_text.is_none();
        if error == 0
            && completes
            && let Some(spoken) = &spell
        {
            let saying = spoken.saying.clone();
            self.post_chat_from(
                object_id,
                LOCAL_SPEECH_CHAT_KIND,
                &saying,
                SPELLCASTING_TEXT_TYPE,
            );
        }
        if show_result {
            let name = spell.map_or_else(|| format!("Spell {spell_id}"), |spell| spell.name);
            let text = result_text.unwrap_or_else(|| format!("You cast {name} on yourself"));
            self.post_captured(&text, SPELL_RESULT_TEXT_TYPE);
        }
        if lands {
            let automation = self.automation();
            automation.land_enchantment(spell_id);
            automation.raise_skill(spell_id);
        }
        true
    }

    /// A cast through the one wire call: at the selection it is the C#'s
    /// `RequestCast(spell)` (counted, and a staged refusal answers it); at a
    /// named target it is `Cast(spell, target)`, which the C# fake answers as
    /// a plain `Cast(spell)` without selecting anything.
    fn fake_request_cast(&mut self, request: &CastRequest) -> CastResult {
        if request.target_id == 0 {
            let automation = self.automation();
            automation.cast_request_count += 1;
            if let Some(refusal) = automation.cast_refusals.get(&request.spell_id) {
                return *refusal;
            }
        }
        if self.fake_cast(request) {
            CastResult::Sent
        } else {
            CastResult::Unavailable
        }
    }

    /// Answers `import` from the fake automation, `None` for a call it
    /// leaves to the SDK's fake host.
    pub(crate) fn automation_call(&mut self, import: Import, bytes: &[u8]) -> Option<Vec<u8>> {
        let kind = self.automation.as_ref()?.kind;
        if kind == FakeKind::CombatCapable
            && let Some(reply) = self.combat_capable_call(import, bytes)
        {
            return Some(reply);
        }
        let reply = match import {
            Import::CharacterInfo => self.answer::<calls::CharacterInfo>(bytes, |world, ()| {
                let automation = world.automation();
                automation.is_available.then(|| automation.character())
            }),
            Import::CharacterSkills => self.answer::<calls::CharacterSkills>(bytes, |world, ()| {
                world.automation().skills.clone()
            }),
            Import::CharacterSkill => {
                self.answer::<calls::CharacterSkill>(bytes, |world, skill_id: u32| {
                    world
                        .automation()
                        .skills
                        .iter()
                        .find(|skill| skill.skill_id == skill_id)
                        .cloned()
                })
            }
            Import::CharacterAttributes => self
                .answer::<calls::CharacterAttributes>(bytes, |world, ()| {
                    world.automation().attributes.clone()
                }),
            Import::CharacterEnchantments => self
                .answer::<calls::CharacterEnchantments>(bytes, |world, _timed: bool| {
                    world.automation().active_enchantments.clone()
                }),
            Import::SpellsKnown => {
                self.answer::<calls::SpellsKnown>(bytes, |world, list: SpellList| match list {
                    SpellList::SelfBuffs => {
                        let automation = world.automation();
                        automation.known_self_buff_reads += 1;
                        automation.known_self_buffs.clone()
                    }
                    _ => Vec::new(),
                })
            }
            Import::SpellsGet => self.answer::<calls::SpellsGet>(bytes, |world, spell_id: u32| {
                world.automation().try_get(spell_id)
            }),
            Import::SpellsComponent => {
                self.answer::<calls::SpellsComponent>(bytes, |world, component_id: u32| {
                    world.automation().components.get(&component_id).cloned()
                })
            }
            Import::MagicIsCasting => self
                .answer::<calls::MagicIsCasting>(bytes, |world, ()| world.automation().is_casting),
            Import::MagicGate => self
                .answer::<calls::MagicGate>(bytes, |world, _request: CastRequest| {
                    world.automation().cast_gate
                }),
            Import::MagicCast => self
                .answer::<calls::MagicCast>(bytes, |world, request: CastRequest| {
                    world.fake_request_cast(&request)
                }),
            Import::MagicHasComponents => {
                self.answer::<calls::MagicHasComponents>(bytes, |world, spell_id: u32| {
                    !world
                        .automation()
                        .missing_component_spell_ids
                        .contains(&spell_id)
                })
            }
            Import::MagicLastCompletion => self
                .answer::<calls::MagicLastCompletion>(bytes, |world, ()| {
                    world.automation().last_completion()
                }),
            Import::EnchantmentsCapture => {
                self.answer::<calls::EnchantmentsCapture>(bytes, |world, target: u32| {
                    world
                        .automation()
                        .item_enchantments
                        .get(&target)
                        .cloned()
                        .unwrap_or_default()
                })
            }
            Import::SelectionState => self.answer::<calls::SelectionState>(bytes, |world, ()| {
                let automation = world.automation();
                SelectionState {
                    selected: automation.selected,
                    previous: automation.previous_selected,
                }
            }),
            Import::SelectionSelect => {
                self.answer::<calls::SelectionSelect>(bytes, |world, object_id: u32| {
                    world.automation().select(object_id);
                    true
                })
            }
            Import::SelectionClear => self.answer::<calls::SelectionClear>(bytes, |world, ()| {
                world.automation().clear_selection();
                true
            }),
            Import::CombatSnapshot => self.answer::<calls::CombatSnapshot>(bytes, |world, ()| {
                world.automation().combat.clone()
            }),
            Import::CombatEnterMode => {
                self.answer::<calls::CombatEnterMode>(bytes, |world, mode: CombatMode| {
                    world.automation().combat.mode = mode;
                    CombatResult::of(CombatStatus::ModeChangeSent)
                })
            }
            Import::CombatHostileTargets => {
                self.answer::<calls::CombatHostileTargets>(bytes, |world, maximum: f32| {
                    world
                        .automation()
                        .hostile_targets
                        .iter()
                        .filter(|target| target.distance <= maximum)
                        .cloned()
                        .collect()
                })
            }
            Import::CombatEnterDefaultMode => self
                .answer::<calls::CombatEnterDefaultMode>(bytes, |_, ()| {
                    CombatResult::of(CombatStatus::Unavailable)
                }),
            Import::CombatAbortAttack => self.answer::<calls::CombatAbortAttack>(bytes, |_, ()| {
                CombatResult::of(CombatStatus::Stopped)
            }),
            Import::ItemsOwned => self.answer::<calls::ItemsOwned>(bytes, |world, ()| {
                world.automation().capture_owned_items()
            }),
            Import::ItemsState => self.answer::<calls::ItemsState>(bytes, |world, ()| {
                let automation = world.automation();
                ItemState {
                    is_available: automation.is_available,
                    is_busy: automation.items_busy,
                    last_completion: automation.item_completion.clone(),
                    ..ItemState::default()
                }
            }),
            Import::ItemsProperties => self
                .answer::<calls::ItemsProperties>(bytes, |world, object_id: u32| {
                    world.automation().try_capture_properties(object_id)
                }),
            Import::WorldProperties => self
                .answer::<calls::WorldProperties>(bytes, |world, object_id: u32| {
                    world.automation().try_capture_properties(object_id)
                }),
            Import::WorldObject => {
                self.answer::<calls::WorldObject>(bytes, |world, object_id: u32| {
                    let automation = world.automation();
                    automation.try_get_object(object_id).or_else(|| {
                        automation
                            .equipment
                            .iter()
                            .find(|item| item.object_id == object_id)
                            .map(|item| WorldObject {
                                object_id: item.object_id,
                                name: item.name.clone(),
                                object_class: item.object_class,
                                item_type: item.item_type,
                                container_id: item.container_id,
                                wielder_id: item.wielder_id,
                                is_owned: true,
                                has_appraisal_data: true,
                                last_id_time: 1,
                                ..WorldObject::default()
                            })
                    })
                })
            }
            Import::EquipmentOwned => self.answer::<calls::EquipmentOwned>(bytes, |world, ()| {
                world.automation().equipment.clone()
            }),
            Import::EquipmentPlacements => {
                self.answer::<calls::EquipmentPlacements>(bytes, |world, ()| {
                    world
                        .automation()
                        .equipment
                        .iter()
                        .filter(|item| item.equipped_location != 0)
                        .map(|item| EquipmentPlacement {
                            object_id: item.object_id,
                            equipped_location: item.equipped_location,
                        })
                        .collect()
                })
            }
            Import::WorldObjects => self.answer::<calls::WorldObjects>(bytes, |world, ()| {
                world.automation().world_objects.clone()
            }),
            Import::WorldIdentify => self
                .answer::<calls::WorldIdentify>(bytes, |world, object_id: u32| {
                    world.automation().identify(object_id)
                }),
            Import::ItemsUse => self.answer::<calls::ItemsUse>(bytes, |world, object_id: u32| {
                world.automation().used_item_ids.push(object_id);
                ItemResult::of(ItemStatus::Started)
            }),
            Import::ItemsApply => {
                self.answer::<calls::ItemsApply>(bytes, |world, (object_id, target): (u32, u32)| {
                    world.automation().applied.push((object_id, target));
                    ItemResult::of(ItemStatus::Started)
                })
            }
            Import::ItemsGive => self.answer::<calls::ItemsGive>(bytes, |world, give| {
                world
                    .automation()
                    .gives
                    .push((give.object_id, give.target_id, give.amount));
                ItemResult::of(ItemStatus::Started)
            }),
            Import::NavigationSnapshot => self
                .answer::<calls::NavigationSnapshot>(bytes, |world, ()| {
                    world.automation().navigation.clone()
                }),
            Import::NavigationSetIntent => {
                self.answer::<calls::NavigationSetIntent>(bytes, |world, intent| {
                    world.automation().movement_intents.push(intent);
                    NavigationStatus::Accepted
                })
            }
            Import::NavigationClearIntent => {
                self.answer::<calls::NavigationClearIntent>(bytes, |world, ()| {
                    world.automation().clear_movement_count += 1;
                    NavigationStatus::Accepted
                })
            }
            Import::NavigationRoomAhead => {
                self.answer::<calls::NavigationRoomAhead>(bytes, |world, distance: f32| {
                    let automation = world.automation();
                    automation.room_ahead_asked.push(distance);
                    ac_plugin_abi::navigation::RoomAhead {
                        status: automation.room_ahead,
                        ..ac_plugin_abi::navigation::RoomAhead::default()
                    }
                })
            }
            _ => return None,
        };
        Some(reply)
    }
}

/// Which of the C# panel tests' fakes a [`FakeAutomation`] plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FakeKind {
    /// `FakeAutomation`: no equipment (see [`stand_in_caster`]), casts
    /// that speak, answer and land, appraisals and item uses.
    #[default]
    Plain,
    /// `CombatCapableFakeAutomation`: equipment that equips, a stance, the
    /// monsters around and the swings at them; casts answer without words
    /// and land for ten minutes; nothing is appraised or used.
    CombatCapable,
}

/// `NavigationAt(heading)`: in the world, outdoors at the origin of cell
/// 0x00010001.
#[must_use]
pub fn navigation_at(heading_degrees: f32) -> NavigationSnapshot {
    let position = ac_plugin_abi::world::Position {
        cell_id: 0x0001_0001,
        east_west: 0.0,
        north_south: 0.0,
        elevation: 0.0,
        heading_degrees,
        is_outdoor: true,
    };
    NavigationSnapshot {
        is_available: true,
        is_portal_space: false,
        local_object_id: 1,
        position: position.clone(),
        confirmed_position: position,
        ..NavigationSnapshot::default()
    }
}

/// `EquipmentItem(id, name, itemType)`: something the character can wield
/// in the main hand, not wielded yet.
#[must_use]
pub fn equipment_item(object_id: u32, name: &str, item_type: u32) -> EquipmentItem {
    EquipmentItem {
        object_id,
        name: name.to_owned(),
        item_type,
        valid_locations: 0x0010_0000,
        equipped_location: 0,
        container_id: 1,
        wielder_id: 0,
        combat_use: 1,
        damage_type: 0,
        weapon_skill: 44,
        damage: 20,
        damage_variance: 0.25,
        object_class: super::item(object_id, name, item_type).object_class,
        ..EquipmentItem::default()
    }
}

/// `new PluginCombatTarget(id, name, wcid, distance, 0, true, 1)`.
#[must_use]
pub fn combat_target(
    object_id: u32,
    name: &str,
    weenie_class_id: u32,
    distance: f32,
) -> CombatTarget {
    CombatTarget {
        object_id,
        name: name.to_owned(),
        weenie_class_id,
        distance,
        relative_angle_degrees: 0.0,
        is_health_known: true,
        health_fraction: 1.0,
        ..CombatTarget::default()
    }
}

impl FakeAutomation {
    /// `new CombatCapableFakeAutomation()`: nameless, in peace at the
    /// origin, nothing owned or wielded.
    #[must_use]
    pub fn combat_capable() -> Self {
        Self {
            kind: FakeKind::CombatCapable,
            name: String::new(),
            equipment: Vec::new(),
            combat: CombatSnapshot {
                mode: CombatMode::Peace,
                attack_height: ac_plugin_abi::combat::AttackHeight::Medium,
                desired_power: 0.5,
                ..CombatSnapshot::default()
            },
            navigation: navigation_at(0.0),
            ..Self::default()
        }
    }

    /// `IWorldObjectAutomation.TryGet` of the combat-capable fake: the
    /// equipment, the owned items (all appraised), then the monsters.
    fn combat_try_get_object(&self, object_id: u32) -> Option<WorldObject> {
        if let Some(item) = self
            .equipment
            .iter()
            .find(|item| item.object_id == object_id)
        {
            return Some(equipment_object(item));
        }
        if let Some(item) = self
            .item_entries
            .iter()
            .find(|item| item.object_id == object_id)
        {
            return Some(WorldObject {
                object_id: item.object_id,
                weenie_class_id: item.weenie_class_id,
                name: item.name.clone(),
                object_class: ObjectClass::Unknown,
                item_type: item.item_type,
                container_id: item.container_id,
                wielder_id: item.wielder_id,
                last_id_time: 1,
                ..WorldObject::default()
            });
        }
        self.hostile_targets
            .iter()
            .find(|target| target.object_id == object_id)
            .map(|target| WorldObject {
                object_id: target.object_id,
                weenie_class_id: target.weenie_class_id,
                name: target.name.clone(),
                object_class: ObjectClass::Unknown,
                ..WorldObject::default()
            })
    }

    /// `TryCaptureProperties` of the combat-capable fake.
    fn combat_try_capture_properties(&self, object_id: u32) -> Option<ItemProperties> {
        if let Some(properties) = self.properties.get(&object_id) {
            return Some(properties.clone());
        }
        if let Some(item) = self
            .item_entries
            .iter()
            .find(|item| item.object_id == object_id)
        {
            let mut ints = Vec::new();
            if item.item_maximum_mana > 0 {
                ints.push((107, item.item_current_mana));
                ints.push((108, item.item_maximum_mana));
            }
            return Some(ItemProperties {
                ints,
                ..ItemProperties::default()
            });
        }
        self.combat_try_get_object(object_id)
            .map(|_| ItemProperties::default())
    }

    /// `CaptureObjects` of the combat-capable fake: the world objects and
    /// then the equipment.
    fn combat_capture_objects(&self) -> Vec<WorldObject> {
        let mut objects = self.world_objects.clone();
        objects.extend(self.equipment.iter().map(equipment_object));
        objects
    }

    /// `Equip`: the item goes to the main hand and everything else comes
    /// off, each change raised as a placement.
    fn combat_equip(&mut self, object_id: u32) -> EquipmentResult {
        self.call_log.push(format!("Equip:{object_id:08X}"));
        let before = self.equipment.clone();
        for item in &mut self.equipment {
            item.equipped_location = if item.object_id == object_id {
                0x0010_0000
            } else {
                0
            };
        }
        for old in &before {
            if old.object_id != object_id && old.equipped_location != 0 {
                self.pending_observations.push(EquipmentObservation {
                    object_id: old.object_id,
                    equipped_location: 0,
                    is_removal: true,
                    is_initial_placement: false,
                });
            }
        }
        self.pending_observations.push(EquipmentObservation {
            object_id,
            equipped_location: 0x0010_0000,
            is_removal: false,
            is_initial_placement: false,
        });
        EquipmentResult {
            status: EquipmentStatus::Started,
            ..EquipmentResult::default()
        }
    }

    /// `EnterDefaultMode`: the stance of the first wielded item.
    fn combat_enter_default_mode(&mut self) -> CombatResult {
        let mode = self
            .equipment
            .iter()
            .find(|item| item.equipped_location != 0)
            .map_or(CombatMode::Peace, |item| {
                if item.item_type & 0x0000_8000 != 0 {
                    CombatMode::Magic
                } else {
                    CombatMode::Melee
                }
            });
        self.combat.mode = mode;
        self.call_log.push(format!("EnterDefaultMode:{mode:?}"));
        CombatResult::of(CombatStatus::ModeChangeSent)
    }
}

/// An owned piece of equipment as the object table holds it.
fn equipment_object(item: &EquipmentItem) -> WorldObject {
    WorldObject {
        object_id: item.object_id,
        name: item.name.clone(),
        object_class: item.object_class,
        item_type: item.item_type,
        container_id: item.container_id,
        wielder_id: item.wielder_id,
        last_id_time: 1,
        is_owned: true,
        ..WorldObject::default()
    }
}

impl World {
    /// The combat-capable fake's `Cast`: always sent, answered at once, the
    /// result line without words, and a self buff lands for ten minutes.
    fn combat_cast(&mut self, request: &CastRequest) -> CastResult {
        let automation = self.automation();
        let spell_id = request.spell_id;
        if request.target_id != 0 {
            automation.cast_targets.push(request.target_id);
        }
        automation.cast_spell_ids.push(spell_id);
        automation.call_log.push(format!("Cast:{spell_id}"));
        let error = automation.next_cast_weenie_error;
        automation.last_completion = CastCompletion {
            revision: automation.last_completion.revision + 1,
            spell_id,
            target_id: 0,
            weenie_error: error,
        };
        let name = automation
            .try_get(spell_id)
            .map_or_else(|| format!("Spell {spell_id}"), |spell| spell.name);
        if let Some(spell) = automation
            .known_self_buffs
            .iter()
            .find(|spell| spell.spell_id == spell_id)
            .cloned()
        {
            automation.active_enchantments.push(ActiveEnchantment {
                spell_id,
                family: spell.family,
                tier: spell.tier,
                seconds_remaining: 600.0,
            });
        }
        if error == 0 {
            self.post_captured(
                &format!("You cast {name} on yourself"),
                SPELL_RESULT_TEXT_TYPE,
            );
        }
        CastResult::Sent
    }

    /// The calls the combat-capable fake answers its own way.
    #[allow(clippy::too_many_lines)]
    fn combat_capable_call(&mut self, import: Import, bytes: &[u8]) -> Option<Vec<u8>> {
        let reply = match import {
            Import::MagicGate => {
                self.answer::<calls::MagicGate>(bytes, |_, _request: CastRequest| CastGate::Ready)
            }
            Import::MagicCast => self
                .answer::<calls::MagicCast>(bytes, |world, request: CastRequest| {
                    world.combat_cast(&request)
                }),
            Import::SpellsIsKnown => self
                .answer::<calls::SpellsIsKnown>(bytes, |world, spell_id: u32| {
                    world.automation().known_spell_ids.contains(&spell_id)
                }),
            Import::WorldObject => self
                .answer::<calls::WorldObject>(bytes, |world, object_id: u32| {
                    world.automation().combat_try_get_object(object_id)
                }),
            Import::WorldObjects => self.answer::<calls::WorldObjects>(bytes, |world, ()| {
                world.automation().combat_capture_objects()
            }),
            Import::ItemsProperties => self
                .answer::<calls::ItemsProperties>(bytes, |world, object_id: u32| {
                    world.automation().combat_try_capture_properties(object_id)
                }),
            Import::WorldProperties => self
                .answer::<calls::WorldProperties>(bytes, |world, object_id: u32| {
                    world.automation().combat_try_capture_properties(object_id)
                }),
            Import::WorldIdentify => self.answer::<calls::WorldIdentify>(bytes, |_, _id: u32| {
                ItemResult::of(ItemStatus::Unavailable)
            }),
            Import::ItemsOwned => self.answer::<calls::ItemsOwned>(bytes, |world, ()| {
                world.automation().item_entries.clone()
            }),
            Import::ItemsUse => self.answer::<calls::ItemsUse>(bytes, |_, _id: u32| {
                ItemResult::of(ItemStatus::Unavailable)
            }),
            Import::ItemsGive => self.answer::<calls::ItemsGive>(bytes, |_, _give| {
                ItemResult::of(ItemStatus::Unavailable)
            }),
            Import::ItemsApply => {
                self.answer::<calls::ItemsApply>(bytes, |world, (object_id, target): (u32, u32)| {
                    let automation = world.automation();
                    automation.apply_count += 1;
                    automation.last_applied_item = (object_id, target);
                    automation
                        .call_log
                        .push(format!("Apply:{object_id:08X}->{target:08X}"));
                    ItemResult::of(ItemStatus::Started)
                })
            }
            Import::CombatHostileTargets => self
                .answer::<calls::CombatHostileTargets>(bytes, |world, _maximum: f32| {
                    world.automation().hostile_targets.clone()
                }),
            Import::CombatEnterMode => {
                self.answer::<calls::CombatEnterMode>(bytes, |world, mode: CombatMode| {
                    let automation = world.automation();
                    automation.mode_change_requests += 1;
                    automation.combat.mode = mode;
                    automation.call_log.push(format!("EnterMode:{mode:?}"));
                    CombatResult::of(CombatStatus::ModeChangeSent)
                })
            }
            Import::CombatEnterDefaultMode => self
                .answer::<calls::CombatEnterDefaultMode>(bytes, |world, ()| {
                    world.automation().combat_enter_default_mode()
                }),
            Import::CombatBeginAttack => {
                self.answer::<calls::CombatBeginAttack>(bytes, |world, request| {
                    let automation = world.automation();
                    automation.last_begin_target = request.target_id;
                    automation.begin_count += 1;
                    automation
                        .call_log
                        .push(format!("Attack:{:08X}", request.target_id));
                    CombatResult::of(CombatStatus::Started)
                })
            }
            Import::CombatReleaseAttack => {
                self.answer::<calls::CombatReleaseAttack>(bytes, |world, ()| {
                    let automation = world.automation();
                    automation.release_count += 1;
                    automation.call_log.push("Release".to_owned());
                    CombatResult::of(CombatStatus::Released)
                })
            }
            Import::EquipmentPlacements => {
                self.answer::<calls::EquipmentPlacements>(bytes, |world, ()| {
                    world
                        .automation()
                        .equipment
                        .iter()
                        .map(|item| EquipmentPlacement {
                            object_id: item.object_id,
                            equipped_location: item.equipped_location,
                        })
                        .collect()
                })
            }
            Import::EquipmentEquip => self
                .answer::<calls::EquipmentEquip>(bytes, |world, request| {
                    world.automation().combat_equip(request.object_id)
                }),
            _ => return None,
        };
        Some(reply)
    }

    /// One frame of the macro (`panel.OnTick`), after the placements the
    /// fake's equips raised since the last one have reached the plugin.
    pub fn frame(&mut self, plugin: &mut mosstank::MossTank, seconds: f64) {
        let observations = self
            .automation
            .as_mut()
            .map(|automation| std::mem::take(&mut automation.pending_observations))
            .unwrap_or_default();
        for observation in observations {
            self.send(
                plugin,
                ac_plugin_abi::Event::EquipmentPlacement(observation),
            );
        }
        self.with_tank(plugin, |tank, client| tank.on_tick(client, seconds));
    }
}
