// GENERATED CODE - DO NOT MODIFY BY HAND
// coverage:ignore-file
// ignore_for_file: type=lint
// ignore_for_file: unused_element, deprecated_member_use, deprecated_member_use_from_same_package, use_function_type_syntax_for_parameters, unnecessary_const, avoid_init_to_null, invalid_override_different_default_values_named, prefer_expression_function_bodies, annotate_overrides, invalid_annotation_target, unnecessary_question_mark

part of 'lib.dart';

// **************************************************************************
// FreezedGenerator
// **************************************************************************

// dart format off
T _$identity<T>(T value) => value;

/// @nodoc
mixin _$FfiCommand {
  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand()';
  }
}

/// @nodoc
class $FfiCommandCopyWith<$Res> {
  $FfiCommandCopyWith(FfiCommand _, $Res Function(FfiCommand) __);
}

/// Adds pattern-matching-related methods to [FfiCommand].
extension FfiCommandPatterns on FfiCommand {
  /// A variant of `map` that fallback to returning `orElse`.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case _:
  ///     return orElse();
  /// }
  /// ```

  @optionalTypeArgs
  TResult maybeMap<TResult extends Object?>({
    TResult Function(FfiCommand_Pause value)? pause,
    TResult Function(FfiCommand_Resume value)? resume,
    TResult Function(FfiCommand_Toggle value)? toggle,
    TResult Function(FfiCommand_Stop value)? stop,
    TResult Function(FfiCommand_SeekMs value)? seekMs,
    TResult Function(FfiCommand_PlaySong value)? playSong,
    TResult Function(FfiCommand_QueueAdd value)? queueAdd,
    TResult Function(FfiCommand_QueueRemove value)? queueRemove,
    TResult Function(FfiCommand_QueueClear value)? queueClear,
    TResult Function(FfiCommand_Next value)? next,
    TResult Function(FfiCommand_Previous value)? previous,
    TResult Function(FfiCommand_Search value)? search,
    TResult Function(FfiCommand_SearchMore value)? searchMore,
    required TResult orElse(),
  }) {
    final _that = this;
    switch (_that) {
      case FfiCommand_Pause() when pause != null:
        return pause(_that);
      case FfiCommand_Resume() when resume != null:
        return resume(_that);
      case FfiCommand_Toggle() when toggle != null:
        return toggle(_that);
      case FfiCommand_Stop() when stop != null:
        return stop(_that);
      case FfiCommand_SeekMs() when seekMs != null:
        return seekMs(_that);
      case FfiCommand_PlaySong() when playSong != null:
        return playSong(_that);
      case FfiCommand_QueueAdd() when queueAdd != null:
        return queueAdd(_that);
      case FfiCommand_QueueRemove() when queueRemove != null:
        return queueRemove(_that);
      case FfiCommand_QueueClear() when queueClear != null:
        return queueClear(_that);
      case FfiCommand_Next() when next != null:
        return next(_that);
      case FfiCommand_Previous() when previous != null:
        return previous(_that);
      case FfiCommand_Search() when search != null:
        return search(_that);
      case FfiCommand_SearchMore() when searchMore != null:
        return searchMore(_that);
      case _:
        return orElse();
    }
  }

  /// A `switch`-like method, using callbacks.
  ///
  /// Callbacks receives the raw object, upcasted.
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case final Subclass2 value:
  ///     return ...;
  /// }
  /// ```

