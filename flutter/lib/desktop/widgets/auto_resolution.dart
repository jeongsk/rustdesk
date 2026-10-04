import 'dart:async';
import 'dart:math';

import 'package:flutter/material.dart';
import 'package:get/get.dart';

import '../../consts.dart';
import '../../models/model.dart';
import '../../models/platform_model.dart';

const String kOptionAutoFitResolution = 'auto-fit-resolution';

/// Keeps the remote display resolution matched to the local view size.
///
/// When enabled for a peer, the controller picks the remote resolution that
/// best fills the current view whenever the view is resized (including
/// entering or leaving full screen) or the remote display info changes.
/// The original resolution is restored by the controlled side when the
/// session ends.
class AutoResolutionController {
  final FFI ffi;
  final enabled = false.obs;

  static const _debounce = Duration(milliseconds: 800);

  Timer? _timer;
  Size? _viewSize;
  // The last request, used to avoid re-sending a request the peer could not
  // apply (e.g. no matching mode) on every display info notification.
  String? _lastRequestKey;
  bool _disposed = false;

  AutoResolutionController(this.ffi) {
    ffi.ffiModel.addListener(_onModelChanged);
    _loadOption();
  }

  static String _tag(String id) => 'auto-resolution-$id';

  static AutoResolutionController? find(String id) {
    final tag = _tag(id);
    return Get.isRegistered<AutoResolutionController>(tag: tag)
        ? Get.find<AutoResolutionController>(tag: tag)
        : null;
  }

  static AutoResolutionController put(String id, FFI ffi) =>
      Get.put<AutoResolutionController>(AutoResolutionController(ffi),
          tag: _tag(id));

  static Future<void> delete(String id) async {
    find(id)?.dispose();
    await Get.delete<AutoResolutionController>(tag: _tag(id));
  }

  Future<void> _loadOption() async {
    final v = await bind.sessionGetOption(
        sessionId: ffi.sessionId, arg: kOptionAutoFitResolution);
    if (_disposed) return;
    enabled.value = v == 'Y';
    _schedule();
  }

  Future<void> setEnabled(bool v) async {
    enabled.value = v;
    _lastRequestKey = null;
    await bind.sessionPeerOption(
        sessionId: ffi.sessionId,
        name: kOptionAutoFitResolution,
        value: v ? 'Y' : '');
    _schedule();
  }

  void onViewSizeChanged(Size size) {
    if (_viewSize == size) return;
    _viewSize = size;
    _lastRequestKey = null;
    _schedule();
  }

  // FfiModel notifies often; don't let that keep pushing the debounce back.
  void _onModelChanged() {
    if (_timer?.isActive ?? false) return;
    _schedule();
  }

  void _schedule() {
    if (_disposed || !enabled.value) return;
    _timer?.cancel();
    _timer = Timer(_debounce, _apply);
  }

  Future<void> _apply() async {
    if (_disposed || !enabled.value) return;
    final view = _viewSize;
    if (view == null || view.width < 1 || view.height < 1) return;
    final ffiModel = ffi.ffiModel;
    final pi = ffiModel.pi;
    if (!pi.isSet.value || !ffiModel.keyboard) return;
    if (ffi.connType != ConnType.defaultConn) return;
    if (pi.currentDisplay == kAllDisplayValue) return;
    final display = pi.tryGetDisplayIfNotAllDisplay();
    if (display == null) return;

    final target = bestFitResolution(
      view.width.floor(),
      view.height.floor(),
      pi.resolutions,
      isVirtualDisplay: display.isVirtualDisplayResolution,
    );
    if (target == null) return;

    // `Display.width/height` are physical pixels, resolutions are logical.
    final curW = (display.width / display.scale).round();
    final curH = (display.height / display.scale).round();
    if (target.width == curW && target.height == curH) return;

    final key = '${pi.currentDisplay}:${target.width}x${target.height}';
    if (key == _lastRequestKey) return;
    _lastRequestKey = key;
    debugPrint('Auto-fit resolution: view ${view.width}x${view.height}, '
        'current ${curW}x$curH, request ${target.width}x${target.height}');
    await bind.sessionChangeResolution(
      sessionId: ffi.sessionId,
      display: pi.currentDisplay,
      width: target.width,
      height: target.height,
    );
  }

  void dispose() {
    _disposed = true;
    _timer?.cancel();
    ffi.ffiModel.removeListener(_onModelChanged);
  }
}

/// Pick the resolution that best fills a [viewW] x [viewH] view.
///
/// A virtual display accepts any size, so the view size itself is used.
/// Otherwise the largest supported mode that fits inside the view wins,
/// preferring the closest aspect ratio among modes of similar area, so the
/// remote screen is shown 1:1 without scrolling. If no mode fits, the
/// smallest one is used. Returns null when there is nothing to choose from.
Resolution? bestFitResolution(
  int viewW,
  int viewH,
  List<Resolution> resolutions, {
  bool isVirtualDisplay = false,
}) {
  if (viewW <= 0 || viewH <= 0) return null;
  if (isVirtualDisplay) {
    // Even sizes are friendlier to video encoders.
    return Resolution(viewW & ~1, viewH & ~1);
  }
  if (resolutions.length < 2) return null;

  final viewAspect = viewW / viewH;
  double score(Resolution r) {
    final fill = (r.width * r.height) / (viewW * viewH);
    final aspectDiff = (log((r.width / r.height) / viewAspect)).abs();
    return fill - aspectDiff;
  }

  final fitting = resolutions
      .where((r) =>
          r.width > 0 && r.height > 0 && r.width <= viewW && r.height <= viewH)
      .toList();
  if (fitting.isNotEmpty) {
    return fitting.reduce((a, b) => score(b) > score(a) ? b : a);
  }
  final valid = resolutions.where((r) => r.width > 0 && r.height > 0);
  if (valid.isEmpty) return null;
  return valid
      .reduce((a, b) => b.width * b.height < a.width * a.height ? b : a);
}
