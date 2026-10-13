//! Combat pets (`PetAutomation.cs`, `PetDeviceCatalog.cs`,
//! `SummonPetRule.cs`): which essence to summon with for which monster
//! ([`select_pet`]), which essence to top up with which spirit
//! ([`try_select_refill`]), the refill rule ([`PetRefillRule`]) and the
//! summon rule that runs beside the main list ([`SummonPetRule`]).

use ac_plugin_abi::CharacterInfo;
use ac_plugin_abi::character::Training;
use ac_plugin_abi::combat::CombatTarget;
use ac_plugin_abi::items::{InventoryItem, ItemStatus, ItemUseCompletion};
use ac_plugin_abi::world::ItemProperties;
use mosstank_automation::surface::SUMMONING_COOLDOWN_ID;
use mosstank_automation::{Automation, RoomAhead};
use mosstank_data::damage::MonsterDamageType;
use mosstank_meta::scheduler::MacroPassContext;
use mosstank_settings::model::{CombatSettings, PetRangeMode};
use openac_plugin_compat::text::ordinal;

/// The character level an item asks for before it can be used.
pub const USE_REQUIRES_LEVEL_PROPERTY: u32 = 369;
/// An item's remaining uses.
pub const STRUCTURE_PROPERTY: u32 = 92;
/// What an essence is topped up with.
pub const ENCAPSULATED_SPIRIT_NAME: &str = "Encapsulated Spirit";
/// Summoning.
const SUMMONING_SKILL_ID: u32 = 54;
/// How far ahead of the character a summoned pet appears, metres.
pub const PET_ROOM_AHEAD_METERS: f32 = 3.0;

/// The element each summoning essence's pet strikes with, by the essence's
/// weenie class (`PetDeviceCatalog.DamageType`); `Auto` for one the table
/// does not know.
#[must_use]
pub fn device_damage_type(weenie_class_id: u32) -> MonsterDamageType {
    match weenie_class_id {
        48878 | 48880 | 48882 | 48884 | 48886 | 48888 | 48890 => MonsterDamageType::Bludgeon,
        48972
        | 49213..=49219
        | 49234..=49239
        | 49261..=49267
        | 49282..=49288
        | 49310..=49316
        | 49338..=49344
        | 49366..=49372
        | 49421..=49427
        | 49524..=49530 => MonsterDamageType::Acid,
        48942
        | 48944..=48948
        | 48956
        | 48957
        | 48959
        | 48961
        | 48963
        | 48965
        | 48967
        | 48969
        | 49247..=49253
        | 49296..=49302
        | 49324..=49330
        | 49352..=49358
        | 49380..=49386
        | 49435..=49441
        | 49531..=49537 => MonsterDamageType::Fire,
        49212
        | 49227..=49233
        | 49254..=49260
        | 49275..=49281
        | 49303..=49309
        | 49331..=49337
        | 49359..=49365
        | 49387..=49392
        | 49442..=49448
        | 49538..=49544 => MonsterDamageType::Cold,
        49220..=49226
        | 49240..=49246
        | 49268..=49274
        | 49289..=49295
        | 49317..=49323
        | 49345..=49351
        | 49373..=49379
        | 49428..=49434
        | 49545..=49551 => MonsterDamageType::Electric,
        _ => MonsterDamageType::Auto,
    }
}

/// The essence the summon rule will use and the monster it chose it for
/// (`PetSummonChoice`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PetSummonChoice {
    /// The essence.
    pub device: InventoryItem,
    /// The monster.
    pub target: CombatTarget,
}

impl PetSummonChoice {
    /// No choice.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.device.object_id == 0
    }
}

/// Whether an item summons a pet (`IsPetDevice`): it belongs to the
/// essences' shared cooldown, or the server named its pet.
pub fn is_pet_device(automation: &mut dyn Automation, item: &InventoryItem) -> bool {
    item.pet_class != 0 || automation.shared_cooldown_id(item.object_id) == SUMMONING_COOLDOWN_ID
}

/// The distance monsters are counted within (`PetRange`).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn pet_range(settings: &CombatSettings) -> f32 {
    (if settings.pet_range_mode == PetRangeMode::CUSTOM {
        settings.pet_custom_range
    } else {
        settings.maximum_range
    }) as f32
}