  @optionalTypeArgs
  TResult map<TResult extends Object?>({
    required TResult Function(FfiCommand_Pause value) pause,
    required TResult Function(FfiCommand_Resume value) resume,
    required TResult Function(FfiCommand_Toggle value) toggle,
    required TResult Function(FfiCommand_Stop value) stop,
    required TResult Function(FfiCommand_SeekMs value) seekMs,
    required TResult Function(FfiCommand_PlaySong value) playSong,
    required TResult Function(FfiCommand_QueueAdd value) queueAdd,
    required TResult Function(FfiCommand_QueueRemove value) queueRemove,
    required TResult Function(FfiCommand_QueueClear value) queueClear,
    required TResult Function(FfiCommand_Next value) next,
    required TResult Function(FfiCommand_Previous value) previous,
    required TResult Function(FfiCommand_Search value) search,
    required TResult Function(FfiCommand_SearchMore value) searchMore,
  }) {
    final _that = this;
    switch (_that) {
      case FfiCommand_Pause():
        return pause(_that);
      case FfiCommand_Resume():
        return resume(_that);
      case FfiCommand_Toggle():
        return toggle(_that);
      case FfiCommand_Stop():
        return stop(_that);
      case FfiCommand_SeekMs():
        return seekMs(_that);
      case FfiCommand_PlaySong():
        return playSong(_that);
      case FfiCommand_QueueAdd():
        return queueAdd(_that);
      case FfiCommand_QueueRemove():
        return queueRemove(_that);
      case FfiCommand_QueueClear():
        return queueClear(_that);
      case FfiCommand_Next():
        return next(_that);
      case FfiCommand_Previous():
        return previous(_that);
      case FfiCommand_Search():
        return search(_that);
      case FfiCommand_SearchMore():
        return searchMore(_that);
    }
  }

  /// A variant of `map` that fallback to returning `null`.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case final Subclass value:
  ///     return ...;
  ///   case _:
  ///     return null;
  /// }
  /// ```

  @optionalTypeArgs
  TResult? mapOrNull<TResult extends Object?>({
    TResult? Function(FfiCommand_Pause value)? pause,
    TResult? Function(FfiCommand_Resume value)? resume,
    TResult? Function(FfiCommand_Toggle value)? toggle,
    TResult? Function(FfiCommand_Stop value)? stop,
    TResult? Function(FfiCommand_SeekMs value)? seekMs,
    TResult? Function(FfiCommand_PlaySong value)? playSong,
    TResult? Function(FfiCommand_QueueAdd value)? queueAdd,
    TResult? Function(FfiCommand_QueueRemove value)? queueRemove,
    TResult? Function(FfiCommand_QueueClear value)? queueClear,
    TResult? Function(FfiCommand_Next value)? next,
    TResult? Function(FfiCommand_Previous value)? previous,
    TResult? Function(FfiCommand_Search value)? search,
    TResult? Function(FfiCommand_SearchMore value)? searchMore,
  }) {
    final _that = this;
    switch (_that) {
      case FfiCommand_Pause() when pause != null:
        return pause(_that);
      case FfiCommand_Resume() when resume != null:
        return resume(_that);
      case FfiCommand_Toggle() when toggle != null:
        return toggle(_that);
      case FfiCommand_Stop() when stop != null:
        return stop(_that);
      case FfiCommand_SeekMs() when seekMs != null:
        return seekMs(_that);
      case FfiCommand_PlaySong() when playSong != null:
        return playSong(_that);
      case FfiCommand_QueueAdd() when queueAdd != null:
        return queueAdd(_that);
      case FfiCommand_QueueRemove() when queueRemove != null:
        return queueRemove(_that);
      case FfiCommand_QueueClear() when queueClear != null:
        return queueClear(_that);
      case FfiCommand_Next() when next != null:
        return next(_that);
      case FfiCommand_Previous() when previous != null:
        return previous(_that);
      case FfiCommand_Search() when search != null:
        return search(_that);
      case FfiCommand_SearchMore() when searchMore != null:
        return searchMore(_that);
      case _:
        return null;
    }
  }

  /// A variant of `when` that fallback to an `orElse` callback.
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case _:
  ///     return orElse();
  /// }
  /// ```

