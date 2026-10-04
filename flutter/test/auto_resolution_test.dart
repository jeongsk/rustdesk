import 'package:flutter_hbb/desktop/widgets/auto_resolution.dart';
import 'package:flutter_hbb/models/model.dart';
import 'package:flutter_test/flutter_test.dart';

String? _fmt(Resolution? r) => r == null ? null : '${r.width}x${r.height}';

void main() {
  // Typical HiDPI modes of a headless Mac mini.
  final macModes = [
    Resolution(2560, 1440),
    Resolution(1920, 1080),
    Resolution(1680, 1050),
    Resolution(1512, 982),
    Resolution(1440, 900),
    Resolution(1280, 800),
    Resolution(1280, 720),
    Resolution(1024, 768),
  ];

  test('picks the exact mode when it fits the view', () {
    expect(_fmt(bestFitResolution(1512, 982, macModes)), '1512x982');
  });

  test('picks the largest mode that fits inside the view', () {
    expect(_fmt(bestFitResolution(1500, 940, macModes)), '1440x900');
  });

  test('ignores modes taller than the view', () {
    // 1280x800 is wider-fitting but too tall for a 16:9 view.
    expect(_fmt(bestFitResolution(1300, 740, macModes)), '1280x720');
  });

  test('falls back to the smallest mode when nothing fits', () {
    expect(_fmt(bestFitResolution(800, 500, macModes)), '1024x768');
  });

  test('uses the view size on a virtual display', () {
    expect(
        _fmt(bestFitResolution(1513, 981, const [], isVirtualDisplay: true)),
        '1512x980');
  });

  test('returns null without a choice or a view', () {
    expect(bestFitResolution(1512, 982, [Resolution(1920, 1080)]), isNull);
    expect(bestFitResolution(0, 982, macModes), isNull);
  });
}