/// The owned items the Items page lists, in the page's order: the rows
/// named by object id first, then anything named only by name
/// (`InItemsPageOrder`).
#[must_use]
pub fn in_items_page_order(
    owned: &[InventoryItem],
    settings: &CombatSettings,
) -> Vec<InventoryItem> {
    let mut ordered: Vec<InventoryItem> = Vec::new();
    let mut placed: Vec<u32> = Vec::new();
    for &id in &settings.combat_item_order_ids {
        if let Some(item) = owned.iter().find(|item| item.object_id == id)
            && !placed.contains(&id)
        {
            placed.push(id);
            ordered.push(item.clone());
        }
    }
    for item in owned {
        if placed.contains(&item.object_id) {
            continue;
        }
        if settings.combat_item_object_ids.contains(&item.object_id)
            || settings.combat_item_names.contains(item.name.as_str())
        {
            placed.push(item.object_id);
            ordered.push(item.clone());
        }
    }
    ordered
}

/// The summon rule's choice (`SelectPet`): the nearest monster that also
/// outranks every nearer one, once enough monsters want a pet; then the
/// usable essence whose element suits it best, the higher level
/// requirement winning a tie.
#[allow(clippy::too_many_arguments)]
pub fn select_pet(
    automation: &mut dyn Automation,
    owned: &[InventoryItem],
    targets: &[CombatTarget],
    character: &CharacterInfo,
    summoning: u32,
    settings: &CombatSettings,
    attack_element: &mut dyn FnMut(&mut dyn Automation, &CombatTarget) -> MonsterDamageType,
    damage_preferences: &dyn Fn(&str) -> Vec<MonsterDamageType>,
    warn: &mut dyn FnMut(&mut dyn Automation, &str),
) -> PetSummonChoice {
    let range = pet_range(settings);
    let mut chosen: Option<&CombatTarget> = None;
    let mut nearest = f64::MAX;
    let mut highest = -1;
    let mut wanted = MonsterDamageType::None;
    let mut wanting = 0;
    for target in targets {
        if target.distance > range {
            continue;
        }
        let rule = settings.resolve_rule(target);
        if rule.priority() < 0 || rule.actions().pet_damage_type == MonsterDamageType::None {
            continue;
        }
        wanting += 1;
        // Both at once, as the reference asks it: a monster takes the pick
        // only by being nearer AND ranked higher than the one holding it.
        if f64::from(target.distance) < nearest && rule.priority() > highest {
            highest = rule.priority();
            chosen = Some(target);
            nearest = f64::from(target.distance);
            wanted = rule.actions().pet_damage_type;
        }
    }
    let Some(chosen) = chosen else {
        return PetSummonChoice::default();
    };
    if wanting < settings.pet_monster_density {
        return PetSummonChoice::default();
    }

    let attack = attack_element(automation, chosen);
    let preferences = damage_preferences(&chosen.name);

    let mut best: Option<InventoryItem> = None;
    let mut best_rank = i32::MAX;
    let mut best_level = 0;
    for item in in_items_page_order(owned, settings) {
        let Some(properties) = automation.item_properties(item.object_id) else {
            continue;
        };
        if !is_pet_device(automation, &item) {
            continue;
        }
        match usability(&item, &properties, character, summoning) {
            Usability::Usable => {}
            Usability::Unusable => continue,
            Usability::WrongMastery(text) => {
                warn(automation, &text);
                continue;
            }
        }
        let element = device_element(item.weenie_class_id);
        let level = properties.int(USE_REQUIRES_LEVEL_PROPERTY).unwrap_or(0);
        let rank = rank(wanted, element, attack, &preferences);
        if best.is_none() || rank < best_rank || (level > best_level && rank == best_rank) {
            best_level = level;
            best_rank = rank;
            best = Some(item);
        }
    }
    match best {
        Some(device) => PetSummonChoice {
            device,
            target: chosen.clone(),
        },
        None => PetSummonChoice::default(),
    }
}

