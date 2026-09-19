//! flutter_rust_bridge entry points.
//!
//! The generated Dart bindings should expose only DTOs/commands/streams from
//! this module; ApplicationService itself never crosses the Flutter ABI.

pub use crate::{
    FfiCommand, LyricLineDto, LyricsDto, PlaylistDto, SearchDto, SongDto, VoicefoxController,
    VoicefoxEvent, VoicefoxEventStream, VoicefoxState,
};

pub fn command_pause() -> FfiCommand {
    FfiCommand::Pause
}
pub fn command_resume() -> FfiCommand {
    FfiCommand::Resume
}
pub fn command_toggle() -> FfiCommand {
    FfiCommand::Toggle
}
pub fn command_stop() -> FfiCommand {
    FfiCommand::Stop
}
pub fn command_next() -> FfiCommand {
    FfiCommand::Next
}
pub fn command_previous() -> FfiCommand {
    FfiCommand::Previous
}
pub fn command_search(keyword: String) -> FfiCommand {
    FfiCommand::Search { keyword }
}
pub fn command_search_more(keyword: String, page: u32) -> FfiCommand {
    FfiCommand::SearchMore { keyword, page }
}
pub fn command_play(song: SongDto) -> FfiCommand {
    FfiCommand::PlaySong { song }
}
pub fn command_queue_add(song: SongDto) -> FfiCommand {
    FfiCommand::QueueAdd { song, next: false }
}
pub fn command_queue_remove(index: u32) -> FfiCommand {
    FfiCommand::QueueRemove { index }
}
pub fn command_queue_clear() -> FfiCommand {
    FfiCommand::QueueClear
}

pub fn new_desktop() -> Result<VoicefoxController, String> {
    VoicefoxController::new_desktop()
}

pub fn state(controller: &VoicefoxController) -> VoicefoxState {
    controller.state()
}

pub async fn dispatch(controller: &VoicefoxController, command: FfiCommand) -> Result<(), String> {
    controller.dispatch(command).await
}

pub fn subscribe(controller: &VoicefoxController) -> VoicefoxEventStream {
    controller.subscribe()
}

pub async fn event_next(stream: &mut VoicefoxEventStream) -> Result<VoicefoxEvent, String> {
    stream.next().await
}
