//! `/ub clearbugged`, against `ClearBuggedRunTests`: a client and a server
//! in one fake, the single appraisal slot answered by `deliver`.

use std::collections::{HashMap, HashSet};

use ac_plugin_abi::items::{AppraisalState, InventoryItem, ItemResult, ItemStatus};
use mosstank_ub::tools::clear_bugged::{
    ClearBuggedClient, ClearBuggedRun, IDENTIFY_INTERVAL_SECONDS, NO_PROGRESS_TIMEOUT_SECONDS,
    RECHECK_DELAY_SECONDS, WAITING_LINE_INTERVAL_SECONDS,
};
use openac_plugin_compat::regex::{DotNetRegex, RegexOptions};

const FRAME: f64 = 0.1;
const OWN_ID: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerAnswer {
    /// Described every time.
    Described,
    /// The server has no such item: refused every time.
    Gone,
    /// Resists appraisal: refused every time.
    Resists,
    /// Refused on the first request, described after.
    RefusedOnce,
}

#[derive(Debug, Default)]
struct Fake {
    answers: HashMap<u32, ServerAnswer>,
    asked: HashMap<u32, i32>,
    last_refused: HashSet<u32>,
    slot: AppraisalState,
    now: f64,
    owned: Vec<InventoryItem>,
    requests: Vec<(u32, f64)>,
    answered: Vec<(u32, f64)>,
    forgotten: Vec<u32>,
    messages: Vec<String>,
    slot_busy: bool,
    items_busy: bool,
    answer_nothing: bool,
    unavailable: bool,
}

impl Fake {
    fn add(&mut self, item: InventoryItem, answer: ServerAnswer) {
        self.answers.insert(item.object_id, answer);
        self.owned.push(item);
    }

    /// The client stops reporting a wait that ran out; nothing else changes.
    fn expire_wait(&mut self) {
        self.slot.awaiting_id = 0;
    }

    fn deliver(&mut self) {
        let id = self.slot.awaiting_id;
        if id == 0 || self.answer_nothing {
            return;
        }
        let asked = self.asked[&id];
        let described = match self.answers[&id] {
            ServerAnswer::Described => true,
            ServerAnswer::RefusedOnce => asked > 1,
            ServerAnswer::Gone | ServerAnswer::Resists => false,
        };
        if described {
            self.last_refused.remove(&id);
        } else {
            self.last_refused.insert(id);
        }
        self.slot = AppraisalState {
            revision: self.slot.revision + 1,
            awaiting_id: 0,
            current_id: id,
            last_abandoned_id: 0,
            current_object_unsuccessful: !described,
        };
        self.answered.push((id, self.now));
    }
}

impl ClearBuggedClient for Fake {
    fn in_world(&mut self) -> bool {
        !self.unavailable
    }

    fn own_object_id(&mut self) -> u32 {
        OWN_ID
    }

    fn owned_items(&mut self) -> Vec<InventoryItem> {
        self.owned.clone()
    }

    fn items_busy(&mut self) -> bool {
        self.items_busy
    }

    fn identify(&mut self, object_id: u32) -> ItemResult {
        if self.slot_busy {
            return ItemResult::of(ItemStatus::Busy);
        }
        self.requests.push((object_id, self.now));
        *self.asked.entry(object_id).or_default() += 1;
        self.slot.revision += 1;
        self.slot.awaiting_id = object_id;
        ItemResult::of(ItemStatus::Started)
    }

    fn appraisal(&mut self) -> AppraisalState {
        self.slot.clone()
    }

    fn forget_stale_item(&mut self, object_id: u32) -> ItemResult {
        if self.items_busy {
            return ItemResult::of(ItemStatus::Busy);
        }
        let Some(index) = self
            .owned
            .iter()
            .position(|item| item.object_id == object_id)
        else {
            return ItemResult::of(ItemStatus::InvalidItem);
        };
        if self.owned[index].equipped_location != 0 || !self.last_refused.contains(&object_id) {
            return ItemResult::because(ItemStatus::Refused, "not refused");
        }
        self.owned.remove(index);
        self.forgotten.push(object_id);
        ItemResult::of(ItemStatus::Completed)
    }
}