  @optionalTypeArgs
  TResult maybeWhen<TResult extends Object?>({
    TResult Function()? pause,
    TResult Function()? resume,
    TResult Function()? toggle,
    TResult Function()? stop,
    TResult Function(BigInt field0)? seekMs,
    TResult Function(SongDto song)? playSong,
    TResult Function(SongDto song, bool next)? queueAdd,
    TResult Function(int index)? queueRemove,
    TResult Function()? queueClear,
    TResult Function()? next,
    TResult Function()? previous,
    TResult Function(String keyword)? search,
    TResult Function(String keyword, int page)? searchMore,
    required TResult orElse(),
  }) {
    final _that = this;
    switch (_that) {
      case FfiCommand_Pause() when pause != null:
        return pause();
      case FfiCommand_Resume() when resume != null:
        return resume();
      case FfiCommand_Toggle() when toggle != null:
        return toggle();
      case FfiCommand_Stop() when stop != null:
        return stop();
      case FfiCommand_SeekMs() when seekMs != null:
        return seekMs(_that.field0);
      case FfiCommand_PlaySong() when playSong != null:
        return playSong(_that.song);
      case FfiCommand_QueueAdd() when queueAdd != null:
        return queueAdd(_that.song, _that.next);
      case FfiCommand_QueueRemove() when queueRemove != null:
        return queueRemove(_that.index);
      case FfiCommand_QueueClear() when queueClear != null:
        return queueClear();
      case FfiCommand_Next() when next != null:
        return next();
      case FfiCommand_Previous() when previous != null:
        return previous();
      case FfiCommand_Search() when search != null:
        return search(_that.keyword);
      case FfiCommand_SearchMore() when searchMore != null:
        return searchMore(_that.keyword, _that.page);
      case _:
        return orElse();
    }
  }

  /// A `switch`-like method, using callbacks.
  ///
  /// As opposed to `map`, this offers destructuring.
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case Subclass2(:final field2):
  ///     return ...;
  /// }
  /// ```

  @optionalTypeArgs
  TResult when<TResult extends Object?>({
    required TResult Function() pause,
    required TResult Function() resume,
    required TResult Function() toggle,
    required TResult Function() stop,
    required TResult Function(BigInt field0) seekMs,
    required TResult Function(SongDto song) playSong,
    required TResult Function(SongDto song, bool next) queueAdd,
    required TResult Function(int index) queueRemove,
    required TResult Function() queueClear,
    required TResult Function() next,
    required TResult Function() previous,
    required TResult Function(String keyword) search,
    required TResult Function(String keyword, int page) searchMore,
  }) {
    final _that = this;
    switch (_that) {
      case FfiCommand_Pause():
        return pause();
      case FfiCommand_Resume():
        return resume();
      case FfiCommand_Toggle():
        return toggle();
      case FfiCommand_Stop():
        return stop();
      case FfiCommand_SeekMs():
        return seekMs(_that.field0);
      case FfiCommand_PlaySong():
        return playSong(_that.song);
      case FfiCommand_QueueAdd():
        return queueAdd(_that.song, _that.next);
      case FfiCommand_QueueRemove():
        return queueRemove(_that.index);
      case FfiCommand_QueueClear():
        return queueClear();
      case FfiCommand_Next():
        return next();
      case FfiCommand_Previous():
        return previous();
      case FfiCommand_Search():
        return search(_that.keyword);
      case FfiCommand_SearchMore():
        return searchMore(_that.keyword, _that.page);
    }
  }

  /// A variant of `when` that fallback to returning `null`
  ///
  /// It is equivalent to doing:
  /// ```dart
  /// switch (sealedClass) {
  ///   case Subclass(:final field):
  ///     return ...;
  ///   case _:
  ///     return null;
  /// }
  /// ```

  @optionalTypeArgs
  TResult? whenOrNull<TResult extends Object?>({
    TResult? Function()? pause,
    TResult? Function()? resume,
    TResult? Function()? toggle,
    TResult? Function()? stop,
    TResult? Function(BigInt field0)? seekMs,
    TResult? Function(SongDto song)? playSong,
    TResult? Function(SongDto song, bool next)? queueAdd,
    TResult? Function(int index)? queueRemove,
    TResult? Function()? queueClear,
    TResult? Function()? next,
    TResult? Function()? previous,
    TResult? Function(String keyword)? search,
    TResult? Function(String keyword, int page)? searchMore,
  }) {
    final _that = this;
    switch (_that) {
      case FfiCommand_Pause() when pause != null:
        return pause();
      case FfiCommand_Resume() when resume != null:
        return resume();
      case FfiCommand_Toggle() when toggle != null:
        return toggle();
      case FfiCommand_Stop() when stop != null:
        return stop();
      case FfiCommand_SeekMs() when seekMs != null:
        return seekMs(_that.field0);
      case FfiCommand_PlaySong() when playSong != null:
        return playSong(_that.song);
      case FfiCommand_QueueAdd() when queueAdd != null:
        return queueAdd(_that.song, _that.next);
      case FfiCommand_QueueRemove() when queueRemove != null:
        return queueRemove(_that.index);
      case FfiCommand_QueueClear() when queueClear != null:
        return queueClear();
      case FfiCommand_Next() when next != null:
        return next();
      case FfiCommand_Previous() when previous != null:
        return previous();
      case FfiCommand_Search() when search != null:
        return search(_that.keyword);
      case FfiCommand_SearchMore() when searchMore != null:
        return searchMore(_that.keyword, _that.page);
      case _:
        return null;
    }
  }
}

