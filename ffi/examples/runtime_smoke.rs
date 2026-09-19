fn main() {
    let controller = voicefox_ffi::api::new_desktop().expect("desktop runtime");
    let state = voicefox_ffi::api::state(&controller);
    println!(
        "runtime smoke: state={}, queue={}",
        state.playback.state,
        state.queue.len()
    );
}
