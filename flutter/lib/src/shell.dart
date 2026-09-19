import "package:flutter/material.dart";
import "app.dart";
import "bridge/lib.dart";

class VoicefoxShell extends StatefulWidget {
  const VoicefoxShell({super.key, required this.controller});
  final VoicefoxAppController controller;
  @override
  State<VoicefoxShell> createState() => _VoicefoxShellState();
}

class _VoicefoxShellState extends State<VoicefoxShell> {
  int _index = 0;
  final _search = TextEditingController();
  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
        animation: widget.controller,
        builder: (context, _) => LayoutBuilder(builder: (context, constraints) {
              final wide = constraints.maxWidth >= 900;
              final pages = [
                _HomePage(controller: widget.controller),
                _SearchPage(controller: widget.controller, field: _search),
                _QueuePage(controller: widget.controller),
                _LyricsPage(controller: widget.controller),
                _PlaylistPage(controller: widget.controller),
              ];
              return Scaffold(
                body: SafeArea(
                    child: Row(children: [
                  if (wide)
                    _Rail(
                        index: _index,
                        onChanged: (v) => setState(() => _index = v)),
                  Expanded(child: pages[_index]),
                ])),
                bottomNavigationBar: wide
                    ? MiniPlayer(controller: widget.controller)
                    : Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          MiniPlayer(controller: widget.controller),
                          NavigationBar(
                              selectedIndex: _index,
                              onDestinationSelected: (v) =>
                                  setState(() => _index = v),
                              destinations: const [
                                NavigationDestination(
                                    icon: Icon(Icons.home_outlined),
                                    selectedIcon: Icon(Icons.home),
                                    label: "首页"),
                                NavigationDestination(
                                    icon: Icon(Icons.search), label: "搜索"),
                                NavigationDestination(
                                    icon: Icon(Icons.queue_music), label: "队列"),
                                NavigationDestination(
                                    icon: Icon(Icons.lyrics_outlined),
                                    label: "歌词"),
                                NavigationDestination(
                                    icon: Icon(Icons.library_music_outlined),
                                    label: "歌单"),
                              ]),
                        ],
                      ),
              );
            }));
  }
}

class _Rail extends StatelessWidget {
  const _Rail({required this.index, required this.onChanged});
  final int index;
  final ValueChanged<int> onChanged;
  @override
  Widget build(BuildContext context) => NavigationRail(
          selectedIndex: index,
          onDestinationSelected: onChanged,
          labelType: NavigationRailLabelType.all,
          destinations: const [
            NavigationRailDestination(
                icon: Icon(Icons.home_outlined),
                selectedIcon: Icon(Icons.home),
                label: Text("首页")),
            NavigationRailDestination(
                icon: Icon(Icons.search), label: Text("搜索")),
            NavigationRailDestination(
                icon: Icon(Icons.queue_music), label: Text("队列")),
            NavigationRailDestination(
                icon: Icon(Icons.lyrics_outlined), label: Text("歌词")),
            NavigationRailDestination(
                icon: Icon(Icons.library_music_outlined), label: Text("歌单")),
          ]);
}

class _Page extends StatelessWidget {
  const _Page({required this.title, required this.child, this.action});
  final String title;
  final Widget child;
  final Widget? action;
  @override
  Widget build(BuildContext context) => CustomScrollView(slivers: [
        SliverAppBar.medium(
            title: Text(title), actions: action == null ? null : [action!]),
        SliverPadding(
            padding: const EdgeInsets.fromLTRB(20, 8, 20, 120),
            sliver: SliverToBoxAdapter(child: child)),
      ]);
}

class _HomePage extends StatelessWidget {
  const _HomePage({required this.controller});
  final VoicefoxAppController controller;
  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    final song = state?.playback.song;
    return _Page(
        title: "Voicefox",
        child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(song == null ? "准备播放" : "正在播放",
              style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 12),
          _NowPlayingCard(controller: controller),
          const SizedBox(height: 24),
          Text("最近队列", style: Theme.of(context).textTheme.titleLarge),
          const SizedBox(height: 8),
          if (state == null || state.queue.isEmpty)
            const _EmptyState(icon: Icons.queue_music, text: "队列还是空的")
          else
            ...state.queue.take(8).map((song) => _SongTile(
                song: song,
                onPlay: () => controller.play(song),
                onAdd: () => controller.addToQueue(song))),
        ]));
  }
}