/// @nodoc

class FfiCommand_Pause extends FfiCommand {
  const FfiCommand_Pause() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_Pause);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.pause()';
  }
}

/// @nodoc

class FfiCommand_Resume extends FfiCommand {
  const FfiCommand_Resume() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_Resume);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.resume()';
  }
}

/// @nodoc

class FfiCommand_Toggle extends FfiCommand {
  const FfiCommand_Toggle() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_Toggle);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.toggle()';
  }
}

/// @nodoc

class FfiCommand_Stop extends FfiCommand {
  const FfiCommand_Stop() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_Stop);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.stop()';
  }
}

/// @nodoc

class FfiCommand_SeekMs extends FfiCommand {
  const FfiCommand_SeekMs(this.field0) : super._();

  final BigInt field0;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $FfiCommand_SeekMsCopyWith<FfiCommand_SeekMs> get copyWith =>
      _$FfiCommand_SeekMsCopyWithImpl<FfiCommand_SeekMs>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is FfiCommand_SeekMs &&
            (identical(other.field0, field0) || other.field0 == field0));
  }

  @override
  int get hashCode => Object.hash(runtimeType, field0);

  @override
  String toString() {
    return 'FfiCommand.seekMs(field0: $field0)';
  }
}

/// @nodoc
abstract mixin class $FfiCommand_SeekMsCopyWith<$Res>
    implements $FfiCommandCopyWith<$Res> {
  factory $FfiCommand_SeekMsCopyWith(
          FfiCommand_SeekMs value, $Res Function(FfiCommand_SeekMs) _then) =
      _$FfiCommand_SeekMsCopyWithImpl;
  @useResult
  $Res call({BigInt field0});
}

/// @nodoc
class _$FfiCommand_SeekMsCopyWithImpl<$Res>
    implements $FfiCommand_SeekMsCopyWith<$Res> {
  _$FfiCommand_SeekMsCopyWithImpl(this._self, this._then);

  final FfiCommand_SeekMs _self;
  final $Res Function(FfiCommand_SeekMs) _then;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? field0 = null,
  }) {
    return _then(FfiCommand_SeekMs(
      null == field0
          ? _self.field0
          : field0 // ignore: cast_nullable_to_non_nullable
              as BigInt,
    ));
  }
}

/// @nodoc

class FfiCommand_PlaySong extends FfiCommand {
  const FfiCommand_PlaySong({required this.song}) : super._();

  final SongDto song;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $FfiCommand_PlaySongCopyWith<FfiCommand_PlaySong> get copyWith =>
      _$FfiCommand_PlaySongCopyWithImpl<FfiCommand_PlaySong>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is FfiCommand_PlaySong &&
            (identical(other.song, song) || other.song == song));
  }

  @override
  int get hashCode => Object.hash(runtimeType, song);

  @override
  String toString() {
    return 'FfiCommand.playSong(song: $song)';
  }
}

/// @nodoc
abstract mixin class $FfiCommand_PlaySongCopyWith<$Res>
    implements $FfiCommandCopyWith<$Res> {
  factory $FfiCommand_PlaySongCopyWith(
          FfiCommand_PlaySong value, $Res Function(FfiCommand_PlaySong) _then) =
      _$FfiCommand_PlaySongCopyWithImpl;
  @useResult
  $Res call({SongDto song});
}

/// @nodoc
class _$FfiCommand_PlaySongCopyWithImpl<$Res>
    implements $FfiCommand_PlaySongCopyWith<$Res> {
  _$FfiCommand_PlaySongCopyWithImpl(this._self, this._then);

  final FfiCommand_PlaySong _self;
  final $Res Function(FfiCommand_PlaySong) _then;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? song = null,
  }) {
    return _then(FfiCommand_PlaySong(
      song: null == song
          ? _self.song
          : song // ignore: cast_nullable_to_non_nullable
              as SongDto,
    ));
  }
}

