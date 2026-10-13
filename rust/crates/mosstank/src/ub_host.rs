//! The UB tab's view of the client ([`UbHost`]): the calls only the UB
//! commands make, each one SDK call.

use mosstank_ub::host::{
    AllegianceResult, Floorplan, ItemResult, MoveChannel, MoveDirection, MovePace, MoveReport,
    MoveUnit, NavigationStatus, OptionResult, Skill, UbHost,
};

use crate::automation::HostAutomation;

impl UbHost for HostAutomation<'_, '_> {
    fn chat_add_interceptor(&mut self) -> Result<u32, String> {
        self.host.chat_add_interceptor()
    }

    fn chat_remove_interceptor(&mut self, handle: u32) -> bool {
        self.host.chat_remove_interceptor(handle)
    }

    fn face_heading(&mut self, heading_degrees: f32) -> NavigationStatus {
        self.host.face_heading(heading_degrees)
    }

    fn move_by(
        &mut self,
        direction: MoveDirection,
        pace: MovePace,
        amount: f32,
        unit: MoveUnit,
    ) -> NavigationStatus {
        self.host.move_by(direction, pace, amount, unit)
    }

    fn stop_moving_channel(&mut self, channel: MoveChannel) -> NavigationStatus {
        self.host.stop_moving_channel(channel)
    }

    fn jump(&mut self, power: f32) -> NavigationStatus {
        self.host.jump(power)
    }

    fn move_report(&mut self) -> MoveReport {
        self.host.move_report()
    }

    fn skills(&mut self) -> Vec<Skill> {
        self.host.skills()
    }

    fn character_options(&mut self) -> Vec<(String, bool)> {
        self.host.character_options()
    }

    fn set_character_option(&mut self, name: &str, value: bool) -> OptionResult {
        self.host.set_character_option(name, value)
    }

    fn allegiance_swear(&mut self, patron_id: u32) -> AllegianceResult {
        self.host.allegiance_swear(patron_id)
    }

    fn allegiance_break(&mut self, target_id: u32) -> AllegianceResult {
        self.host.allegiance_break(target_id)
    }

    fn answer_dialog(&mut self, context_id: u32, accept: bool) -> bool {
        self.host.answer_dialog(context_id, accept)
    }

    fn open_loot(&mut self, container_id: u32) -> ItemResult {
        self.host.open_loot(container_id)
    }

    fn close_loot(&mut self, container_id: u32) -> ItemResult {
        self.host.close_loot(container_id)
    }

    fn request_close(&mut self) -> bool {
        self.host.request_close()
    }

    fn broadcast_command(&mut self, line: &str, tags: &[String], delay_ms: i32) -> bool {
        self.host.broadcast_command(line, tags, delay_ms)
    }

    fn set_peer_tags(&mut self, tags: &[String]) -> bool {
        self.host.set_peer_tags(tags)
    }

    fn peer_subscribe(&mut self, capabilities: u32) -> Option<u32> {
        self.host.peer_subscribe(capabilities)
    }

    fn peer_unsubscribe(&mut self, handle: u32) -> bool {
        self.host.peer_unsubscribe(handle)
    }

    fn peers_connected(&mut self) -> bool {
        self.host.peers_connected()
    }

    fn is_sealed_dungeon(&mut self, cell_id: u32) -> bool {
        self.host.is_sealed_dungeon(cell_id)
    }

    fn dungeon_floorplan(&mut self, landblock_id: u32) -> Option<Floorplan> {
        self.host.dungeon_floorplan(landblock_id)
    }
}