class _SearchPage extends StatelessWidget {
  const _SearchPage({required this.controller, required this.field});
  final VoicefoxAppController controller;
  final TextEditingController field;
  @override
  Widget build(BuildContext context) {
    final search = controller.state?.search;
    return _Page(
        title: "搜索",
        child: Column(children: [
          TextField(
              controller: field,
              textInputAction: TextInputAction.search,
              onSubmitted: controller.search,
              decoration: InputDecoration(
                  hintText: "搜索歌曲、歌手或专辑",
                  prefixIcon: const Icon(Icons.search),
                  suffixIcon: IconButton(
                      onPressed: () => controller.search(field.text),
                      icon: const Icon(Icons.arrow_forward)),
                  border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(18)))),
          const SizedBox(height: 20),
          if (controller.searching) const LinearProgressIndicator(minHeight: 2),
          if (search != null && search.items.isNotEmpty)
            ...search.items.map((song) => _SongTile(
                song: song,
                onPlay: () => controller.play(song),
                onAdd: () => controller.addToQueue(song))),
          if (search != null &&
              search.items.isEmpty &&
              !controller.searching &&
              search.keyword.isNotEmpty)
            const _EmptyState(icon: Icons.search_off, text: "没有找到结果"),
          if (search?.hasMore == true)
            Padding(
                padding: const EdgeInsets.only(top: 12),
                child: OutlinedButton(
                    onPressed: controller.searchMore,
                    child: const Text("加载更多"))),
        ]));
  }
}

class _QueuePage extends StatelessWidget {
  const _QueuePage({required this.controller});
  final VoicefoxAppController controller;
  @override
  Widget build(BuildContext context) {
    final queue = controller.state?.queue ?? const <SongDto>[];
    return _Page(
        title: "播放队列",
        action: queue.isEmpty
            ? null
            : TextButton(
                onPressed: controller.clearQueue, child: const Text("清空")),
        child: queue.isEmpty
            ? const _EmptyState(icon: Icons.queue_music, text: "播放队列为空")
            : Column(children: [
                for (var i = 0; i < queue.length; i++)
                  _QueueTile(
                      index: i,
                      song: queue[i],
                      active: i == controller.state?.queueIndex,
                      onPlay: () => controller.play(queue[i]),
                      onRemove: () => controller.removeQueue(i)),
              ]));
  }
}

class _LyricsPage extends StatelessWidget {
  const _LyricsPage({required this.controller});
  final VoicefoxAppController controller;
  @override
  Widget build(BuildContext context) {
    final state = controller.state;
    final lyrics = state?.lyrics;
    final song = state?.playback.song;
    if (lyrics == null || lyrics.isEmpty)
      return _Page(
          title: "歌词",
          child: _EmptyState(
              icon: Icons.lyrics_outlined,
              text: song == null ? "开始播放后显示歌词" : "暂无歌词"));
    return _Page(
        title: song?.name ?? "歌词",
        child: Column(children: [
          for (var i = 0; i < lyrics.lines.length; i++)
            Padding(
                padding: const EdgeInsets.symmetric(vertical: 7),
                child: Text(lyrics.lines[i].text,
                    textAlign: TextAlign.center,
                    style: Theme.of(context).textTheme.bodyLarge?.copyWith(
                        fontWeight: i == lyrics.currentLine
                            ? FontWeight.w700
                            : FontWeight.normal,
                        color: i == lyrics.currentLine
                            ? Theme.of(context).colorScheme.primary
                            : null))),
        ]));
  }
}

class _PlaylistPage extends StatelessWidget {
  const _PlaylistPage({required this.controller});
  final VoicefoxAppController controller;
  @override
  Widget build(BuildContext context) {
    final playlists = controller.state?.playlists ?? const <PlaylistDto>[];
    return _Page(
        title: "歌单",
        child: playlists.isEmpty
            ? const _EmptyState(
                icon: Icons.library_music_outlined, text: "暂无歌单数据")
            : GridView.builder(
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
                    maxCrossAxisExtent: 260,
                    mainAxisExtent: 150,
                    crossAxisSpacing: 14,
                    mainAxisSpacing: 14),
                itemCount: playlists.length,
                itemBuilder: (context, index) =>
                    _PlaylistCard(playlist: playlists[index])));
  }
}

class _NowPlayingCard extends StatelessWidget {
  const _NowPlayingCard({required this.controller});
  final VoicefoxAppController controller;
  @override
  Widget build(BuildContext context) {
    final song = controller.state?.playback.song;
    return Card(
        child: Padding(
            padding: const EdgeInsets.all(18),
            child: Row(children: [
              _Cover(url: song?.coverUrl, size: 88),
              const SizedBox(width: 16),
              Expanded(
                  child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                    Text(song?.name ?? "未播放",
                        style: Theme.of(context).textTheme.titleLarge,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis),
                    Text(song?.singer ?? "Voicefox",
                        maxLines: 1, overflow: TextOverflow.ellipsis),
                    const SizedBox(height: 12),
                    Row(children: [
                      IconButton(
                          onPressed: controller.previous,
                          icon: const Icon(Icons.skip_previous)),
                      FilledButton.tonal(
                          onPressed: controller.toggle,
                          child: Icon(
                              controller.state?.playback.state == "playing"
                                  ? Icons.pause
                                  : Icons.play_arrow)),
                      IconButton(
                          onPressed: controller.next,
                          icon: const Icon(Icons.skip_next)),
                    ]),
                  ])),
            ])));
  }
}