/// The refill rule's choice (`TrySelectRefill`): the first essence on the
/// Items page down to the threshold and short of full, and the smallest
/// stack of spirits.
pub fn try_select_refill(
    automation: &mut dyn Automation,
    owned: &[InventoryItem],
    settings: &CombatSettings,
    threshold: i32,
) -> Option<(InventoryItem, InventoryItem)> {
    let mut device: Option<InventoryItem> = None;
    for item in in_items_page_order(owned, settings) {
        let Some(properties) = automation.item_properties(item.object_id) else {
            continue;
        };
        if !is_pet_device(automation, &item) {
            continue;
        }
        let structure = structure(&item, &properties, 99999);
        if structure <= threshold && structure < item.maximum_structure {
            device = Some(item);
            break;
        }
    }
    let device = device?;
    let spirit = smallest_stack(owned, ENCAPSULATED_SPIRIT_NAME)?;
    Some((device, spirit))
}

/// How well an essence suits the monster; lower is better (`Rank`).
fn rank(
    wanted: MonsterDamageType,
    element: MonsterDamageType,
    attack: MonsterDamageType,
    preferences: &[MonsterDamageType],
) -> i32 {
    use MonsterDamageType as D;
    match wanted {
        D::Pierce | D::Bludgeon | D::Slash | D::Acid | D::Electric | D::Cold | D::Fire => {
            if wanted == element {
                0
            } else if attack == element {
                1
            } else {
                preference_rank(element, preferences)
            }
        }
        D::PlayerAuto => {
            if attack == element {
                0
            } else {
                preference_rank(element, preferences)
            }
        }
        D::Auto => preference_rank(element, preferences),
        _ => i32::MAX,
    }
}

fn preference_rank(element: MonsterDamageType, preferences: &[MonsterDamageType]) -> i32 {
    preferences
        .iter()
        .position(|preference| *preference == element)
        .map_or(i32::MAX, |index| {
            i32::try_from(index).unwrap_or(i32::MAX - 10) + 10
        })
}

/// An essence the character can use now (`IsUsable`).
fn usability(
    item: &InventoryItem,
    properties: &ItemProperties,
    character: &CharacterInfo,
    summoning: u32,
) -> Usability {
    if properties.int(USE_REQUIRES_LEVEL_PROPERTY).unwrap_or(0) > character.level {
        return Usability::Unusable;
    }
    if i64::from(item.use_requires_skill_level) > i64::from(summoning) {
        return Usability::Unusable;
    }
    if structure(item, properties, 9999) == 0 {
        return Usability::Unusable;
    }
    if item.summoning_mastery != 0 && item.summoning_mastery != character.summoning_mastery {
        return Usability::WrongMastery(format!(
            "Warning: ignoring pet {} because you have the wrong summoning mastery!",
            item.name
        ));
    }
    Usability::Usable
}

/// Whether an essence can be used now, and the warning when it is another
/// mastery's.
enum Usability {
    Usable,
    Unusable,
    WrongMastery(String),
}

/// The essence's charges: the assessed number, else the item's own, else
/// `unknown` (`Structure`).
fn structure(item: &InventoryItem, properties: &ItemProperties, unknown: i32) -> i32 {
    if let Some(assessed) = properties.int(STRUCTURE_PROPERTY) {
        return assessed;
    }
    if item.maximum_structure > 0 {
        item.structure
    } else {
        unknown
    }
}

/// The element an essence's pet strikes with; one the table does not know
/// strikes as a bludgeon, as the golems do (`DeviceElement`).
fn device_element(weenie_class_id: u32) -> MonsterDamageType {
    match device_damage_type(weenie_class_id) {
        MonsterDamageType::Auto => MonsterDamageType::Bludgeon,
        element => element,
    }
}

fn smallest_stack(owned: &[InventoryItem], name: &str) -> Option<InventoryItem> {
    let mut smallest: Option<&InventoryItem> = None;
    let mut fewest = i32::MAX;
    for item in owned {
        if !ordinal::equals_ignore_case(&item.name, name) {
            continue;
        }
        let count = item.stack_size.max(1);
        if count < fewest {
            fewest = count;
            smallest = Some(item);
        }
    }
    smallest.cloned()
}

/// The Summoning skill's current level, 0 when the character has none.
#[must_use]
pub fn summoning_level(automation: &mut dyn Automation) -> u32 {
    automation
        .try_get_skill(SUMMONING_SKILL_ID)
        .map_or(0, |skill| skill.current)
}

