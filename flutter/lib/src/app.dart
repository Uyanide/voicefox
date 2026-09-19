import "dart:async";
import "package:flutter/foundation.dart";
import "bridge/api.dart" as bridge;
import "bridge/lib.dart";

class VoicefoxAppController extends ChangeNotifier {
  VoicefoxAppController(this._controller);
  final VoicefoxController _controller;
  VoicefoxState? _state;
  VoicefoxState? get state => _state;
  VoicefoxEventStream? _events;
  String? _notification;
  String? get notification => _notification;
  bool _closed = false;
  bool _searching = false;
  bool get searching => _searching;

  Future<void> toggle() async => _dispatch(await bridge.commandToggle());
  Future<void> pause() async => _dispatch(await bridge.commandPause());
  Future<void> resume() async => _dispatch(await bridge.commandResume());
  Future<void> next() async => _dispatch(await bridge.commandNext());
  Future<void> previous() async => _dispatch(await bridge.commandPrevious());
  Future<void> _dispatch(FfiCommand command) =>
      bridge.dispatch(controller: _controller, command: command);

  Future<void> search(String keyword) async {
    final value = keyword.trim();
    if (value.isEmpty) return;
    _searching = true;
    notifyListeners();
    await _dispatch(await bridge.commandSearch(keyword: value));
  }

  Future<void> searchMore() async {
    final s = _state;
    if (s == null || !s.search.hasMore || s.search.keyword.isEmpty) return;
    await _dispatch(await bridge.commandSearchMore(
      keyword: s.search.keyword,
      page: s.search.page + 1,
    ));
  }

  Future<void> play(SongDto song) async =>
      _dispatch(await bridge.commandPlay(song: song));
  Future<void> addToQueue(SongDto song) async =>
      _dispatch(await bridge.commandQueueAdd(song: song));
  Future<void> removeQueue(int index) async =>
      _dispatch(await bridge.commandQueueRemove(index: index));
  Future<void> clearQueue() async =>
      _dispatch(await bridge.commandQueueClear());

  Future<void> initialize() async {
    _state = await bridge.state(controller: _controller);
    _events = await bridge.subscribe(controller: _controller);
    notifyListeners();
    unawaited(_pumpEvents());
  }

  Future<void> _pumpEvents() async {
    final events = _events;
    if (events == null) return;
    while (!_closed) {
      final event = await bridge.eventNext(stream: events);
      if (_closed) return;
      if (event.message != null) _notification = event.message;
      if (event.kind == "search")
        _searching = event.items.isEmpty && event.message == null;
      _state = await bridge.state(controller: _controller);
      notifyListeners();
    }
  }

  @override
  void dispose() {
    _closed = true;
    super.dispose();
  }
}