/// @nodoc

class FfiCommand_QueueAdd extends FfiCommand {
  const FfiCommand_QueueAdd({required this.song, required this.next})
      : super._();

  final SongDto song;
  final bool next;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $FfiCommand_QueueAddCopyWith<FfiCommand_QueueAdd> get copyWith =>
      _$FfiCommand_QueueAddCopyWithImpl<FfiCommand_QueueAdd>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is FfiCommand_QueueAdd &&
            (identical(other.song, song) || other.song == song) &&
            (identical(other.next, next) || other.next == next));
  }

  @override
  int get hashCode => Object.hash(runtimeType, song, next);

  @override
  String toString() {
    return 'FfiCommand.queueAdd(song: $song, next: $next)';
  }
}

/// @nodoc
abstract mixin class $FfiCommand_QueueAddCopyWith<$Res>
    implements $FfiCommandCopyWith<$Res> {
  factory $FfiCommand_QueueAddCopyWith(
          FfiCommand_QueueAdd value, $Res Function(FfiCommand_QueueAdd) _then) =
      _$FfiCommand_QueueAddCopyWithImpl;
  @useResult
  $Res call({SongDto song, bool next});
}

/// @nodoc
class _$FfiCommand_QueueAddCopyWithImpl<$Res>
    implements $FfiCommand_QueueAddCopyWith<$Res> {
  _$FfiCommand_QueueAddCopyWithImpl(this._self, this._then);

  final FfiCommand_QueueAdd _self;
  final $Res Function(FfiCommand_QueueAdd) _then;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? song = null,
    Object? next = null,
  }) {
    return _then(FfiCommand_QueueAdd(
      song: null == song
          ? _self.song
          : song // ignore: cast_nullable_to_non_nullable
              as SongDto,
      next: null == next
          ? _self.next
          : next // ignore: cast_nullable_to_non_nullable
              as bool,
    ));
  }
}

/// @nodoc

class FfiCommand_QueueRemove extends FfiCommand {
  const FfiCommand_QueueRemove({required this.index}) : super._();

  final int index;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $FfiCommand_QueueRemoveCopyWith<FfiCommand_QueueRemove> get copyWith =>
      _$FfiCommand_QueueRemoveCopyWithImpl<FfiCommand_QueueRemove>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is FfiCommand_QueueRemove &&
            (identical(other.index, index) || other.index == index));
  }

  @override
  int get hashCode => Object.hash(runtimeType, index);

  @override
  String toString() {
    return 'FfiCommand.queueRemove(index: $index)';
  }
}

/// @nodoc
abstract mixin class $FfiCommand_QueueRemoveCopyWith<$Res>
    implements $FfiCommandCopyWith<$Res> {
  factory $FfiCommand_QueueRemoveCopyWith(FfiCommand_QueueRemove value,
          $Res Function(FfiCommand_QueueRemove) _then) =
      _$FfiCommand_QueueRemoveCopyWithImpl;
  @useResult
  $Res call({int index});
}

/// @nodoc
class _$FfiCommand_QueueRemoveCopyWithImpl<$Res>
    implements $FfiCommand_QueueRemoveCopyWith<$Res> {
  _$FfiCommand_QueueRemoveCopyWithImpl(this._self, this._then);

  final FfiCommand_QueueRemove _self;
  final $Res Function(FfiCommand_QueueRemove) _then;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? index = null,
  }) {
    return _then(FfiCommand_QueueRemove(
      index: null == index
          ? _self.index
          : index // ignore: cast_nullable_to_non_nullable
              as int,
    ));
  }
}

/// @nodoc

class FfiCommand_QueueClear extends FfiCommand {
  const FfiCommand_QueueClear() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_QueueClear);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.queueClear()';
  }
}

/// @nodoc

class FfiCommand_Next extends FfiCommand {
  const FfiCommand_Next() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_Next);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.next()';
  }
}

/// @nodoc