fn item(id: u32, name: &str, container: u32, wielder: u32, equipped: u32) -> InventoryItem {
    InventoryItem {
        object_id: id,
        name: name.to_owned(),
        container_id: container,
        wielder_id: wielder,
        equipped_location: equipped,
        stack_size: 1,
        ..InventoryItem::default()
    }
}

fn carried(id: u32, name: &str) -> InventoryItem {
    item(id, name, OWN_ID, 0, 0)
}

fn start(run: &mut ClearBuggedRun, fake: &mut Fake) {
    let mut lines = Vec::new();
    run.start(fake, &mut |line| lines.push(line));
    fake.messages.extend(lines);
}

fn drive(run: &mut ClearBuggedRun, fake: &mut Fake, seconds: f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (seconds / FRAME).round() as usize;
    for _ in 0..frames {
        fake.now += FRAME;
        let mut lines = Vec::new();
        run.tick(fake, FRAME, &mut |line| lines.push(line));
        fake.messages.extend(lines);
        fake.deliver();
    }
}

fn requested(fake: &Fake) -> Vec<u32> {
    fake.requests.iter().map(|(id, _)| *id).collect()
}

fn strings(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_owned()).collect()
}

const DONE: &str = "[UB] InventoryManager: ClearBugged: Done!";

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: AGhostRefusedTwiceFiveSecondsApartIsLetGoOf
#[test]
fn a_ghost_refused_twice_five_seconds_apart_is_let_go_of() {
    let mut fake = Fake::default();
    fake.add(carried(10, "Phantom Pack"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 20.0);

    assert_eq!(vec![10, 10], requested(&fake));
    assert!(fake.requests[1].1 - fake.answered[0].1 >= RECHECK_DELAY_SECONDS - 1e-9);
    assert_eq!(vec![10], fake.forgotten);
    assert!(fake.owned.is_empty());
    assert_eq!(
        strings(&[
            "[UB] InventoryManager: ClearBugged: Identifying 1 items, to check for bugged items...",
            "[UB] new Assessor.Job initialized with 1 items...",
            "[UB] Assessor waiting to ID 1 of 1 items. This will take about 0.50 seconds.",
            "[UB] Error: Assessor: Bugged Item: 0x0000000A Phantom Pack",
            DONE,
        ]),
        fake.messages
    );
    assert!(!run.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: TheAppraiserSaysWhatIsLeftEveryTenSeconds
#[test]
fn the_appraiser_says_what_is_left_every_ten_seconds() {
    let mut fake = Fake::default();
    for id in 100..150 {
        fake.add(carried(id, "Gem"), ServerAnswer::Described);
    }
    let mut run = ClearBuggedRun::new();
    let mut said: Vec<(f64, String)> = Vec::new();

    let mut lines = Vec::new();
    run.start(&mut fake, &mut |line| lines.push(line));
    said.extend(lines.into_iter().map(|line| (fake.now, line)));
    for _ in 0..600 {
        fake.now += FRAME;
        let mut lines = Vec::new();
        run.tick(&mut fake, FRAME, &mut |line| lines.push(line));
        said.extend(lines.into_iter().map(|line| (fake.now, line)));
        fake.deliver();
    }

    assert_eq!(
        "[UB] new Assessor.Job initialized with 50 items...",
        said[1].1
    );
    let waiting: Vec<&(f64, String)> = said
        .iter()
        .filter(|(_, line)| line.starts_with("[UB] Assessor waiting to ID "))
        .collect();
    assert_eq!(
        "[UB] Assessor waiting to ID 50 of 50 items. This will take about 24.95 seconds.",
        waiting[0].1
    );
    assert_eq!(3, waiting.len());
    for pair in waiting.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        assert!(gap >= WAITING_LINE_INTERVAL_SECONDS - 1e-9);
        assert!(gap <= WAITING_LINE_INTERVAL_SECONDS + FRAME + 1e-9);
    }
    let pattern = DotNetRegex::new(
        r"^\[UB\] Assessor waiting to ID \d+ of 50 items\. This will take about \d+\.\d\d seconds\.$",
        RegexOptions::NONE,
    )
    .unwrap();
    assert!(pattern.is_match(&waiting[waiting.len() - 1].1).unwrap());
    assert_eq!(DONE, said[said.len() - 1].1);
    assert!(!run.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: AnItemTheServerDescribesIsKept
#[test]
fn an_item_the_server_describes_is_kept() {
    let mut fake = Fake::default();
    fake.add(carried(11, "Pyreal"), ServerAnswer::Described);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 20.0);

    assert_eq!(vec![11], requested(&fake));
    assert!(fake.forgotten.is_empty());
    assert_eq!(DONE, fake.messages[fake.messages.len() - 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: AnItemRefusedOnceAndDescribedTheSecondTimeIsKept
#[test]
fn an_item_refused_once_and_described_the_second_time_is_kept() {
    let mut fake = Fake::default();
    fake.add(carried(12, "Stubborn Gem"), ServerAnswer::RefusedOnce);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 20.0);

    assert_eq!(vec![12, 12], requested(&fake));
    assert!(fake.forgotten.is_empty());
    assert_eq!(1, fake.owned.len());
    assert_eq!(DONE, fake.messages[fake.messages.len() - 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: AnItemThatResistsEveryTimeIsLetGoOfLikeAGhost
#[test]
fn an_item_that_resists_every_time_is_let_go_of_like_a_ghost() {
    let mut fake = Fake::default();
    fake.add(carried(13, "Sealed Chest"), ServerAnswer::Resists);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 20.0);

    assert_eq!(vec![13], fake.forgotten);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: WornWieldedAndFilledPacksAreLeftAlone
#[test]
fn worn_wielded_and_filled_packs_are_left_alone() {
    let mut fake = Fake::default();
    fake.add(item(20, "Worn Robe", OWN_ID, 0, 0x200), ServerAnswer::Gone);
    fake.add(
        item(21, "Wielded Sword", OWN_ID, OWN_ID, 0),
        ServerAnswer::Gone,
    );
    fake.add(carried(22, "Full Pack"), ServerAnswer::Gone);
    fake.add(item(23, "Pack Content", 22, 0, 0), ServerAnswer::Gone);
    fake.add(carried(24, "Empty Pack"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 30.0);

    assert_eq!(
        "[UB] InventoryManager: ClearBugged: Identifying 5 items, to check for bugged items...",
        fake.messages[0]
    );
    assert!(!requested(&fake).iter().any(|id| matches!(id, 20..=22)));
    let mut forgotten = fake.forgotten.clone();
    forgotten.sort_unstable();
    assert_eq!(vec![23, 24], forgotten);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: TheSecondRequestWaitsFiveSecondsAfterTheFirstRefusal
#[test]
fn the_second_request_waits_five_seconds_after_the_first_refusal() {
    let mut fake = Fake::default();
    fake.add(carried(30, "Phantom"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 1.0);
    assert_eq!(1, fake.answered.len());
    let refused_at = fake.answered[0].1;
    let wait = refused_at + 4.9 - fake.now;
    drive(&mut run, &mut fake, wait);
    assert_eq!(1, fake.requests.len());
    assert!(fake.forgotten.is_empty());

    drive(&mut run, &mut fake, 1.0);
    assert_eq!(2, fake.requests.len());
    assert_eq!(vec![30], fake.forgotten);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: RequestsArePacedOnTheIdentifyCadence
#[test]
fn requests_are_paced_on_the_identify_cadence() {
    let mut fake = Fake::default();
    for id in 40..46 {
        fake.add(carried(id, "Real"), ServerAnswer::Described);
    }
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 10.0);

    assert_eq!(6, fake.requests.len());
    for pair in fake.requests.windows(2) {
        assert!(pair[1].1 - pair[0].1 >= IDENTIFY_INTERVAL_SECONDS - 1e-9);
    }
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: AnUnansweredSecondRequestIsNotTakenForASecondRefusal
#[test]
fn an_unanswered_second_request_is_not_taken_for_a_second_refusal() {
    let mut fake = Fake::default();
    fake.add(carried(50, "Phantom"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 1.0);
    fake.answer_nothing = true;
    drive(&mut run, &mut fake, 6.0);
    assert_eq!(2, fake.requests.len());
    fake.expire_wait();
    drive(&mut run, &mut fake, 15.0);

    assert!(fake.forgotten.is_empty());
    assert_eq!(DONE, fake.messages[fake.messages.len() - 1]);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: ABusySlotIsAskedAgainLater
#[test]
fn a_busy_slot_is_asked_again_later() {
    let mut fake = Fake {
        slot_busy: true,
        ..Fake::default()
    };
    fake.add(carried(60, "Phantom"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 2.0);
    assert!(fake.requests.is_empty());
    fake.slot_busy = false;
    drive(&mut run, &mut fake, 20.0);

    assert_eq!(vec![60], fake.forgotten);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: ASecondStartWhileRunningIsRefused
#[test]
fn a_second_start_while_running_is_refused() {
    let mut fake = Fake::default();
    fake.add(carried(70, "Real"), ServerAnswer::Described);
    fake.add(carried(71, "Real"), ServerAnswer::Described);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 0.2);
    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 10.0);

    assert_eq!(
        "[UB] Error: InventoryManager: ClearBugged: already running.",
        fake.messages[3]
    );
    assert_eq!(vec![70, 71], requested(&fake));
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: AResetRunStopsWithoutAWord
#[test]
fn a_reset_run_stops_without_a_word() {
    let mut fake = Fake::default();
    fake.add(carried(80, "Phantom"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 1.0);
    run.reset();
    let said = fake.messages.len();
    drive(&mut run, &mut fake, 20.0);

    assert_eq!(1, fake.requests.len());
    assert!(fake.forgotten.is_empty());
    assert_eq!(said, fake.messages.len());
    assert!(!run.is_running());
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: ForgettingWaitsForAFreeClient
#[test]
fn forgetting_waits_for_a_free_client() {
    let mut fake = Fake::default();
    fake.add(carried(90, "Phantom"), ServerAnswer::Gone);
    let mut run = ClearBuggedRun::new();

    start(&mut run, &mut fake);
    drive(&mut run, &mut fake, 1.0);
    fake.items_busy = true;
    drive(&mut run, &mut fake, 8.0);
    assert!(fake.forgotten.is_empty());
    assert!(run.is_running());
    fake.items_busy = false;
    drive(&mut run, &mut fake, 1.0);

    assert_eq!(vec![90], fake.forgotten);
}

// Source: tests/AcDream.Plugins.MossTank.Tests/ClearBuggedRunTests.cs :: ARunThatMakesNoProgressEndsOnItsOwn
#[test]
fn a_run_that_makes_no_progress_ends_on_its_own() {
    for slot_busy in [true, false] {
        let mut fake = Fake {
            slot_busy,
            ..Fake::default()
        };
        fake.add(carried(100, "Phantom"), ServerAnswer::Gone);
        let mut run = ClearBuggedRun::new();

        start(&mut run, &mut fake);
        fake.unavailable = !slot_busy;
        drive(&mut run, &mut fake, NO_PROGRESS_TIMEOUT_SECONDS - 1.0);
        assert!(run.is_running());
        drive(&mut run, &mut fake, 30.0);

        assert!(!run.is_running());
        assert!(fake.forgotten.is_empty());
        assert_eq!(DONE, fake.messages[fake.messages.len() - 1]);
    }
}