/// Tops a pet essence's charges back up (`PetRefillRule`); it stands twice
/// in the pass, with the tight normal threshold and the looser idle one.
#[derive(Debug, Clone)]
pub struct PetRefillRule {
    status: String,
    now: f64,
    retry_at: f64,
    retry_holds_turn: bool,
}

impl Default for PetRefillRule {
    fn default() -> Self {
        Self {
            status: String::new(),
            now: 0.0,
            retry_at: f64::NEG_INFINITY,
            retry_holds_turn: false,
        }
    }
}

/// How long a refused refill waits before it is asked for again, seconds
/// (`PetRefillRule.RefusalRetrySeconds`).
pub const REFILL_REFUSAL_RETRY_SECONDS: f64 = 1.0;

impl PetRefillRule {
    /// A rule.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What it is doing.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Its turn (`Tick`): valid when an essence on the Items page is low and
    /// a spirit is at hand; it drops to peace first, then uses the spirit on
    /// the essence. A refused use waits out the retry clock; a busy refusal
    /// keeps the turn meanwhile, any other gives it up.
    pub fn tick(
        &mut self,
        automation: &mut dyn Automation,
        settings: &CombatSettings,
        threshold: i32,
        ready_in_peace: &mut dyn FnMut(&mut dyn Automation) -> bool,
        context: MacroPassContext,
    ) -> bool {
        self.now += context.elapsed_seconds.max(0.0);
        self.status.clear();
        if !automation.is_available() || !automation.items_available() {
            return false;
        }
        let owned = automation.owned_items();
        let Some((device, spirit)) = try_select_refill(automation, &owned, settings, threshold)
        else {
            return false;
        };
        if self.now < self.retry_at {
            self.status = format!("Waiting to refill {}", device.name);
            return self.retry_holds_turn;
        }
        if !context.can_act {
            return true;
        }
        if !ready_in_peace(automation) {
            "Entering peace mode to refill the combat pet".clone_into(&mut self.status);
            return true;
        }
        let result = automation.apply_item(spirit.object_id, device.object_id);
        if result.status == ItemStatus::Started {
            self.status = format!("Refilling {}", device.name);
            return true;
        }
        self.retry_at = self.now + REFILL_REFUSAL_RETRY_SECONDS;
        self.retry_holds_turn = result.status == ItemStatus::Busy;
        self.status = result.notice.unwrap_or_else(|| {
            format!(
                "Combat pet refill refused: {}",
                mosstank_automation::surface::item_status_name(result.status)
            )
        });
        self.retry_holds_turn
    }

    /// Forgets everything (`Reset`).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// How long a refused or failed summon waits before it is tried again,
/// seconds (`SummonPetRule.RefusalRetrySeconds`).
pub const SUMMON_REFUSAL_RETRY_SECONDS: f64 = 1.0;
/// How long a summon that went out waits for the server's answer, seconds
/// (`AnswerWaitSeconds`).
pub const SUMMON_ANSWER_WAIT_SECONDS: f64 = 5.0;

/// What the summon rule asks of its owner for one look.
#[allow(missing_debug_implementations)]
pub struct SummonTurn<'a> {
    /// The client.
    pub automation: &'a mut dyn Automation,
    /// The combat settings.
    pub settings: &'a CombatSettings,
    /// The element the attack would strike a monster with.
    pub attack_element: &'a mut dyn FnMut(&mut dyn Automation, &CombatTarget) -> MonsterDamageType,
    /// The elements the game database lists for a monster, best first.
    pub damage_preferences: &'a dyn Fn(&str) -> Vec<MonsterDamageType>,
    /// Where a once-per-cause warning goes.
    pub warn: &'a mut dyn FnMut(&mut dyn Automation, &str),
}

/// Summons a combat pet beside the main list (`SummonPetRule`).
#[derive(Debug, Clone)]
pub struct SummonPetRule {
    choice: PetSummonChoice,
    running: bool,
    status: String,
    decline_reason: Option<String>,
    now: f64,
    retry_at: f64,
    awaiting_device: u32,
    issue_revision: i64,
    issued_at: f64,
}

impl Default for SummonPetRule {
    fn default() -> Self {
        Self {
            choice: PetSummonChoice::default(),
            running: false,
            status: String::new(),
            decline_reason: None,
            now: 0.0,
            retry_at: f64::NEG_INFINITY,
            awaiting_device: 0,
            issue_revision: 0,
            issued_at: 0.0,
        }
    }
}