class FfiCommand_Previous extends FfiCommand {
  const FfiCommand_Previous() : super._();

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType && other is FfiCommand_Previous);
  }

  @override
  int get hashCode => runtimeType.hashCode;

  @override
  String toString() {
    return 'FfiCommand.previous()';
  }
}

/// @nodoc

class FfiCommand_Search extends FfiCommand {
  const FfiCommand_Search({required this.keyword}) : super._();

  final String keyword;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $FfiCommand_SearchCopyWith<FfiCommand_Search> get copyWith =>
      _$FfiCommand_SearchCopyWithImpl<FfiCommand_Search>(this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is FfiCommand_Search &&
            (identical(other.keyword, keyword) || other.keyword == keyword));
  }

  @override
  int get hashCode => Object.hash(runtimeType, keyword);

  @override
  String toString() {
    return 'FfiCommand.search(keyword: $keyword)';
  }
}

/// @nodoc
abstract mixin class $FfiCommand_SearchCopyWith<$Res>
    implements $FfiCommandCopyWith<$Res> {
  factory $FfiCommand_SearchCopyWith(
          FfiCommand_Search value, $Res Function(FfiCommand_Search) _then) =
      _$FfiCommand_SearchCopyWithImpl;
  @useResult
  $Res call({String keyword});
}

/// @nodoc
class _$FfiCommand_SearchCopyWithImpl<$Res>
    implements $FfiCommand_SearchCopyWith<$Res> {
  _$FfiCommand_SearchCopyWithImpl(this._self, this._then);

  final FfiCommand_Search _self;
  final $Res Function(FfiCommand_Search) _then;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? keyword = null,
  }) {
    return _then(FfiCommand_Search(
      keyword: null == keyword
          ? _self.keyword
          : keyword // ignore: cast_nullable_to_non_nullable
              as String,
    ));
  }
}

/// @nodoc

class FfiCommand_SearchMore extends FfiCommand {
  const FfiCommand_SearchMore({required this.keyword, required this.page})
      : super._();

  final String keyword;
  final int page;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @JsonKey(includeFromJson: false, includeToJson: false)
  @pragma('vm:prefer-inline')
  $FfiCommand_SearchMoreCopyWith<FfiCommand_SearchMore> get copyWith =>
      _$FfiCommand_SearchMoreCopyWithImpl<FfiCommand_SearchMore>(
          this, _$identity);

  @override
  bool operator ==(Object other) {
    return identical(this, other) ||
        (other.runtimeType == runtimeType &&
            other is FfiCommand_SearchMore &&
            (identical(other.keyword, keyword) || other.keyword == keyword) &&
            (identical(other.page, page) || other.page == page));
  }

  @override
  int get hashCode => Object.hash(runtimeType, keyword, page);

  @override
  String toString() {
    return 'FfiCommand.searchMore(keyword: $keyword, page: $page)';
  }
}

/// @nodoc
abstract mixin class $FfiCommand_SearchMoreCopyWith<$Res>
    implements $FfiCommandCopyWith<$Res> {
  factory $FfiCommand_SearchMoreCopyWith(FfiCommand_SearchMore value,
          $Res Function(FfiCommand_SearchMore) _then) =
      _$FfiCommand_SearchMoreCopyWithImpl;
  @useResult
  $Res call({String keyword, int page});
}

/// @nodoc
class _$FfiCommand_SearchMoreCopyWithImpl<$Res>
    implements $FfiCommand_SearchMoreCopyWith<$Res> {
  _$FfiCommand_SearchMoreCopyWithImpl(this._self, this._then);

  final FfiCommand_SearchMore _self;
  final $Res Function(FfiCommand_SearchMore) _then;

  /// Create a copy of FfiCommand
  /// with the given fields replaced by the non-null parameter values.
  @pragma('vm:prefer-inline')
  $Res call({
    Object? keyword = null,
    Object? page = null,
  }) {
    return _then(FfiCommand_SearchMore(
      keyword: null == keyword
          ? _self.keyword
          : keyword // ignore: cast_nullable_to_non_nullable
              as String,
      page: null == page
          ? _self.page
          : page // ignore: cast_nullable_to_non_nullable
              as int,
    ));
  }
}

// dart format on