class _SongTile extends StatelessWidget {
  const _SongTile(
      {required this.song, required this.onPlay, required this.onAdd});
  final SongDto song;
  final VoidCallback onPlay;
  final VoidCallback onAdd;
  @override
  Widget build(BuildContext context) => Card(
      child: ListTile(
          leading: _Cover(url: song.coverUrl, size: 48),
          title: Text(song.name, maxLines: 1, overflow: TextOverflow.ellipsis),
          subtitle: Text(song.singer + " · " + song.source,
              maxLines: 1, overflow: TextOverflow.ellipsis),
          onTap: onPlay,
          trailing: IconButton(
              onPressed: onAdd, tooltip: "加入队列", icon: const Icon(Icons.add))));
}

class _QueueTile extends StatelessWidget {
  const _QueueTile(
      {required this.index,
      required this.song,
      required this.active,
      required this.onPlay,
      required this.onRemove});
  final int index;
  final SongDto song;
  final bool active;
  final VoidCallback onPlay;
  final VoidCallback onRemove;
  @override
  Widget build(BuildContext context) => Card(
      color: active ? Theme.of(context).colorScheme.primaryContainer : null,
      child: ListTile(
          leading: CircleAvatar(child: Text((index + 1).toString())),
          title: Text(song.name, maxLines: 1, overflow: TextOverflow.ellipsis),
          subtitle: Text(song.singer),
          onTap: onPlay,
          trailing:
              IconButton(onPressed: onRemove, icon: const Icon(Icons.close))));
}

class _PlaylistCard extends StatelessWidget {
  const _PlaylistCard({required this.playlist});
  final PlaylistDto playlist;
  @override
  Widget build(BuildContext context) => Card(
      child: Padding(
          padding: const EdgeInsets.all(14),
          child: Row(children: [
            _Cover(url: playlist.coverUrl, size: 62),
            const SizedBox(width: 12),
            Expanded(
                child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                  Text(playlist.name,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.titleMedium),
                  const SizedBox(height: 5),
                  Text(playlist.songCount.toString() +
                      " 首 · " +
                      playlist.source),
                ])),
          ])));
}

class _Cover extends StatelessWidget {
  const _Cover({required this.url, required this.size});
  final String? url;
  final double size;
  @override
  Widget build(BuildContext context) => ClipRRect(
      borderRadius: BorderRadius.circular(12),
      child: SizedBox(
          width: size,
          height: size,
          child: url == null || url!.isEmpty
              ? ColoredBox(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                  child: const Icon(Icons.music_note))
              : Image.network(url!,
                  fit: BoxFit.cover,
                  errorBuilder: (_, __, ___) => ColoredBox(
                      color:
                          Theme.of(context).colorScheme.surfaceContainerHighest,
                      child: const Icon(Icons.music_note)))));
}

class _EmptyState extends StatelessWidget {
  const _EmptyState({required this.icon, required this.text});
  final IconData icon;
  final String text;
  @override
  Widget build(BuildContext context) => SizedBox(
      width: double.infinity,
      height: 180,
      child: Center(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
        Icon(icon, size: 42),
        const SizedBox(height: 10),
        Text(text),
      ])));
}

class MiniPlayer extends StatelessWidget {
  const MiniPlayer({super.key, required this.controller});
  final VoicefoxAppController controller;
  @override
  Widget build(BuildContext context) {
    final playback = controller.state?.playback;
    final song = playback?.song;
    return Material(
        elevation: 8,
        child: SafeArea(
            top: false,
            child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
                child: Row(children: [
                  _Cover(url: song?.coverUrl, size: 48),
                  const SizedBox(width: 12),
                  Expanded(
                      child: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                        Text(song?.name ?? "未播放",
                            maxLines: 1, overflow: TextOverflow.ellipsis),
                        Text(song?.singer ?? "Voicefox",
                            maxLines: 1, overflow: TextOverflow.ellipsis),
                      ])),
                  IconButton(
                      onPressed: controller.previous,
                      icon: const Icon(Icons.skip_previous)),
                  IconButton(
                      onPressed: controller.toggle,
                      icon: Icon(playback?.state == "playing"
                          ? Icons.pause
                          : Icons.play_arrow)),
                  IconButton(
                      onPressed: controller.next,
                      icon: const Icon(Icons.skip_next)),
                ]))));
  }
}