impl SummonPetRule {
    /// The rule's name.
    pub const NAME: &'static str = "SummonPet";

    /// A rule.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What it is doing.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Why it last declined, when its own clock held it.
    #[must_use]
    pub fn decline_reason(&self) -> Option<&str> {
        self.decline_reason.as_deref()
    }

    /// Whether it runs.
    #[must_use]
    pub fn running(&self) -> bool {
        self.running
    }

    /// Whether it would summon now (`ValidNow`): the choice is made here
    /// and nothing is issued.
    pub fn valid_now(&mut self, turn: &mut SummonTurn<'_>, context: MacroPassContext) -> bool {
        self.now += context.elapsed_seconds.max(0.0);
        self.decline_reason = None;
        self.choice = PetSummonChoice::default();
        let automation = &mut *turn.automation;
        if !automation.is_available() || !automation.items_available() {
            return false;
        }
        let completion = automation.last_item_completion();
        self.observe_answer(&completion);
        if !turn.settings.enabled || !turn.settings.summon_pets {
            return false;
        }
        if !is_summoning_trained(automation) {
            return false;
        }
        if automation
            .spells()
            .cooldown_remaining(SUMMONING_COOLDOWN_ID)
            > 0.0
        {
            return false;
        }
        if self.awaiting_device != 0 {
            self.decline_reason =
                Some("waiting for the server to answer the last summon".to_owned());
            return false;
        }
        if self.now < self.retry_at {
            self.decline_reason = Some("a refused summon is waiting to be tried again".to_owned());
            return false;
        }
        let owned = automation.owned_items();
        let targets = automation.hostile_targets(pet_range(turn.settings));
        let character = automation.character();
        let summoning = summoning_level(automation);
        self.choice = select_pet(
            automation,
            &owned,
            &targets,
            &character,
            summoning,
            turn.settings,
            turn.attack_element,
            turn.damage_preferences,
            turn.warn,
        );
        if self.choice.is_none() {
            return false;
        }
        // Last, as in the reference: is there room for the pet to appear?
        // A client that cannot tell counts as room.
        if turn.automation.check_room_ahead(PET_ROOM_AHEAD_METERS) == RoomAhead::Blocked {
            self.choice = PetSummonChoice::default();
            return false;
        }
        true
    }

    /// Runs or stops (`Running`): on run, uses the chosen essence.
    pub fn set_running(&mut self, automation: &mut dyn Automation, running: bool) {
        self.running = running;
        if !running {
            self.status.clear();
            return;
        }
        if self.choice.is_none() {
            return;
        }
        let revision = automation.last_item_completion().revision;
        let result = automation.use_item(self.choice.device.object_id);
        if result.status == ItemStatus::Started {
            self.awaiting_device = self.choice.device.object_id;
            self.issue_revision = revision;
            self.issued_at = self.now;
            self.status = format!(
                "Summoning {} for {}",
                self.choice.device.name, self.choice.target.name
            );
            return;
        }
        self.retry_at = self.now + SUMMON_REFUSAL_RETRY_SECONDS;
        self.status = result.notice.unwrap_or_else(|| {
            format!(
                "Combat pet summon refused: {}",
                mosstank_automation::surface::item_status_name(result.status)
            )
        });
    }

    /// Forgets everything (`Reset`).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Folds the server's answer to the summon that went out into the retry
    /// clock (`ObserveAnswer`).
    fn observe_answer(&mut self, completion: &ItemUseCompletion) {
        if self.awaiting_device == 0 {
            return;
        }
        if completion.revision > self.issue_revision && completion.source_id == self.awaiting_device
        {
            self.awaiting_device = 0;
            if completion.weenie_error != 0 {
                self.retry_at = self.now + SUMMON_REFUSAL_RETRY_SECONDS;
            }
            return;
        }
        if self.now - self.issued_at >= SUMMON_ANSWER_WAIT_SECONDS {
            self.awaiting_device = 0;
        }
    }
}

fn is_summoning_trained(automation: &mut dyn Automation) -> bool {
    automation
        .try_get_skill(SUMMONING_SKILL_ID)
        .is_some_and(|skill| matches!(skill.training, Training::Trained | Training::Specialized))
}
